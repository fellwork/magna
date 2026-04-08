//! Steps 6-9 resolver: argument tracing, word studies, historical context, literary context.

use async_graphql::dynamic::*;
use fw_decode::scan_text_for_refs;
use fw_graph_types::PgValue;

use crate::executor::RequestConnection;
use super::structs::*;

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
        Some(PgValue::Null) | None => None,
        _ => None,
    }
}

fn float_col(row: &fw_graph_types::PgRow, col: &str) -> f64 {
    match row.get(col) {
        Some(PgValue::Float(f)) => *f,
        Some(PgValue::Int(n)) => *n as f64,
        _ => 0.0,
    }
}

// ── SQL Fetch Functions ──────────────────────────────────────────────────────

/// Step 6: Fetch discourse notes + role labels for a pericope.
pub async fn fetch_argument_tracing(
    conn: &RequestConnection,
    book: &str,
    pericope_ref: &str,
) -> Result<(Vec<DiscourseNote>, Vec<String>), async_graphql::Error> {
    let _ = pericope_ref; // used for scoping in future; current query is book-level

    let discourse_sql = r#"
SELECT
  cdn.description AS note,
  COALESCE(cs.series_name, 'unknown') AS series
FROM commentary_discourse_notes cdn
LEFT JOIN commentary_sources cs ON cdn.source_id = cs.id
WHERE cdn.book = $1
ORDER BY cdn.verse_ref
"#;
    let disc_rows = conn.execute(discourse_sql, &[PgValue::Text(book.to_owned())])
        .await.map_err(|e| async_graphql::Error::new(format!("discourse_notes query: {e}")))?;

    let discourse_notes: Vec<DiscourseNote> = disc_rows.iter().map(|row| DiscourseNote {
        note: text_col(row, "note"),
        series: text_col(row, "series"),
    }).collect();

    // Role labels from phrase_structure_nodes (discourse connectives)
    let role_sql = r#"
SELECT DISTINCT role_label
FROM phrase_structure_nodes
WHERE book = $1
  AND role_label IN ('Ground','Inference','Contrast','Purpose','Result','Concessive','Causal','Condition')
ORDER BY role_label
"#;
    let role_rows = conn.execute(role_sql, &[PgValue::Text(book.to_owned())])
        .await.unwrap_or_default();

    let role_labels: Vec<String> = role_rows.iter().map(|row| text_col(row, "role_label")).collect();

    Ok((discourse_notes, role_labels))
}

/// Step 7: Fetch word studies for a book, grouped by lemma.
pub async fn fetch_word_studies(
    conn: &RequestConnection,
    book: &str,
) -> Result<Vec<WordStudyEntry>, async_graphql::Error> {
    let sql = r#"
SELECT
  w.greek_term AS lemma,
  w.transliteration,
  COALESCE(vi.strong_number, '') AS strong,
  COALESCE(vi.short_gloss, '') AS gloss,
  COALESCE(vi.semantic_domain, '') AS semantic_domain,
  COALESCE(vi.frequency_count, 0) AS frequency_count,
  COALESCE(w.editor_summary, LEFT(w.issue_description, 400)) AS content,
  COALESCE(cs.series_name, 'unknown') AS series
FROM word_study_entries w
LEFT JOIN commentary_sources cs ON w.source_id = cs.id
LEFT JOIN vocabulary_items vi ON vi.id = w.vocabulary_item_id
WHERE w.book = $1
ORDER BY w.greek_term
"#;
    let rows = conn.execute(sql, &[PgValue::Text(book.to_owned())])
        .await.map_err(|e| async_graphql::Error::new(format!("word_study_entries query: {e}")))?;

    // Group by lemma
    let mut by_lemma: std::collections::BTreeMap<String, WordStudyEntry> =
        std::collections::BTreeMap::new();

    for row in &rows {
        let lemma = text_col(row, "lemma");
        let content = text_col(row, "content");
        let series = text_col(row, "series");

        let lexicon_refs: Vec<FoundLexiconRef> = scan_text_for_refs(&content)
            .into_iter()
            .map(|r| FoundLexiconRef {
                raw: r.raw,
                abbreviation: r.decoded.abbreviation,
                full_title: r.decoded.full_title.to_string(),
                page_or_section: if r.decoded.page_or_section.is_empty() {
                    None
                } else {
                    Some(r.decoded.page_or_section)
                },
                description: r.decoded.description.to_string(),
            })
            .collect();

        let entry = by_lemma.entry(lemma.clone()).or_insert_with(|| {
            let sem_domain = opt_text_col(row, "semantic_domain");
            let domain_name = sem_domain.as_deref().and_then(|d| {
                let first = d.split(',').next().unwrap_or(d).trim();
                let major = first.split('.').next().unwrap_or(first);
                let name = fw_decode::theology::domain_name(major);
                if name.is_empty() { None } else { Some(name.to_string()) }
            });

            let freq = float_col(row, "frequency_count").max(1.0);
            let rarity = (1.0 - (freq.ln() / 10.0_f64.ln())).clamp(0.0, 1.0) as f32;

            WordStudyEntry {
                lemma: lemma.clone(),
                transliteration: text_col(row, "transliteration"),
                strong: text_col(row, "strong"),
                gloss: text_col(row, "gloss"),
                significance: rarity,
                louw_nida_domain: sem_domain,
                louw_nida_domain_name: domain_name,
                studies: Vec::new(),
                lexicon_refs: Vec::new(),
            }
        });

        entry.studies.push(SeriesStudy { series, content, lexicon_refs });
    }

    Ok(by_lemma.into_values().collect())
}

/// Step 8: Fetch historical context for a book/pericope.
pub async fn fetch_historical_context(
    conn: &RequestConnection,
    book: &str,
) -> Result<Option<(Vec<CommentaryNote>, Vec<EntityRef>)>, async_graphql::Error> {
    let commentary_sql = r#"
SELECT content, COALESCE(series, 'unknown') AS series
FROM commentary_historical_context
WHERE book = $1
ORDER BY passage_ref
"#;
    let c_rows = conn.execute(commentary_sql, &[PgValue::Text(book.to_owned())])
        .await.unwrap_or_default();

    let commentary_notes: Vec<CommentaryNote> = c_rows.iter().map(|row| CommentaryNote {
        content: text_col(row, "content"),
        series: text_col(row, "series"),
    }).collect();

    // Entity refs from ref_passage_entities (may not exist)
    let entity_sql = r#"
SELECT re.name, re.entity_type::text AS entity_type
FROM ref_passage_entities rpe
JOIN ref_entities re ON rpe.entity_id = re.id
WHERE rpe.passage_ref LIKE $1
ORDER BY re.name
LIMIT 20
"#;
    let e_rows = conn.execute(entity_sql, &[PgValue::Text(format!("{book}.%"))])
        .await.unwrap_or_default();

    let entities: Vec<EntityRef> = e_rows.iter().map(|row| EntityRef {
        name: text_col(row, "name"),
        entity_type: text_col(row, "entity_type"),
        role: String::new(),
        lat: None,
        lng: None,
    }).collect();

    if commentary_notes.is_empty() && entities.is_empty() {
        Ok(None)
    } else {
        Ok(Some((commentary_notes, entities)))
    }
}

/// Step 9: Fetch literary context (outline position, pericope summary, intertextual refs).
pub async fn fetch_literary_context(
    conn: &RequestConnection,
    book: &str,
) -> Result<Option<(Option<String>, Option<String>, Vec<String>)>, async_graphql::Error> {
    let main_idea_sql = r#"
SELECT main_idea FROM commentary_main_ideas WHERE book = $1 LIMIT 1
"#;
    let mi_rows = conn.execute(main_idea_sql, &[PgValue::Text(book.to_owned())])
        .await.unwrap_or_default();
    let pericope_summary = mi_rows.first().and_then(|r| opt_text_col(r, "main_idea"));

    let outline_sql = r#"
SELECT context_prose::text AS outline_position FROM commentary_literary_context WHERE book = $1 LIMIT 1
"#;
    let ol_rows = conn.execute(outline_sql, &[PgValue::Text(book.to_owned())])
        .await.unwrap_or_default();
    let outline_position = ol_rows.first().and_then(|r| opt_text_col(r, "outline_position"));

    let xref_sql = r#"
SELECT DISTINCT target_ref FROM commentary_cross_references
WHERE book = $1 ORDER BY target_ref LIMIT 20
"#;
    let xref_rows = conn.execute(xref_sql, &[PgValue::Text(book.to_owned())])
        .await.unwrap_or_default();
    let xrefs: Vec<String> = xref_rows.iter().map(|r| text_col(r, "target_ref")).collect();

    if pericope_summary.is_none() && outline_position.is_none() && xrefs.is_empty() {
        Ok(None)
    } else {
        Ok(Some((outline_position, pericope_summary, xrefs)))
    }
}

// ── Aggregated PericopeCard output (Steps 6-9) ───────────────────────────────

#[derive(Clone)]
pub struct PericopeCardPartial {
    pub r#ref: String,
    pub title: String,
    pub discourse_notes: Vec<DiscourseNote>,
    pub logical_flow: Vec<String>,
    pub word_studies: Vec<WordStudyEntry>,
    pub historical_notes: Vec<CommentaryNote>,
    pub entities: Vec<EntityRef>,
    pub outline_position: Option<String>,
    pub pericope_summary: Option<String>,
    pub intertextual_refs: Vec<String>,
}

pub async fn build_pericope_partial(
    conn: &RequestConnection,
    book: &str,
    pericope_ref: &str,
    title: &str,
) -> PericopeCardPartial {
    let (arg_res, word_studies_res, hist_res, lit_res) = tokio::join!(
        fetch_argument_tracing(conn, book, pericope_ref),
        fetch_word_studies(conn, book),
        fetch_historical_context(conn, book),
        fetch_literary_context(conn, book),
    );

    let (discourse_notes, logical_flow) = arg_res.unwrap_or_default();
    let word_studies = word_studies_res.unwrap_or_default();
    let (historical_notes, entities) = hist_res.unwrap_or(None).unwrap_or_default();
    let (outline_position, pericope_summary, intertextual_refs) =
        lit_res.unwrap_or(None).unwrap_or((None, None, vec![]));

    PericopeCardPartial {
        r#ref: pericope_ref.to_string(),
        title: title.to_string(),
        discourse_notes,
        logical_flow,
        word_studies,
        historical_notes,
        entities,
        outline_position,
        pericope_summary,
        intertextual_refs,
    }
}

// ── GraphQL Type Registration ────────────────────────────────────────────────

pub fn register_pericope_types(builder: SchemaBuilder) -> SchemaBuilder {
    let found_lexicon_ref = Object::new("TocmaLexiconRef")
        .field(Field::new("raw",          TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<FoundLexiconRef>()?;
                Ok(Some(FieldValue::value(v.raw.clone())))
            })
        }))
        .field(Field::new("abbreviation", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<FoundLexiconRef>()?;
                Ok(Some(FieldValue::value(v.abbreviation.clone())))
            })
        }))
        .field(Field::new("fullTitle",    TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<FoundLexiconRef>()?;
                Ok(Some(FieldValue::value(v.full_title.clone())))
            })
        }))
        .field(Field::new("description",  TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<FoundLexiconRef>()?;
                Ok(Some(FieldValue::value(v.description.clone())))
            })
        }))
        .field(Field::new("pageOrSection", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<FoundLexiconRef>()?;
                Ok(v.page_or_section.clone().map(FieldValue::value))
            })
        }));

    let series_study = Object::new("TocmaSeriesStudy")
        .field(Field::new("series",      TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<SeriesStudy>()?;
                Ok(Some(FieldValue::value(v.series.clone())))
            })
        }))
        .field(Field::new("content",     TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<SeriesStudy>()?;
                Ok(Some(FieldValue::value(v.content.clone())))
            })
        }))
        .field(Field::new("lexiconRefs", TypeRef::named_nn_list_nn("TocmaLexiconRef"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<SeriesStudy>()?;
                let values: Vec<FieldValue> = v.lexicon_refs.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }));

    let word_study = Object::new("TocmaWordStudyEntry")
        .field(Field::new("lemma",            TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<WordStudyEntry>()?;
                Ok(Some(FieldValue::value(v.lemma.clone())))
            })
        }))
        .field(Field::new("transliteration",  TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<WordStudyEntry>()?;
                Ok(Some(FieldValue::value(v.transliteration.clone())))
            })
        }))
        .field(Field::new("strong",           TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<WordStudyEntry>()?;
                Ok(Some(FieldValue::value(v.strong.clone())))
            })
        }))
        .field(Field::new("gloss",            TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<WordStudyEntry>()?;
                Ok(Some(FieldValue::value(v.gloss.clone())))
            })
        }))
        .field(Field::new("significance",     TypeRef::named_nn(TypeRef::FLOAT), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<WordStudyEntry>()?;
                Ok(Some(FieldValue::value(v.significance as f64)))
            })
        }))
        .field(Field::new("louwNidaDomain",   TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<WordStudyEntry>()?;
                Ok(v.louw_nida_domain.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("louwNidaDomainName", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<WordStudyEntry>()?;
                Ok(v.louw_nida_domain_name.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("studies",          TypeRef::named_nn_list_nn("TocmaSeriesStudy"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<WordStudyEntry>()?;
                let values: Vec<FieldValue> = v.studies.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }));

    let discourse_note = Object::new("TocmaDiscourseNote")
        .field(Field::new("note",   TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DiscourseNote>()?;
                Ok(Some(FieldValue::value(v.note.clone())))
            })
        }))
        .field(Field::new("series", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DiscourseNote>()?;
                Ok(Some(FieldValue::value(v.series.clone())))
            })
        }));

    let commentary_note = Object::new("TocmaCommentaryNote")
        .field(Field::new("content", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<CommentaryNote>()?;
                Ok(Some(FieldValue::value(v.content.clone())))
            })
        }))
        .field(Field::new("series",  TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<CommentaryNote>()?;
                Ok(Some(FieldValue::value(v.series.clone())))
            })
        }));

    let entity_ref = Object::new("TocmaEntityRef")
        .field(Field::new("name",       TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<EntityRef>()?;
                Ok(Some(FieldValue::value(v.name.clone())))
            })
        }))
        .field(Field::new("entityType", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<EntityRef>()?;
                Ok(Some(FieldValue::value(v.entity_type.clone())))
            })
        }))
        .field(Field::new("role",       TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<EntityRef>()?;
                Ok(Some(FieldValue::value(v.role.clone())))
            })
        }));

    let pericope_card = Object::new("TocmaPericopeCard")
        .field(Field::new("ref",              TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardPartial>()?;
                Ok(Some(FieldValue::value(v.r#ref.clone())))
            })
        }))
        .field(Field::new("title",            TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardPartial>()?;
                Ok(Some(FieldValue::value(v.title.clone())))
            })
        }))
        .field(Field::new("discourseNotes",   TypeRef::named_nn_list_nn("TocmaDiscourseNote"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardPartial>()?;
                let values: Vec<FieldValue> = v.discourse_notes.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("logicalFlow",      TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardPartial>()?;
                let values: Vec<FieldValue> = v.logical_flow.iter().cloned().map(FieldValue::value).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("wordStudies",      TypeRef::named_nn_list_nn("TocmaWordStudyEntry"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardPartial>()?;
                let values: Vec<FieldValue> = v.word_studies.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("historicalNotes",  TypeRef::named_nn_list_nn("TocmaCommentaryNote"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardPartial>()?;
                let values: Vec<FieldValue> = v.historical_notes.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("entities",         TypeRef::named_nn_list_nn("TocmaEntityRef"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardPartial>()?;
                let values: Vec<FieldValue> = v.entities.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("outlinePosition",  TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardPartial>()?;
                Ok(v.outline_position.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("pericopeSummary",  TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardPartial>()?;
                Ok(v.pericope_summary.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("intertextualRefs", TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeCardPartial>()?;
                let values: Vec<FieldValue> = v.intertextual_refs.iter().cloned().map(FieldValue::value).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }));

    builder
        .register(found_lexicon_ref)
        .register(series_study)
        .register(word_study)
        .register(discourse_note)
        .register(commentary_note)
        .register(entity_ref)
        .register(pericope_card)
}

// ── Root query field ─────────────────────────────────────────────────────────

/// `tocmaPericope(book: String!, pericopeRef: String!, title: String!): TocmaPericopeCard`
/// Returns Steps 6-9 (mechanical pericope data). Steps 10-12 are added by theology.rs.
pub fn tocma_pericope_field() -> Field {
    Field::new(
        "tocmaPericope",
        TypeRef::named("TocmaPericopeCard"),
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
                let title = ctx.args.try_get("title").ok()
                    .and_then(|v| v.string().ok().map(|s| s.to_owned()))
                    .unwrap_or_default();

                let partial = build_pericope_partial(conn, &book, &pericope_ref, &title).await;
                Ok(Some(FieldValue::owned_any(partial)))
            })
        },
    )
    .argument(InputValue::new("book",        TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("pericopeRef", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("title",       TypeRef::named(TypeRef::STRING)))
}
