//! Custom resolver factories for the concept reader.
//!
//! * `conceptAlignments(book: String!, chapter: Int!): [ConceptAlignment!]!`
//!   — Enriched concept alignments for a chapter (alignment + concept data joined).
//!
//! * `depthInsights(book: String!, chapter: Int!): [DepthInsight!]!`
//!   — Depth insights for a chapter from `depth_insights` + `depth_insight_links`.
//!
//! * `pericopeContext(book: String!, chapter: Int!): [PericopeUnit!]!`
//!   — Pericope units covering a chapter from `pericope_units`.

use std::sync::Arc;

use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef,
};
use fw_graph_types::PgValue;

use crate::executor::{QueryExecutor, RequestConnection};

// ── Output structs ────────────────────────────────────────────────────────────

/// A depth insight for a passage: theological, linguistic, or contextual annotation.
#[derive(Clone)]
pub struct DepthInsight {
    pub id: String,
    pub passage_ref: String,
    pub insight_type: String,
    pub title: String,
    pub body: String,
    pub related_concept_ids: Vec<String>,
    pub related_passage_refs: Vec<String>,
    pub confidence: f64,
}

/// An enriched concept alignment: alignment fields + joined concept data.
#[derive(Clone)]
pub struct ConceptAlignment {
    pub id: String,
    pub passage_ref: String,
    pub concept_id: String,
    pub english_span: String,
    pub verse: i64,
    pub role: Option<String>,
    pub alignment_note: Option<String>,
    pub confidence: f64,
    pub token_surface_forms: Vec<String>,
    // Joined from concepts table
    pub lemma: String,
    pub language: String,
    pub transliteration: String,
    pub strongs_display: String,
    pub semantic_range: Vec<String>,
    pub theological_note: Option<String>,
    pub occurrence_count: i64,
}

/// A pericope (reading unit) that covers part of a chapter.
#[derive(Clone)]
pub struct PericopeUnit {
    pub id: String,
    pub title: String,
    pub start_ref: String,
    pub end_ref: String,
    pub genre: Option<String>,
    pub structure_note: Option<String>,
    pub anchor_concept_id: Option<String>,
}

// ── Type registration ─────────────────────────────────────────────────────────

/// Register `ConceptAlignment`, `DepthInsight` and `PericopeUnit` object types.
/// Call this before `builder.finish()`.
pub fn register_reader_types(
    builder: async_graphql::dynamic::SchemaBuilder,
) -> async_graphql::dynamic::SchemaBuilder {
    // ── ConceptAlignment type ────────────────────────────────────────────────
    let concept_alignment = Object::new("ConceptAlignment")
        .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(Some(FieldValue::value(a.id.clone())))
            })
        }))
        .field(Field::new("passageRef", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(Some(FieldValue::value(a.passage_ref.clone())))
            })
        }))
        .field(Field::new("conceptId", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(Some(FieldValue::value(a.concept_id.clone())))
            })
        }))
        .field(Field::new("englishSpan", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(Some(FieldValue::value(a.english_span.clone())))
            })
        }))
        .field(Field::new("verse", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(Some(FieldValue::value(a.verse)))
            })
        }))
        .field(Field::new("role", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(a.role.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("alignmentNote", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(a.alignment_note.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("confidence", TypeRef::named_nn(TypeRef::FLOAT), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(Some(FieldValue::value(a.confidence)))
            })
        }))
        .field(Field::new("tokenSurfaceForms", TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                let v: Vec<FieldValue> = a.token_surface_forms.iter().map(|s| FieldValue::value(s.clone())).collect();
                Ok(Some(FieldValue::list(v)))
            })
        }))
        .field(Field::new("lemma", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(Some(FieldValue::value(a.lemma.clone())))
            })
        }))
        .field(Field::new("language", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(Some(FieldValue::value(a.language.clone())))
            })
        }))
        .field(Field::new("transliteration", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(Some(FieldValue::value(a.transliteration.clone())))
            })
        }))
        .field(Field::new("strongsDisplay", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(Some(FieldValue::value(a.strongs_display.clone())))
            })
        }))
        .field(Field::new("semanticRange", TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                let v: Vec<FieldValue> = a.semantic_range.iter().map(|s| FieldValue::value(s.clone())).collect();
                Ok(Some(FieldValue::list(v)))
            })
        }))
        .field(Field::new("theologicalNote", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(a.theological_note.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("occurrenceCount", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let a = ctx.parent_value.try_downcast_ref::<ConceptAlignment>()?;
                Ok(Some(FieldValue::value(a.occurrence_count)))
            })
        }));

    // ── DepthInsight type ────────────────────────────────────────────────────
    let depth_insight = Object::new("DepthInsight")
        .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DepthInsight>()?;
                Ok(Some(FieldValue::value(d.id.clone())))
            })
        }))
        .field(Field::new("passageRef", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DepthInsight>()?;
                Ok(Some(FieldValue::value(d.passage_ref.clone())))
            })
        }))
        .field(Field::new("insightType", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DepthInsight>()?;
                Ok(Some(FieldValue::value(d.insight_type.clone())))
            })
        }))
        .field(Field::new("title", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DepthInsight>()?;
                Ok(Some(FieldValue::value(d.title.clone())))
            })
        }))
        .field(Field::new("body", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DepthInsight>()?;
                Ok(Some(FieldValue::value(d.body.clone())))
            })
        }))
        .field(Field::new("relatedConceptIds", TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DepthInsight>()?;
                let values: Vec<FieldValue> = d.related_concept_ids
                    .iter()
                    .map(|s| FieldValue::value(s.clone()))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("relatedPassageRefs", TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DepthInsight>()?;
                let values: Vec<FieldValue> = d.related_passage_refs
                    .iter()
                    .map(|s| FieldValue::value(s.clone()))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("confidence", TypeRef::named_nn(TypeRef::FLOAT), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DepthInsight>()?;
                Ok(Some(FieldValue::value(d.confidence)))
            })
        }));

    let pericope_unit = Object::new("PericopeUnit")
        .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move {
                let p = ctx.parent_value.try_downcast_ref::<PericopeUnit>()?;
                Ok(Some(FieldValue::value(p.id.clone())))
            })
        }))
        .field(Field::new("title", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let p = ctx.parent_value.try_downcast_ref::<PericopeUnit>()?;
                Ok(Some(FieldValue::value(p.title.clone())))
            })
        }))
        .field(Field::new("startRef", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let p = ctx.parent_value.try_downcast_ref::<PericopeUnit>()?;
                Ok(Some(FieldValue::value(p.start_ref.clone())))
            })
        }))
        .field(Field::new("endRef", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let p = ctx.parent_value.try_downcast_ref::<PericopeUnit>()?;
                Ok(Some(FieldValue::value(p.end_ref.clone())))
            })
        }))
        .field(Field::new("genre", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let p = ctx.parent_value.try_downcast_ref::<PericopeUnit>()?;
                Ok(p.genre.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("structureNote", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let p = ctx.parent_value.try_downcast_ref::<PericopeUnit>()?;
                Ok(p.structure_note.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("anchorConceptId", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let p = ctx.parent_value.try_downcast_ref::<PericopeUnit>()?;
                Ok(p.anchor_concept_id.clone().map(FieldValue::value))
            })
        }));

    builder
        .register(concept_alignment)
        .register(depth_insight)
        .register(pericope_unit)
}

// ── conceptAlignments resolver ───────────────────────────────────────────────

/// Build `conceptAlignments(book: String!, chapter: Int!): [ConceptAlignment!]!`
///
/// Returns enriched concept alignments for a chapter, joining concept_alignments
/// with concepts to include lemma, semantic_range, transliteration, etc.
pub fn concept_alignments_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "conceptAlignments",
        TypeRef::named_nn_list_nn("ConceptAlignment"),
        move |ctx| {
            FieldFuture::new(async move {
                let book: String = ctx.args.try_get("book")?.string()?.to_owned();
                let chapter: i64 = ctx.args.try_get("chapter")?.i64()?;
                let conn = ctx.data::<RequestConnection>()?;
                let alignments = fetch_concept_alignments(conn, &book, chapter).await?;
                let values: Vec<FieldValue> = alignments
                    .into_iter()
                    .map(FieldValue::owned_any)
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
}

// ── depthInsights resolver ───────────────────────────────────────────────────

/// Build `depthInsights(book: String!, chapter: Int!): [DepthInsight!]!`
///
/// Fetches depth insights for a chapter from `depth_insights`, including
/// insights linked via `depth_insight_links`.
pub fn depth_insights_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "depthInsights",
        TypeRef::named_nn_list_nn("DepthInsight"),
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
                    .map_err(|_| async_graphql::Error::new("chapter must be an int"))?;

                let insights = fetch_depth_insights(conn, &book, chapter).await?;

                let values: Vec<FieldValue> = insights
                    .into_iter()
                    .map(|d| FieldValue::owned_any(d))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
}

// ── pericopeContext resolver ─────────────────────────────────────────────────

/// Build `pericopeContext(book: String!, chapter: Int!): [PericopeUnit!]!`
///
/// Fetches pericope units whose start_ref falls within the given chapter.
pub fn pericope_context_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "pericopeContext",
        TypeRef::named_nn_list_nn("PericopeUnit"),
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
                    .map_err(|_| async_graphql::Error::new("chapter must be an int"))?;

                let units = fetch_pericope_context(conn, &book, chapter).await?;

                let values: Vec<FieldValue> = units
                    .into_iter()
                    .map(|p| FieldValue::owned_any(p))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
}

// ── SQL helpers ──────────────────────────────────────────────────────────────

/// Fetch depth insights for a chapter, including cross-linked insights.
async fn fetch_depth_insights(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Result<Vec<DepthInsight>, async_graphql::Error> {
    let prefix = format!("{}.{}.", book, chapter);

    let sql = r#"
SELECT
  di.id::text,
  di.passage_ref,
  di.insight_type,
  di.title,
  di.body,
  di.related_concept_ids::text[] AS related_concept_ids,
  di.related_passage_refs,
  di.confidence
FROM depth_insights di
WHERE di.passage_ref LIKE $1
   OR EXISTS (
     SELECT 1 FROM depth_insight_links dil
     WHERE dil.insight_id = di.id
       AND dil.linked_passage_ref LIKE $1
   )
ORDER BY di.insight_type, di.passage_ref
"#;

    let like_pattern = format!("{}%", prefix);

    let rows = conn
        .execute(sql, &[PgValue::Text(like_pattern)])
        .await
        .map_err(|e| async_graphql::Error::new(format!("depth_insights query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let related_concept_ids = text_array_col(&row, "related_concept_ids");
            let related_passage_refs = text_array_col(&row, "related_passage_refs");

            DepthInsight {
                id:                  text_col(&row, "id"),
                passage_ref:         text_col(&row, "passage_ref"),
                insight_type:        text_col(&row, "insight_type"),
                title:               text_col(&row, "title"),
                body:                text_col(&row, "body"),
                related_concept_ids,
                related_passage_refs,
                confidence:          float_col(&row, "confidence"),
            }
        })
        .collect())
}

/// Fetch pericope units whose start_ref falls within the given chapter.
async fn fetch_pericope_context(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Result<Vec<PericopeUnit>, async_graphql::Error> {
    let prefix = format!("{}.{}.", book, chapter);

    let sql = r#"
SELECT
  pu.id::text,
  pu.title,
  pu.start_ref,
  pu.end_ref,
  pu.genre,
  pu.structure_note,
  pu.anchor_concept_id::text
FROM pericope_units pu
WHERE pu.start_ref LIKE $1
ORDER BY pu.start_ref
"#;

    let like_pattern = format!("{}%", prefix);

    let rows = conn
        .execute(sql, &[PgValue::Text(like_pattern)])
        .await
        .map_err(|e| async_graphql::Error::new(format!("pericope_context query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| PericopeUnit {
            id:                text_col(&row, "id"),
            title:             text_col(&row, "title"),
            start_ref:         text_col(&row, "start_ref"),
            end_ref:           text_col(&row, "end_ref"),
            genre:             opt_text_col(&row, "genre"),
            structure_note:    opt_text_col(&row, "structure_note"),
            anchor_concept_id: opt_text_col(&row, "anchor_concept_id"),
        })
        .collect())
}

/// Fetch enriched concept alignments for a chapter.
async fn fetch_concept_alignments(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Result<Vec<ConceptAlignment>, async_graphql::Error> {
    let prefix = format!("{}.{}.", book, chapter);

    let sql = r#"
SELECT
  ca.id::text,
  ca.passage_ref,
  ca.concept_id::text,
  ca.english_span,
  ca.verse,
  ca.role,
  ca.alignment_note,
  ca.confidence,
  ca.token_surface_forms,
  c.lemma,
  c.language,
  c.transliteration,
  COALESCE(c.strongs_display, '') AS strongs_display,
  c.semantic_range,
  c.theological_note,
  COALESCE(c.occurrence_count, 0) AS occurrence_count
FROM concept_alignments ca
JOIN concepts c ON c.id = ca.concept_id
WHERE ca.passage_ref LIKE $1
ORDER BY ca.verse, ca.english_token_start
"#;

    let like_pattern = format!("{}%", prefix);

    let rows = conn
        .execute(sql, &[PgValue::Text(like_pattern)])
        .await
        .map_err(|e| async_graphql::Error::new(format!("concept_alignments query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let semantic_range = match row.get("semantic_range") {
                Some(PgValue::Array(arr)) => arr.iter().filter_map(|v| match v {
                    PgValue::Text(s) => Some(s.clone()),
                    _ => None,
                }).collect(),
                Some(PgValue::Text(s)) if s.starts_with('[') => {
                    // JSON array string: ["word","reason"]
                    serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
                }
                _ => vec![],
            };

            ConceptAlignment {
                id:                  text_col(&row, "id"),
                passage_ref:         text_col(&row, "passage_ref"),
                concept_id:          text_col(&row, "concept_id"),
                english_span:        text_col(&row, "english_span"),
                verse:               int_col(&row, "verse"),
                role:                opt_text_col(&row, "role"),
                alignment_note:      opt_text_col(&row, "alignment_note"),
                confidence:          float_col(&row, "confidence"),
                token_surface_forms: text_array_col(&row, "token_surface_forms"),
                lemma:               text_col(&row, "lemma"),
                language:            text_col(&row, "language"),
                transliteration:     text_col(&row, "transliteration"),
                strongs_display:     text_col(&row, "strongs_display"),
                semantic_range,
                theological_note:    opt_text_col(&row, "theological_note"),
                occurrence_count:    int_col(&row, "occurrence_count"),
            }
        })
        .collect())
}

// ── Row accessors ────────────────────────────────────────────────────────────

fn text_col(row: &fw_graph_types::PgRow, col: &str) -> String {
    match row.get(col) {
        Some(PgValue::Text(s)) => s.clone(),
        Some(PgValue::Uuid(u)) => u.to_string(),
        _ => String::new(),
    }
}

#[allow(dead_code)]
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

/// Extract a text array column. Handles `Array(Vec<PgValue>)` from the driver,
/// or falls back to parsing the Postgres text representation `{val1,val2,...}`.
fn text_array_col(row: &fw_graph_types::PgRow, col: &str) -> Vec<String> {
    match row.get(col) {
        Some(PgValue::Array(arr)) => arr
            .iter()
            .filter_map(|v| match v {
                PgValue::Text(s) => Some(s.clone()),
                PgValue::Uuid(u) => Some(u.to_string()),
                _ => None,
            })
            .collect(),
        Some(PgValue::Text(s)) if s.starts_with('{') && s.ends_with('}') => {
            // Postgres text representation of an array: {val1,val2,...}
            s[1..s.len() - 1]
                .split(',')
                .filter(|v| !v.is_empty())
                .map(|v| v.trim_matches('"').to_owned())
                .collect()
        }
        Some(PgValue::Text(s)) if !s.is_empty() => vec![s.clone()],
        _ => vec![],
    }
}
