//! Store-backed data access for reader resolvers.
//!
//! When a `StoreCache` is in the GraphQL context, resolvers use these functions
//! to read from local JSON files instead of querying the database. This enables
//! local development without Docker or Supabase.
//!
//! Each function mirrors the SQL fetch function it replaces, returning the same
//! output structs — the resolver layer above doesn't change.

use std::sync::Arc;

use fw_store::client::StoreCache;
use serde_json::Value;

use super::reader::{
    ConceptAlignment, ConnectedInsight, DepthInsight, DiscoveryHeat, GenreSection,
    LiteraryContext, LiteraryStructure, MainIdea, PericopeDrilldown, PericopeUnit,
    VersePericope,
};
use super::reader_blocks::PhrasedBlock;

// ── JSON value accessors ────────────────────────────────────────────────────

fn jstr(v: &Value, key: &str) -> String {
    match &v[key] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn jstr_opt(v: &Value, key: &str) -> Option<String> {
    match &v[key] {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        _ => None,
    }
}

fn jint(v: &Value, key: &str) -> i64 {
    v[key].as_i64().unwrap_or(0)
}

fn jfloat(v: &Value, key: &str) -> f64 {
    v[key].as_f64().unwrap_or(0.0)
}

fn jstr_array(v: &Value, key: &str) -> Vec<String> {
    match &v[key] {
        Value::Array(arr) => arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
        _ => Vec::new(),
    }
}

/// Check if a passage_ref matches book.chapter.* pattern.
fn matches_chapter(passage_ref: &str, book: &str, chapter: i64) -> bool {
    let prefix = format!("{}.{}.", book, chapter);
    passage_ref.starts_with(&prefix)
}

// ── Phrased blocks ──────────────────────────────────────────────────────────

/// Global store key — reference data is in _global/, not per-book.
const GLOBAL: &str = "_global";

pub fn fetch_phrased_blocks_from_store(
    cache: &StoreCache,
    book: &str,
    chapter: i64,
) -> Result<Vec<PhrasedBlock>, async_graphql::Error> {
    let rows = cache
        .table(book, "phrasing", "phrased_blocks")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;

    let mut blocks: Vec<PhrasedBlock> = rows
        .iter()
        .filter(|row| {
            jstr(row, "passage_ref")
                .starts_with(&format!("{}.{}.", book, chapter))
        })
        .map(|row| PhrasedBlock {
            passage_ref: jstr(row, "passage_ref"),
            block_order: jint(row, "block_order"),
            lines: match &row["lines"] {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            },
        })
        .collect();

    blocks.sort_by_key(|b| b.block_order);
    Ok(blocks)
}

// ── Depth insights ──────────────────────────────────────────────────────────

pub fn fetch_depth_insights_from_store(
    cache: &StoreCache,
    book: &str,
    chapter: i64,
) -> Result<Vec<DepthInsight>, async_graphql::Error> {
    let insights_rows = cache
        .table(GLOBAL, "insights", "depth_insights")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;
    let concepts_rows = cache
        .table(GLOBAL, "insights", "depth_insight_concepts")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;
    let links_rows = cache
        .table(GLOBAL, "insights", "depth_insight_links")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;

    // Filter insights for this chapter (by book+chapter columns or passage_ref pattern)
    let chapter_insights: Vec<&Value> = insights_rows
        .iter()
        .filter(|row| {
            // Direct column match
            (jstr(row, "book") == book && jint(row, "chapter") == chapter)
            // Or passage_ref pattern
            || matches_chapter(&jstr(row, "passage_ref"), book, chapter)
        })
        .collect();

    // Also include insights linked TO this chapter via depth_insight_links
    let chapter_prefix = format!("{}.{}.", book, chapter);
    let linked_insight_ids: Vec<String> = links_rows
        .iter()
        .filter(|row| jstr(row, "linked_passage_ref").starts_with(&chapter_prefix))
        .map(|row| jstr(row, "insight_id"))
        .collect();

    let mut result: Vec<DepthInsight> = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    let all_matching: Vec<&Value> = chapter_insights
        .into_iter()
        .chain(
            insights_rows.iter().filter(|row| {
                let id = jstr(row, "id");
                linked_insight_ids.contains(&id)
            }),
        )
        .collect();

    for row in all_matching {
        let id = jstr(row, "id");
        if !seen_ids.insert(id.clone()) {
            continue;
        }

        let related_concept_ids: Vec<String> = concepts_rows
            .iter()
            .filter(|c| jstr(c, "insight_id") == id)
            .map(|c| jstr(c, "concept_id"))
            .collect();

        let related_passage_refs: Vec<String> = links_rows
            .iter()
            .filter(|l| jstr(l, "insight_id") == id)
            .map(|l| jstr(l, "linked_passage_ref"))
            .collect();

        result.push(DepthInsight {
            id,
            passage_ref: jstr(row, "passage_ref"),
            insight_type: jstr(row, "insight_type"),
            title: jstr(row, "title"),
            body: jstr(row, "body"),
            related_concept_ids,
            related_passage_refs,
            confidence: jfloat(row, "confidence"),
        });
    }

    result.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.passage_ref.cmp(&b.passage_ref))
    });

    Ok(result)
}

// ── Concept alignments ──────────────────────────────────────────────────────

pub fn fetch_concept_alignments_from_store(
    cache: &StoreCache,
    book: &str,
    chapter: i64,
) -> Result<Vec<ConceptAlignment>, async_graphql::Error> {
    let alignment_rows = cache
        .table(GLOBAL, "alignments", "concept_alignments")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;

    let concept_rows: Arc<Vec<Value>> = cache
        .table(GLOBAL, "concepts", "concepts")
        .unwrap_or_else(|_| Arc::new(Vec::new()));

    // Build concept lookup
    let concept_map: std::collections::HashMap<String, &Value> = concept_rows
        .iter()
        .map(|c| (jstr(c, "id"), c))
        .collect();

    let mut alignments: Vec<ConceptAlignment> = alignment_rows
        .iter()
        .filter(|row| jstr(row, "book") == book && jint(row, "chapter") == chapter)
        .map(|row| {
            let concept_id = jstr(row, "concept_id");
            let concept = concept_map.get(&concept_id);

            ConceptAlignment {
                id: jstr(row, "id"),
                passage_ref: jstr(row, "passage_ref"),
                concept_id: concept_id.clone(),
                english_span: jstr(row, "english_span"),
                verse: jint(row, "verse_start"),
                role: jstr_opt(row, "role"),
                alignment_note: jstr_opt(row, "alignment_note"),
                confidence: jfloat(row, "confidence"),
                token_surface_forms: jstr_array(row, "token_surface_forms"),
                // Joined concept fields
                lemma: concept.map(|c| jstr(c, "lemma")).unwrap_or_default(),
                language: concept.map(|c| jstr(c, "language")).unwrap_or_default(),
                transliteration: concept.map(|c| jstr(c, "transliteration")).unwrap_or_default(),
                strongs_display: concept.map(|c| jstr(c, "strongs_display")).unwrap_or_default(),
                semantic_range: concept.map(|c| jstr_array(c, "semantic_range")).unwrap_or_default(),
                theological_note: concept.and_then(|c| jstr_opt(c, "theological_note")),
                occurrence_count: concept.map(|c| jint(c, "occurrence_count")).unwrap_or(0),
            }
        })
        .collect();

    alignments.sort_by_key(|a| (a.verse, a.english_span.clone()));
    Ok(alignments)
}

// ── Pericope context ────────────────────────────────────────────────────────

pub fn fetch_pericope_context_from_store(
    cache: &StoreCache,
    book: &str,
    chapter: i64,
) -> Result<Vec<PericopeUnit>, async_graphql::Error> {
    let rows = cache
        .table(GLOBAL, "structure", "pericope_units")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;

    let mut units: Vec<PericopeUnit> = rows
        .iter()
        .filter(|row| jstr(row, "book") == book && jint(row, "chapter_start") == chapter)
        .map(|row| {
            let chapter_end = jint(row, "chapter_end");
            let verse_start = jint(row, "verse_start");
            let verse_end = row["verse_end"].as_i64().unwrap_or(verse_start);
            PericopeUnit {
                id: jstr(row, "id"),
                title: jstr(row, "pericope_title"),
                start_ref: format!("{}.{}.{}", book, chapter, verse_start),
                end_ref: format!("{}.{}.{}", book, chapter_end, verse_end),
                genre: jstr_opt(row, "genre"),
                structure_note: None,
                anchor_concept_id: None,
            }
        })
        .collect();

    units.sort_by_key(|u| (jint(&Value::Null, ""), u.start_ref.clone()));
    Ok(units)
}

// ── Discovery heat ──────────────────────────────────────────────────────────
// discovery_heat is a materialized view — not archived in stores.
// Returns empty when using file-backed mode (heat is user-specific runtime data).

pub fn fetch_discovery_heat_from_store(
    _cache: &StoreCache,
    _book: &str,
    _chapter: i64,
) -> Result<Vec<DiscoveryHeat>, async_graphql::Error> {
    // Discovery heat is user-specific runtime data from a materialized view.
    // Not available from static stores — return empty.
    Ok(Vec::new())
}

// ── Genre sections ──────────────────────────────────────────────────────────

pub fn fetch_genre_sections_from_store(
    cache: &StoreCache,
    book: &str,
    chapter: i64,
) -> Result<Vec<GenreSection>, async_graphql::Error> {
    let rows = cache
        .table(GLOBAL, "structure", "genre_sections")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;

    let mut sections: Vec<GenreSection> = rows
        .iter()
        .filter(|row| jstr(row, "book") == book && jint(row, "chapter") == chapter)
        .map(|row| GenreSection {
            verse_start: jint(row, "verse_start"),
            verse_end: jint(row, "verse_end"),
            genre: jstr(row, "genre"),
            sub_type: jstr_opt(row, "sub_type"),
        })
        .collect();

    sections.sort_by_key(|s| s.verse_start);
    Ok(sections)
}

// ── Literary structures ─────────────────────────────────────────────────────

pub fn fetch_literary_structures_from_store(
    cache: &StoreCache,
    book: &str,
    chapter: i64,
) -> Result<Vec<LiteraryStructure>, async_graphql::Error> {
    let rows = cache
        .table(GLOBAL, "structure", "literary_structures")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;

    let mut structures: Vec<LiteraryStructure> = rows
        .iter()
        .filter(|row| {
            jstr(row, "book") == book
                && jint(row, "chapter_start") <= chapter
                && jint(row, "chapter_end") >= chapter
        })
        .map(|row| LiteraryStructure {
            structure_type: jstr(row, "structure_type"),
            title: jstr_opt(row, "title"),
            verse_start: jint(row, "verse_start"),
            verse_end: jint(row, "verse_end"),
            pairs: match &row["pairs"] {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            },
            center_ref: jstr_opt(row, "center_ref"),
            source: jstr(row, "source"),
        })
        .collect();

    structures.sort_by_key(|s| s.verse_start);
    Ok(structures)
}

// ── Connected insights ──────────────────────────────────────────────────────

pub fn fetch_connected_insights_from_store(
    cache: &StoreCache,
    book: &str,
    chapter: i64,
) -> Result<Vec<ConnectedInsight>, async_graphql::Error> {
    let insights_rows = cache
        .table(GLOBAL, "insights", "depth_insights")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;
    let links_rows = cache
        .table(GLOBAL, "insights", "depth_insight_links")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;

    let chapter_prefix = format!("{}.{}.", book, chapter);

    // Build insight lookup for titles
    let insight_map: std::collections::HashMap<String, &Value> = insights_rows
        .iter()
        .map(|i| (jstr(i, "id"), i))
        .collect();

    let connected: Vec<ConnectedInsight> = links_rows
        .iter()
        .filter(|row| {
            let linked = jstr(row, "linked_passage_ref");
            linked.starts_with(&chapter_prefix)
        })
        .filter_map(|row| {
            let insight_id = jstr(row, "insight_id");
            let insight = insight_map.get(&insight_id)?;
            let source_ref = jstr(insight, "passage_ref");
            // Exclude self-references (insights from this chapter)
            if source_ref.starts_with(&chapter_prefix) {
                return None;
            }
            Some(ConnectedInsight {
                insight_id,
                source_passage_ref: source_ref,
                source_title: jstr(insight, "title"),
                link_direction: jstr(row, "link_direction"),
            })
        })
        .take(50)
        .collect();

    Ok(connected)
}

// ── Main ideas ──────────────────────────────────────────────────────────────

pub fn fetch_main_ideas_from_store(
    cache: &StoreCache,
    book: &str,
    chapter: i64,
) -> Result<Vec<MainIdea>, async_graphql::Error> {
    let rows = cache
        .table(GLOBAL, "commentary", "commentary_main_ideas")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;

    let ch_str = chapter.to_string();

    let mut ideas: Vec<MainIdea> = rows
        .iter()
        .filter(|row| {
            jstr(row, "book") == book && {
                let vs = jstr(row, "verse_start");
                vs.starts_with(&format!("{}.", ch_str)) || vs.starts_with(&format!("{}.%", ch_str))
            }
        })
        .map(|row| MainIdea {
            id: jstr(row, "id"),
            book: jstr(row, "book"),
            verse_start: jstr(row, "verse_start"),
            verse_end: jstr(row, "verse_end"),
            main_idea: jstr(row, "main_idea"),
            series: jstr(row, "series"),
        })
        .collect();

    ideas.sort_by(|a, b| a.verse_start.cmp(&b.verse_start));
    Ok(ideas)
}

// ── Literary context ────────────────────────────────────────────────────────

pub fn fetch_literary_context_from_store(
    cache: &StoreCache,
    book: &str,
    chapter: i64,
) -> Result<Vec<LiteraryContext>, async_graphql::Error> {
    let rows = cache
        .table(GLOBAL, "commentary", "commentary_literary_context")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;

    let ch_str = chapter.to_string();

    let mut contexts: Vec<LiteraryContext> = rows
        .iter()
        .filter(|row| {
            jstr(row, "book") == book && {
                let vs = jstr(row, "verse_start");
                vs.starts_with(&format!("{}.", ch_str))
            }
        })
        .map(|row| LiteraryContext {
            id: jstr(row, "id"),
            book: jstr(row, "book"),
            verse_start: jstr(row, "verse_start"),
            verse_end: jstr(row, "verse_end"),
            context_prose: match &row["context_prose"] {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            },
            scripture_refs: match &row["scripture_refs"] {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            },
            series: jstr(row, "series"),
        })
        .collect();

    contexts.sort_by(|a, b| a.verse_start.cmp(&b.verse_start));
    Ok(contexts)
}

// ── Pericope index ──────────────────────────────────────────────────────────

/// Look up which pericope a specific verse belongs to.
/// Returns None if the verse isn't covered by any pericope.
pub fn fetch_verse_pericope_from_store(
    cache: &StoreCache,
    book: &str,
    chapter: i64,
    verse: i64,
) -> Result<Option<VersePericope>, async_graphql::Error> {
    let rows = cache
        .table(GLOBAL, "pericope_index", "verse_to_pericope")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;

    let needle = format!("{}.{}.{}", book, chapter, verse);
    Ok(rows
        .iter()
        .find(|row| jstr(row, "verse_ref") == needle)
        .map(|row| VersePericope {
            pericope_id: jstr(row, "pericope_id"),
            pericope_title: jstr(row, "pericope_title"),
        }))
}

/// Full pericope drill-down: metadata + all related entity ID lists.
/// Returns None if the pericope_id isn't in the index.
pub fn fetch_pericope_drilldown_from_store(
    cache: &StoreCache,
    pericope_id: &str,
) -> Result<Option<PericopeDrilldown>, async_graphql::Error> {
    let rows = cache
        .table(GLOBAL, "pericope_index", "pericope_index")
        .map_err(|e| async_graphql::Error::new(format!("store read failed: {e}")))?;

    Ok(rows
        .iter()
        .find(|row| jstr(row, "pericope_id") == pericope_id)
        .map(|row| PericopeDrilldown {
            pericope_id: jstr(row, "pericope_id"),
            book: jstr(row, "book"),
            passage_ref: jstr(row, "passage_ref"),
            pericope_title: jstr(row, "pericope_title"),
            chapter_start: jint(row, "chapter_start"),
            verse_start: jint(row, "verse_start"),
            chapter_end: jint(row, "chapter_end"),
            verse_end: row["verse_end"].as_i64(),
            pm_clause_ids: jstr_array(row, "pm_clause_ids"),
            pm_discourse_unit_ids: jstr_array(row, "pm_discourse_unit_ids"),
            phrase_structure_node_ids: jstr_array(row, "phrase_structure_node_ids"),
            commentary_main_idea_ids: jstr_array(row, "commentary_main_idea_ids"),
            commentary_literary_context_ids: jstr_array(row, "commentary_literary_context_ids"),
            concept_alignment_ids: jstr_array(row, "concept_alignment_ids"),
            depth_insight_ids: jstr_array(row, "depth_insight_ids"),
            genre_section_ids: jstr_array(row, "genre_section_ids"),
        }))
}
