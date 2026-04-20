//! Steps 10-12: Biblical theology arcs, systematic theology doctrine classifier,
//! and practical theology applications.

use std::collections::HashMap;
use std::sync::OnceLock;

use async_graphql::dynamic::*;
use serde::Deserialize;
use fw_decode::theology::explain_doctrine;
use fw_graph_types::PgValue;

use crate::executor::RequestConnection;
use super::structs::*;
use super::pericope::{build_pericope_partial, PericopeCardPartial};

// ── Doctrine classifier config ───────────────────────────────────────────────

#[derive(Deserialize)]
struct DoctrineEntryConfig {
    label: String,
    lemmas: Vec<String>,
    #[serde(default)]
    keywords: Vec<String>,
}

#[derive(Deserialize)]
struct DoctrineConfig {
    #[serde(flatten)]
    doctrines: HashMap<String, DoctrineEntryConfig>,
}

struct DoctrineMap {
    doctrines: Vec<(String, String, Vec<String>, Vec<String>)>, // key, label, lemmas, keywords
    lemma_index: HashMap<String, Vec<usize>>,
}

static DOCTRINE_MAP: OnceLock<DoctrineMap> = OnceLock::new();
const CONFIG_TOML: &str = include_str!("../../../../../../config/doctrine-lemmas.toml");

fn get_doctrine_map() -> &'static DoctrineMap {
    DOCTRINE_MAP.get_or_init(|| {
        let config: DoctrineConfig = toml::from_str(CONFIG_TOML).expect("doctrine-lemmas.toml parse error");
        let mut doctrines = Vec::new();
        let mut lemma_index: HashMap<String, Vec<usize>> = HashMap::new();
        for (key, entry) in &config.doctrines {
            let idx = doctrines.len();
            doctrines.push((key.clone(), entry.label.clone(), entry.lemmas.clone(), entry.keywords.clone()));
            for lemma in &entry.lemmas {
                lemma_index.entry(lemma.clone()).or_default().push(idx);
            }
        }
        DoctrineMap { doctrines, lemma_index }
    })
}

// ── Column helpers ───────────────────────────────────────────────────────────

fn text_col(row: &fw_graph_types::PgRow, col: &str) -> String {
    match row.get(col) {
        Some(PgValue::Text(s)) => s.clone(),
        _ => String::new(),
    }
}

fn opt_text_col(row: &fw_graph_types::PgRow, col: &str) -> Option<String> {
    match row.get(col) {
        Some(PgValue::Text(s)) => Some(s.clone()),
        _ => None,
    }
}

// ── Step 10: Biblical Theology ────────────────────────────────────────────────

/// Build biblical theology arcs for a given NT book.
///
/// Query priority:
/// 1. `ot_in_nt_quotations` — Beale-Carson authoritative OT-in-NT catalog.
///    Arcs are grouped into three themed sections by quotation type.
/// 2. `commentary_cross_references` — general commentary xrefs as fallback
///    when Beale-Carson data is not yet loaded for this book.
pub async fn fetch_biblical_theology(
    conn: &RequestConnection,
    book: &str,
) -> Result<Option<BiblicalTheologyOutput>, async_graphql::Error> {
    // ── Primary: Beale-Carson ot_in_nt_quotations ────────────────────────────
    let bc_sql = r#"
SELECT oinnt.nt_ref, oinnt.ot_ref, oinnt.quotation_type, oinnt.source_form,
       oinnt.strength, oinnt.significance
FROM ot_in_nt_quotations oinnt
WHERE oinnt.nt_book = $1
ORDER BY oinnt.nt_ref, oinnt.ot_ref
LIMIT 200
"#;
    let bc_rows = conn.execute(bc_sql, &[PgValue::Text(book.to_owned())])
        .await.unwrap_or_default();

    if !bc_rows.is_empty() {
        let mut quotations: Vec<ArcLink> = Vec::new();
        let mut allusions:  Vec<ArcLink> = Vec::new();
        let mut echoes:     Vec<ArcLink> = Vec::new();

        for row in &bc_rows {
            let nt_ref       = text_col(row, "nt_ref");
            let ot_ref       = text_col(row, "ot_ref");
            let qtype        = text_col(row, "quotation_type");
            let src_form     = text_col(row, "source_form");
            let significance = opt_text_col(row, "significance");

            let link_type = match qtype.as_str() {
                "direct_quotation" => "canonical_reinterpretation",
                "allusion"         => "intertextual_allusion",
                "echo"             => "lexical_echo",
                _                  => "lexical_echo",
            }.to_string();

            let ot_book = ot_ref.split('.').next().unwrap_or("");
            let direction = fw_decode::theology::arc_direction(book, ot_book).to_string();

            // link_type_explained: use significance when available, else generic label
            let link_type_explained = significance
                .clone()
                .unwrap_or_else(|| fw_decode::theology::explain_arc(&link_type, &direction).to_string());

            let arc = ArcLink {
                r#ref: ot_ref,
                link_type,
                link_type_explained,
                direction,
                shared_lemma: Some(nt_ref),   // NT verse that cites this OT ref
                concept: if src_form.is_empty() || src_form == "unknown" {
                    None
                } else {
                    Some(src_form)             // text form: "MT", "LXX", etc.
                },
            };

            match qtype.as_str() {
                "direct_quotation" => quotations.push(arc),
                "allusion"         => allusions.push(arc),
                _                  => echoes.push(arc),
            }
        }

        let mut themes = Vec::new();
        if !quotations.is_empty() {
            themes.push(ThemeArc { theme: "Direct Quotations".to_string(), arc: quotations });
        }
        if !allusions.is_empty() {
            themes.push(ThemeArc { theme: "Allusions".to_string(), arc: allusions });
        }
        if !echoes.is_empty() {
            themes.push(ThemeArc { theme: "Echoes".to_string(), arc: echoes });
        }

        return Ok(Some(BiblicalTheologyOutput { themes, synthesis: None }));
    }

    // ── Fallback: commentary_cross_references ────────────────────────────────
    let xref_sql = r#"
SELECT ccr.target_ref, ccr.relationship
FROM commentary_cross_references ccr
WHERE ccr.book = $1
ORDER BY ccr.target_ref
LIMIT 30
"#;
    let rows = conn.execute(xref_sql, &[PgValue::Text(book.to_owned())])
        .await.unwrap_or_default();

    if rows.is_empty() {
        return Ok(None);
    }

    let arcs: Vec<ArcLink> = rows.iter().map(|row| {
        let target_ref = text_col(row, "target_ref");
        let relationship = text_col(row, "relationship");
        let link_type = normalize_relationship(&relationship);
        let direction = fw_decode::theology::arc_direction(book, &target_ref
            .split('.').next().unwrap_or("")).to_string();
        let link_type_explained = fw_decode::theology::explain_arc(&link_type, &direction).to_string();
        ArcLink {
            r#ref: target_ref,
            link_type,
            link_type_explained,
            direction,
            shared_lemma: None,
            concept: None,
        }
    }).collect();

    Ok(Some(BiblicalTheologyOutput {
        themes: vec![ThemeArc { theme: "Cross-references".to_string(), arc: arcs }],
        synthesis: None,
    }))
}

fn normalize_relationship(rel: &str) -> String {
    match rel.to_lowercase().as_str() {
        "fulfillment" | "fulfills" => "typological_fulfillment".to_string(),
        "quotation" | "quote" | "ot_quotation" => "canonical_reinterpretation".to_string(),
        "allusion" | "echo" => "intertextual_allusion".to_string(),
        "parallel" | "lexical" => "lexical_echo".to_string(),
        "concept" | "conceptual" => "concept_bridge".to_string(),
        "cross_testament" | "testament" => "cross_testament_bridge".to_string(),
        _ => "lexical_echo".to_string(),
    }
}

#[derive(Clone)]
pub struct BiblicalTheologyOutput {
    pub themes: Vec<ThemeArc>,
    pub synthesis: Option<String>,
}

// ── Step 11: Systematic Theology ──────────────────────────────────────────────

/// Classify doctrinal loci via lemma density scoring.
/// Uses passage_tokens (JOINed to passages) + word study keyword scan.
pub async fn classify_doctrine_step(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> Result<Option<SystematicTheologyOutput>, async_graphql::Error> {
    let map = get_doctrine_map();
    if map.doctrines.is_empty() {
        return Ok(None);
    }

    // Phase 1: Strong's number density in passage_tokens (via JOIN passages)
    let strong_sql = r#"
SELECT pt.strong_number, COUNT(*) AS cnt
FROM passage_tokens pt
JOIN passages p ON p.id = pt.passage_id
WHERE p.book = $1 AND p.chapter = $2
  AND p.verse >= $3 AND p.verse <= $4
  AND pt.strong_number IS NOT NULL AND pt.strong_number != ''
GROUP BY pt.strong_number
"#;
    let strong_rows = conn.execute(strong_sql, &[
        PgValue::Text(book.to_owned()),
        PgValue::Int(chapter),
        PgValue::Int(verse_start),
        PgValue::Int(verse_end),
    ]).await.unwrap_or_default();

    let mut scores = vec![0.0f64; map.doctrines.len()];
    for row in &strong_rows {
        let strong = text_col(row, "strong_number");
        let count = match row.get("cnt") {
            Some(PgValue::Int(n)) => *n as f64,
            _ => 1.0,
        };
        if let Some(doctrine_indices) = map.lemma_index.get(&strong) {
            for &idx in doctrine_indices {
                scores[idx] += count;
            }
        }
    }

    // Phase 2: Keyword scan in word study content
    let kw_sql = r#"
SELECT LOWER(COALESCE(editor_summary, LEFT(issue_description, 400))) AS content
FROM word_study_entries WHERE book = $1
"#;
    let kw_rows = conn.execute(kw_sql, &[PgValue::Text(book.to_owned())])
        .await.unwrap_or_default();

    for row in &kw_rows {
        let content = text_col(row, "content");
        for (idx, (_, _, _, keywords)) in map.doctrines.iter().enumerate() {
            for kw in keywords {
                if content.contains(kw.as_str()) {
                    scores[idx] += 3.0;
                }
            }
        }
    }

    // Phase 3: Normalize by lemma set size
    for (idx, (_, _, lemmas, _)) in map.doctrines.iter().enumerate() {
        if !lemmas.is_empty() {
            scores[idx] /= lemmas.len() as f64;
        }
    }

    // Phase 4: Take top 1-2 doctrines
    let mut scored: Vec<(usize, f64)> = scores.iter().enumerate()
        .filter(|(_, &s)| s > 0.0)
        .map(|(i, &s)| (i, s))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if scored.is_empty() {
        return Ok(None);
    }

    let top_score = scored[0].1;
    let total: f64 = scored.iter().map(|(_, s)| s).sum();
    let confidence = if total > 0.0 { Some(top_score / total) } else { None };

    let mut doctrine_entries = vec![make_doctrine_entry(&map.doctrines[scored[0].0])];
    if scored.len() > 1 && scored[1].1 >= top_score * 0.5 {
        doctrine_entries.push(make_doctrine_entry(&map.doctrines[scored[1].0]));
    }

    Ok(Some(SystematicTheologyOutput { loci: doctrine_entries, content: None, confidence }))
}

fn make_doctrine_entry(entry: &(String, String, Vec<String>, Vec<String>)) -> DoctrineEntry {
    let (locus, label, _, _) = entry;
    let explained = explain_doctrine(locus, "").to_string();
    DoctrineEntry { locus: locus.clone(), label: label.clone(), explained }
}

#[derive(Clone)]
pub struct SystematicTheologyOutput {
    pub loci: Vec<DoctrineEntry>,
    pub content: Option<String>,
    pub confidence: Option<f64>,
}

// ── Step 12: Practical Theology ───────────────────────────────────────────────

pub async fn fetch_practical_theology(
    conn: &RequestConnection,
    book: &str,
) -> Result<Option<Vec<ApplicationEntry>>, async_graphql::Error> {
    let sql = r#"
SELECT
  ca.content,
  COALESCE(cs.series_name, 'unknown') AS series
FROM commentary_applications ca
LEFT JOIN commentary_sources cs ON ca.source_id = cs.id
WHERE ca.book = $1
ORDER BY ca.verse_ref
LIMIT 20
"#;
    let rows = conn.execute(sql, &[PgValue::Text(book.to_owned())])
        .await.unwrap_or_default();

    if rows.is_empty() {
        return Ok(None);
    }

    let entries: Vec<ApplicationEntry> = rows.iter().map(|row| ApplicationEntry {
        content: text_col(row, "content"),
        series: text_col(row, "series"),
        lexicon_refs: vec![],
    }).collect();

    Ok(Some(entries))
}

// ── Full Pericope Card output ─────────────────────────────────────────────────

#[derive(Clone)]
pub struct PericopeCardFull {
    pub partial: PericopeCardPartial,
    pub biblical_theology: Option<BiblicalTheologyOutput>,
    pub systematic_theology: Option<SystematicTheologyOutput>,
    pub practical_theology: Vec<ApplicationEntry>,
}

pub async fn build_pericope_full(
    conn: &RequestConnection,
    book: &str,
    pericope_ref: &str,
    title: &str,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> PericopeCardFull {
    let (partial, bt, st, pt) = tokio::join!(
        build_pericope_partial(conn, book, pericope_ref, title),
        fetch_biblical_theology(conn, book),
        classify_doctrine_step(conn, book, chapter, verse_start, verse_end),
        fetch_practical_theology(conn, book),
    );

    PericopeCardFull {
        partial,
        biblical_theology: bt.unwrap_or(None),
        systematic_theology: st.unwrap_or(None),
        practical_theology: pt.unwrap_or(None).unwrap_or_default(),
    }
}

// ── GraphQL Type Registration ─────────────────────────────────────────────────

pub fn register_theology_types(builder: SchemaBuilder) -> SchemaBuilder {
    let doctrine_entry = Object::new("TocmaDoctrineEntry")
        .field(Field::new("locus",     TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DoctrineEntry>()?;
                Ok(Some(FieldValue::value(v.locus.clone())))
            })
        }))
        .field(Field::new("label",     TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DoctrineEntry>()?;
                Ok(Some(FieldValue::value(v.label.clone())))
            })
        }))
        .field(Field::new("explained", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DoctrineEntry>()?;
                Ok(Some(FieldValue::value(v.explained.clone())))
            })
        }));

    let arc_link = Object::new("TocmaArcLink")
        .field(Field::new("ref",              TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ArcLink>()?;
                Ok(Some(FieldValue::value(v.r#ref.clone())))
            })
        }))
        .field(Field::new("linkType",         TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ArcLink>()?;
                Ok(Some(FieldValue::value(v.link_type.clone())))
            })
        }))
        .field(Field::new("linkTypeExplained", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ArcLink>()?;
                Ok(Some(FieldValue::value(v.link_type_explained.clone())))
            })
        }))
        .field(Field::new("direction",        TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ArcLink>()?;
                Ok(Some(FieldValue::value(v.direction.clone())))
            })
        }))
        .field(Field::new("sharedLemma",      TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ArcLink>()?;
                Ok(v.shared_lemma.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("concept",          TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ArcLink>()?;
                Ok(v.concept.clone().map(FieldValue::value))
            })
        }));

    let theme_arc = Object::new("TocmaThemeArc")
        .field(Field::new("theme", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ThemeArc>()?;
                Ok(Some(FieldValue::value(v.theme.clone())))
            })
        }))
        .field(Field::new("arc",   TypeRef::named_nn_list_nn("TocmaArcLink"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ThemeArc>()?;
                let values: Vec<FieldValue> = v.arc.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }));

    let application_entry = Object::new("TocmaApplicationEntry")
        .field(Field::new("content", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ApplicationEntry>()?;
                Ok(Some(FieldValue::value(v.content.clone())))
            })
        }))
        .field(Field::new("series",  TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ApplicationEntry>()?;
                Ok(Some(FieldValue::value(v.series.clone())))
            })
        }));

    // BiblicalTheologyStep
    let bt_step = Object::new("TocmaBiblicalTheologyStep")
        .field(Field::new("themes",    TypeRef::named_nn_list_nn("TocmaThemeArc"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<BiblicalTheologyOutput>()?;
                let values: Vec<FieldValue> = v.themes.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("synthesis", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<BiblicalTheologyOutput>()?;
                Ok(v.synthesis.clone().map(FieldValue::value))
            })
        }));

    // SystematicTheologyStep
    let st_step = Object::new("TocmaSystematicTheologyStep")
        .field(Field::new("loci",       TypeRef::named_nn_list_nn("TocmaDoctrineEntry"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<SystematicTheologyOutput>()?;
                let values: Vec<FieldValue> = v.loci.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("content",    TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<SystematicTheologyOutput>()?;
                Ok(v.content.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("confidence", TypeRef::named(TypeRef::FLOAT), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<SystematicTheologyOutput>()?;
                Ok(v.confidence.map(FieldValue::value))
            })
        }));

    // Full PericopeCardFull type
    let pericope_full = Object::new("TocmaPericopeCardFull")
        .field(Field::new("ref",                TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardFull>()?;
                Ok(Some(FieldValue::value(v.partial.r#ref.clone())))
            })
        }))
        .field(Field::new("title",              TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardFull>()?;
                Ok(Some(FieldValue::value(v.partial.title.clone())))
            })
        }))
        .field(Field::new("wordStudies",        TypeRef::named_nn_list_nn("TocmaWordStudyEntry"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardFull>()?;
                let values: Vec<FieldValue> = v.partial.word_studies.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("discourseNotes",     TypeRef::named_nn_list_nn("TocmaDiscourseNote"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardFull>()?;
                let values: Vec<FieldValue> = v.partial.discourse_notes.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("historicalNotes",    TypeRef::named_nn_list_nn("TocmaCommentaryNote"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardFull>()?;
                let values: Vec<FieldValue> = v.partial.historical_notes.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("intertextualRefs",   TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardFull>()?;
                let values: Vec<FieldValue> = v.partial.intertextual_refs.iter().cloned().map(FieldValue::value).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("biblicalTheology",   TypeRef::named("TocmaBiblicalTheologyStep"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardFull>()?;
                Ok(v.biblical_theology.clone().map(FieldValue::owned_any))
            })
        }))
        .field(Field::new("systematicTheology", TypeRef::named("TocmaSystematicTheologyStep"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardFull>()?;
                Ok(v.systematic_theology.clone().map(FieldValue::owned_any))
            })
        }))
        .field(Field::new("applications",       TypeRef::named_nn_list_nn("TocmaApplicationEntry"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardFull>()?;
                let values: Vec<FieldValue> = v.practical_theology.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }));

    builder
        .register(doctrine_entry)
        .register(arc_link)
        .register(theme_arc)
        .register(application_entry)
        .register(bt_step)
        .register(st_step)
        .register(pericope_full)
}

// ── Root query field ─────────────────────────────────────────────────────────

/// `tocmaPericopeFull(book: String!, pericopeRef: String!, chapter: Int!, verseStart: Int!, verseEnd: Int!, title: String): TocmaPericopeCardFull`
pub fn tocma_pericope_full_field() -> Field {
    Field::new(
        "tocmaPericopeFull",
        TypeRef::named("TocmaPericopeCardFull"),
        |ctx| {
            FieldFuture::new(async move {
                let conn = ctx
                    .data_opt::<RequestConnection>()
                    .ok_or_else(|| async_graphql::Error::new("No database connection"))?;
                let book = ctx.args.try_get("book")?.string()
                    .map_err(|_| async_graphql::Error::new("book must be a string"))?
                    .to_owned();
                let pericope_ref = ctx.args.try_get("pericopeRef")?.string()
                    .map_err(|_| async_graphql::Error::new("pericopeRef must be a string"))?
                    .to_owned();
                let chapter = ctx.args.try_get("chapter")?.i64()
                    .map_err(|_| async_graphql::Error::new("chapter must be an int"))?;
                let verse_start = ctx.args.try_get("verseStart")?.i64()
                    .map_err(|_| async_graphql::Error::new("verseStart must be an int"))?;
                let verse_end = ctx.args.try_get("verseEnd")?.i64()
                    .map_err(|_| async_graphql::Error::new("verseEnd must be an int"))?;
                let title = ctx.args.try_get("title").ok()
                    .and_then(|v| v.string().ok().map(|s| s.to_owned()))
                    .unwrap_or_default();

                let card = build_pericope_full(
                    conn, &book, &pericope_ref, &title,
                    chapter, verse_start, verse_end,
                ).await;

                Ok(Some(FieldValue::owned_any(card)))
            })
        },
    )
    .argument(InputValue::new("book",        TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("pericopeRef", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter",     TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("verseStart",  TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("verseEnd",    TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("title",       TypeRef::named(TypeRef::STRING)))
}
