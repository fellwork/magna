//! Custom resolver factories for concept-graph traversal queries.
//!
//! Registers three top-level Query fields:
//!
//! * `conceptThread(conceptId: ID!, maxDepth: Int): [ConceptEdge!]!`
//!   — Recursive BFS over `concept_relationships` from a given concept UUID.
//!
//! * `relatedVerses(book: String!, chapter: Int!, verse: Int!, minVotes: Int): [VerseXref!]!`
//!   — Verse-level cross-references from `verse_cross_references`.
//!
//! * `verseContext(book: String!, chapter: Int!, verse: Int!): VerseContext`
//!   — Layer 3 side-panel query: related verses + per-concept threads for
//!     concepts that appear in the requested passage.
//!
//! Output types are registered by `register_graph_types()` which must be called
//! before `build_schema()` finishes.

use std::sync::Arc;

use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef,
};
use fw_graph_types::PgValue;

use crate::executor::{QueryExecutor, RequestConnection};

// ── Output structs ────────────────────────────────────────────────────────────

/// One hop in a concept thread: the neighbouring concept + relationship metadata.
#[derive(Clone)]
pub struct ConceptEdge {
    pub concept_id:        String,
    pub lemma:             String,
    pub language:          String,
    pub relationship_type: String,
    pub strength:          f64,
    pub note:              Option<String>,
    pub depth:             i32,
}

/// One verse cross-reference row.
#[derive(Clone)]
pub struct VerseXref {
    pub to_book:  String,
    pub to_ch:    i32,
    pub to_vs:    i32,
    pub votes:    i32,
    pub source:   String,
}

/// The combined Layer 3 context for a single verse.
#[derive(Clone)]
pub struct VerseContext {
    pub book:          String,
    pub chapter:       i32,
    pub verse:         i32,
    pub related:       Vec<VerseXref>,
    pub concept_edges: Vec<ConceptEdge>,
}

// ── Type registration ─────────────────────────────────────────────────────────

/// Register `ConceptEdge`, `VerseXref`, and `VerseContext` object types.
/// Call this before `builder.finish()`.
pub fn register_graph_types(
    builder: async_graphql::dynamic::SchemaBuilder,
) -> async_graphql::dynamic::SchemaBuilder {
    let concept_edge = Object::new("ConceptEdge")
        .field(Field::new("conceptId", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<ConceptEdge>()?;
                Ok(Some(FieldValue::value(e.concept_id.clone())))
            })
        }))
        .field(Field::new("lemma", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<ConceptEdge>()?;
                Ok(Some(FieldValue::value(e.lemma.clone())))
            })
        }))
        .field(Field::new("language", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<ConceptEdge>()?;
                Ok(Some(FieldValue::value(e.language.clone())))
            })
        }))
        .field(Field::new("relationshipType", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<ConceptEdge>()?;
                Ok(Some(FieldValue::value(e.relationship_type.clone())))
            })
        }))
        .field(Field::new("strength", TypeRef::named_nn(TypeRef::FLOAT), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<ConceptEdge>()?;
                Ok(Some(FieldValue::value(e.strength)))
            })
        }))
        .field(Field::new("note", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<ConceptEdge>()?;
                Ok(e.note.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("depth", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<ConceptEdge>()?;
                Ok(Some(FieldValue::value(e.depth as i64)))
            })
        }));

    let verse_xref = Object::new("VerseXref")
        .field(Field::new("toBook", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseXref>()?;
                Ok(Some(FieldValue::value(v.to_book.clone())))
            })
        }))
        .field(Field::new("toChapter", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseXref>()?;
                Ok(Some(FieldValue::value(v.to_ch as i64)))
            })
        }))
        .field(Field::new("toVerse", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseXref>()?;
                Ok(Some(FieldValue::value(v.to_vs as i64)))
            })
        }))
        .field(Field::new("votes", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseXref>()?;
                Ok(Some(FieldValue::value(v.votes as i64)))
            })
        }))
        .field(Field::new("source", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseXref>()?;
                Ok(Some(FieldValue::value(v.source.clone())))
            })
        }));

    let verse_context = Object::new("VerseContext")
        .field(Field::new("book", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let vc = ctx.parent_value.try_downcast_ref::<VerseContext>()?;
                Ok(Some(FieldValue::value(vc.book.clone())))
            })
        }))
        .field(Field::new("chapter", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let vc = ctx.parent_value.try_downcast_ref::<VerseContext>()?;
                Ok(Some(FieldValue::value(vc.chapter as i64)))
            })
        }))
        .field(Field::new("verse", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let vc = ctx.parent_value.try_downcast_ref::<VerseContext>()?;
                Ok(Some(FieldValue::value(vc.verse as i64)))
            })
        }))
        .field(Field::new("related", TypeRef::named_nn_list_nn("VerseXref"), |ctx| {
            FieldFuture::new(async move {
                let vc = ctx.parent_value.try_downcast_ref::<VerseContext>()?;
                let values: Vec<FieldValue> = vc.related
                    .iter()
                    .cloned()
                    .map(|r| FieldValue::owned_any(r))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("conceptEdges", TypeRef::named_nn_list_nn("ConceptEdge"), |ctx| {
            FieldFuture::new(async move {
                let vc = ctx.parent_value.try_downcast_ref::<VerseContext>()?;
                let values: Vec<FieldValue> = vc.concept_edges
                    .iter()
                    .cloned()
                    .map(|e| FieldValue::owned_any(e))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        }));

    builder
        .register(concept_edge)
        .register(verse_xref)
        .register(verse_context)
}

// ── conceptThread resolver ────────────────────────────────────────────────────

/// Build `conceptThread(conceptId: ID!, maxDepth: Int): [ConceptEdge!]!`
///
/// Uses a recursive CTE to walk `concept_relationships` BFS-style up to
/// `maxDepth` hops (default 2, max 4). Returns each reachable concept once
/// (shortest-path depth wins).
pub fn build_concept_thread_resolver(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "conceptThread",
        TypeRef::named_nn_list_nn("ConceptEdge"),
        |ctx| {
            FieldFuture::new(async move {
                let conn = ctx
                    .data_opt::<RequestConnection>()
                    .ok_or_else(|| async_graphql::Error::new("No database connection"))?;

                let concept_id = ctx
                    .args
                    .try_get("conceptId")?
                    .string()
                    .map_err(|_| async_graphql::Error::new("conceptId must be a string"))?
                    .to_owned();

                let max_depth = ctx
                    .args
                    .get("maxDepth")
                    .and_then(|v| v.i64().ok())
                    .unwrap_or(2)
                    .clamp(1, 4) as i32;

                let edges = fetch_concept_thread(conn, &concept_id, max_depth).await?;

                let values: Vec<FieldValue> = edges
                    .into_iter()
                    .map(|e| FieldValue::owned_any(e))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("conceptId", TypeRef::named_nn(TypeRef::ID)))
    .argument(InputValue::new("maxDepth", TypeRef::named(TypeRef::INT)))
}

// ── relatedVerses resolver ────────────────────────────────────────────────────

/// Build `relatedVerses(book: String!, chapter: Int!, verse: Int!, minVotes: Int): [VerseXref!]!`
pub fn build_related_verses_resolver(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "relatedVerses",
        TypeRef::named_nn_list_nn("VerseXref"),
        |ctx| {
            FieldFuture::new(async move {
                let conn = ctx
                    .data_opt::<RequestConnection>()
                    .ok_or_else(|| async_graphql::Error::new("No database connection"))?;

                let book = ctx
                    .args
                    .try_get("book")?
                    .string()
                    .map_err(|_| async_graphql::Error::new("book must be a string"))?
                    .to_owned();

                let chapter = ctx
                    .args
                    .try_get("chapter")?
                    .i64()
                    .map_err(|_| async_graphql::Error::new("chapter must be an int"))?
                    as i32;

                let verse = ctx
                    .args
                    .try_get("verse")?
                    .i64()
                    .map_err(|_| async_graphql::Error::new("verse must be an int"))?
                    as i32;

                let min_votes = ctx
                    .args
                    .get("minVotes")
                    .and_then(|v| v.i64().ok())
                    .unwrap_or(1) as i32;

                let xrefs =
                    fetch_related_verses(conn, &book, chapter, verse, min_votes).await?;

                let values: Vec<FieldValue> = xrefs
                    .into_iter()
                    .map(|v| FieldValue::owned_any(v))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("verse",   TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("minVotes", TypeRef::named(TypeRef::INT)))
}

// ── verseContext resolver ─────────────────────────────────────────────────────

/// Build `verseContext(book: String!, chapter: Int!, verse: Int!): VerseContext`
///
/// Returns related verses + concept threads for all concepts in the passage,
/// de-duplicated and limited to depth 1 for the concept layer.
pub fn build_verse_context_resolver(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "verseContext",
        TypeRef::named("VerseContext"),
        |ctx| {
            FieldFuture::new(async move {
                let conn = ctx
                    .data_opt::<RequestConnection>()
                    .ok_or_else(|| async_graphql::Error::new("No database connection"))?;

                let book = ctx
                    .args
                    .try_get("book")?
                    .string()
                    .map_err(|_| async_graphql::Error::new("book must be a string"))?
                    .to_owned();

                let chapter = ctx
                    .args
                    .try_get("chapter")?
                    .i64()
                    .map_err(|_| async_graphql::Error::new("chapter must be an int"))?
                    as i32;

                let verse = ctx
                    .args
                    .try_get("verse")?
                    .i64()
                    .map_err(|_| async_graphql::Error::new("verse must be an int"))?
                    as i32;

                // 1. Verse-level cross-references (votes > 0)
                let related = fetch_related_verses(conn, &book, chapter, verse, 1).await?;

                // 2. All concepts that appear in this passage (via passage_tokens)
                let concept_edges =
                    fetch_passage_concept_edges(conn, &book, chapter, verse).await?;

                let vc = VerseContext { book, chapter, verse, related, concept_edges };
                Ok(Some(FieldValue::owned_any(vc)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("verse",   TypeRef::named_nn(TypeRef::INT)))
}

// ── SQL helpers ───────────────────────────────────────────────────────────────

/// Recursive BFS over concept_relationships using a CTE.
/// Returns edges reachable within `max_depth` hops, shortest depth first.
async fn fetch_concept_thread(
    conn: &RequestConnection,
    concept_id: &str,
    max_depth: i32,
) -> Result<Vec<ConceptEdge>, async_graphql::Error> {
    // Validate UUID format before injecting into SQL (prevents injection).
    let root_id = uuid::Uuid::parse_str(concept_id)
        .map_err(|_| async_graphql::Error::new("conceptId must be a valid UUID"))?
        .to_string();

    let sql = r#"
WITH RECURSIVE thread AS (
  -- Seed: direct neighbours of the root concept
  SELECT
    cr.source_concept_id,
    cr.target_concept_id,
    cr.relationship_type,
    cr.strength,
    cr.note,
    1 AS depth
  FROM concept_relationships cr
  WHERE cr.source_concept_id = $1::uuid
     OR cr.target_concept_id = $1::uuid

  UNION

  -- Recursive: neighbours of neighbours, up to max_depth
  SELECT
    cr.source_concept_id,
    cr.target_concept_id,
    cr.relationship_type,
    cr.strength,
    cr.note,
    t.depth + 1
  FROM concept_relationships cr
  JOIN thread t
    ON cr.source_concept_id = t.target_concept_id
    OR cr.target_concept_id = t.source_concept_id
  WHERE t.depth < $2
),
-- Resolve the "other" side of each edge relative to the root
neighbours AS (
  SELECT DISTINCT ON (neighbour_id)
    CASE
      WHEN source_concept_id = $1::uuid THEN target_concept_id
      ELSE source_concept_id
    END AS neighbour_id,
    relationship_type,
    strength,
    note,
    depth
  FROM thread
  WHERE source_concept_id <> target_concept_id
  ORDER BY neighbour_id, depth ASC
)
SELECT
  n.neighbour_id::text AS concept_id,
  c.lemma,
  c.language::text     AS language,
  n.relationship_type,
  n.strength,
  n.note,
  n.depth
FROM neighbours n
JOIN concepts c ON c.id = n.neighbour_id
ORDER BY n.depth ASC, n.strength DESC
LIMIT 100
"#;

    let rows = conn
        .execute(sql, &[PgValue::Text(root_id), PgValue::Int(max_depth as i64)])
        .await
        .map_err(|e| async_graphql::Error::new(format!("concept_thread query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| ConceptEdge {
            concept_id:        text_col(&row, "concept_id"),
            lemma:             text_col(&row, "lemma"),
            language:          text_col(&row, "language"),
            relationship_type: text_col(&row, "relationship_type"),
            strength:          float_col(&row, "strength"),
            note:              opt_text_col(&row, "note"),
            depth:             int_col(&row, "depth") as i32,
        })
        .collect())
}

/// Fetch verse cross-references for one verse from `verse_cross_references`.
async fn fetch_related_verses(
    conn: &RequestConnection,
    book: &str,
    chapter: i32,
    verse: i32,
    min_votes: i32,
) -> Result<Vec<VerseXref>, async_graphql::Error> {
    let sql = r#"
SELECT to_book, to_ch::int, to_vs::int, votes, source
FROM verse_cross_references
WHERE from_book = $1
  AND from_ch   = $2
  AND from_vs   = $3
  AND votes     >= $4
ORDER BY votes DESC
LIMIT 50
"#;

    let rows = conn
        .execute(
            sql,
            &[
                PgValue::Text(book.to_owned()),
                PgValue::Int(chapter as i64),
                PgValue::Int(verse as i64),
                PgValue::Int(min_votes as i64),
            ],
        )
        .await
        .map_err(|e| async_graphql::Error::new(format!("related_verses query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| VerseXref {
            to_book: text_col(&row, "to_book"),
            to_ch:   int_col(&row, "to_ch") as i32,
            to_vs:   int_col(&row, "to_vs") as i32,
            votes:   int_col(&row, "votes") as i32,
            source:  text_col(&row, "source"),
        })
        .collect())
}

/// For a given verse, find all concepts whose tokens appear in that passage,
/// then return their immediate concept_relationships (depth 1).
async fn fetch_passage_concept_edges(
    conn: &RequestConnection,
    book: &str,
    chapter: i32,
    verse: i32,
) -> Result<Vec<ConceptEdge>, async_graphql::Error> {
    // Step 1: collect concept IDs for all tokens in this verse.
    let passage_sql = r#"
SELECT DISTINCT c.id::text AS concept_id
FROM passages p
JOIN passage_tokens pt ON pt.passage_id = p.id
JOIN concepts c ON c.vocabulary_item_id = pt.vocabulary_item_id
WHERE p.book    = $1
  AND p.chapter = $2
  AND p.verse   = $3
LIMIT 50
"#;

    let passage_rows = conn
        .execute(
            passage_sql,
            &[
                PgValue::Text(book.to_owned()),
                PgValue::Int(chapter as i64),
                PgValue::Int(verse as i64),
            ],
        )
        .await
        .map_err(|e| async_graphql::Error::new(format!("passage concept lookup failed: {e}")))?;

    if passage_rows.is_empty() {
        return Ok(vec![]);
    }

    // Step 2: for each concept, get immediate neighbours (depth=1).
    // Run one query with ANY() to avoid N+1.
    let concept_ids: Vec<String> = passage_rows
        .iter()
        .filter_map(|r| r.get("concept_id").and_then(|v| {
            if let PgValue::Text(s) = v { Some(s.clone()) } else { None }
        }))
        .collect();

    // Build a UUID array literal for the IN clause (all validated from DB, safe).
    let placeholders: Vec<String> = concept_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let array_literal = placeholders.join(", ");

    let edges_sql = format!(r#"
SELECT DISTINCT ON (neighbour_id)
  CASE
    WHEN cr.source_concept_id = ANY(ARRAY[{array}]::uuid[]) THEN cr.target_concept_id
    ELSE cr.source_concept_id
  END AS neighbour_id,
  cr.relationship_type,
  cr.strength,
  cr.note
FROM concept_relationships cr
WHERE cr.source_concept_id = ANY(ARRAY[{array}]::uuid[])
   OR cr.target_concept_id = ANY(ARRAY[{array}]::uuid[])
ORDER BY neighbour_id, cr.strength DESC
LIMIT 200
"#, array = array_literal);

    let params: Vec<PgValue> = concept_ids
        .iter()
        .map(|id| PgValue::Text(id.clone()))
        .collect();

    let edge_rows = conn
        .execute(&edges_sql, &params)
        .await
        .map_err(|e| async_graphql::Error::new(format!("passage edges query failed: {e}")))?;

    // Resolve neighbour concept metadata in a second query.
    let neighbour_ids: Vec<String> = edge_rows
        .iter()
        .filter_map(|r| r.get("neighbour_id").and_then(|v| {
            if let PgValue::Text(s) = v { Some(s.clone()) } else { None }
        }))
        .collect();

    if neighbour_ids.is_empty() {
        return Ok(vec![]);
    }

    let concept_ph: Vec<String> = neighbour_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();

    let meta_sql = format!(
        "SELECT id::text AS concept_id, lemma, language::text AS language \
         FROM concepts WHERE id = ANY(ARRAY[{}]::uuid[])",
        concept_ph.join(", ")
    );
    let meta_params: Vec<PgValue> = neighbour_ids
        .iter()
        .map(|id| PgValue::Text(id.clone()))
        .collect();

    let meta_rows = conn
        .execute(&meta_sql, &meta_params)
        .await
        .map_err(|e| async_graphql::Error::new(format!("concept meta query failed: {e}")))?;

    // Join edge_rows + meta_rows by neighbour_id.
    let meta_map: std::collections::HashMap<String, (String, String)> = meta_rows
        .iter()
        .map(|r| {
            (
                text_col(r, "concept_id"),
                (text_col(r, "lemma"), text_col(r, "language")),
            )
        })
        .collect();

    Ok(edge_rows
        .into_iter()
        .filter_map(|row| {
            let neighbour_id = text_col(&row, "neighbour_id");
            let (lemma, language) = meta_map.get(&neighbour_id)?.clone();
            Some(ConceptEdge {
                concept_id:        neighbour_id,
                lemma,
                language,
                relationship_type: text_col(&row, "relationship_type"),
                strength:          float_col(&row, "strength"),
                note:              opt_text_col(&row, "note"),
                depth:             1,
            })
        })
        .collect())
}

// ── Row accessors ─────────────────────────────────────────────────────────────

fn text_col(row: &fw_graph_types::PgRow, col: &str) -> String {
    match row.get(col) {
        Some(PgValue::Text(s)) => s.clone(),
        Some(PgValue::Uuid(u)) => u.to_string(),
        _ => String::new(),
    }
}

fn int_col(row: &fw_graph_types::PgRow, col: &str) -> i64 {
    match row.get(col) {
        Some(PgValue::Int(n)) => *n,
        _ => 0,
    }
}

fn float_col(row: &fw_graph_types::PgRow, col: &str) -> f64 {
    match row.get(col) {
        Some(PgValue::Float(f)) => *f,
        Some(PgValue::Int(n))   => *n as f64,
        _ => 0.0,
    }
}

fn opt_text_col(row: &fw_graph_types::PgRow, col: &str) -> Option<String> {
    match row.get(col) {
        Some(PgValue::Text(s)) => Some(s.clone()),
        Some(PgValue::Null) | None => None,
        _ => None,
    }
}
