//! Custom resolver for phrased blocks (chapter-level discourse-structured text).
//!
//! * `phrasedBlocks(book: String!, chapter: Int!): [PhrasedBlock!]!`
//!   — Phrased reading blocks for a chapter from `phrased_blocks`.

use std::sync::Arc;

use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef,
};
use fw_graph_types::PgValue;

use crate::executor::{QueryExecutor, RequestConnection};

// ── Output struct ────────────────────────────────────────────────────────────

/// A phrased block: discourse-structured reading text for a passage.
#[derive(Clone)]
pub struct PhrasedBlock {
    pub passage_ref: String,
    pub block_order: i64,
    pub lines: String,
}

// ── Type registration ────────────────────────────────────────────────────────

/// Build the `PhrasedBlock` GraphQL object type.
pub fn phrased_block_type() -> Object {
    Object::new("PhrasedBlock")
        .field(Field::new("passageRef", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let b = ctx.parent_value.try_downcast_ref::<PhrasedBlock>()?;
                Ok(Some(FieldValue::value(b.passage_ref.clone())))
            })
        }))
        .field(Field::new("blockOrder", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let b = ctx.parent_value.try_downcast_ref::<PhrasedBlock>()?;
                Ok(Some(FieldValue::value(b.block_order)))
            })
        }))
        .field(Field::new("lines", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let b = ctx.parent_value.try_downcast_ref::<PhrasedBlock>()?;
                Ok(Some(FieldValue::value(b.lines.clone())))
            })
        }))
}

// ── phrasedBlocks resolver ──────────────────────────────────────────────────

/// Build `phrasedBlocks(book: String!, chapter: Int!): [PhrasedBlock!]!`
///
/// Returns discourse-structured phrased blocks for a chapter from `phrased_blocks`.
pub fn phrased_blocks_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "phrasedBlocks",
        TypeRef::named_nn_list_nn("PhrasedBlock"),
        |ctx| {
            FieldFuture::new(async move {
                let conn = ctx
                    .data_opt::<RequestConnection>()
                    .ok_or_else(|| async_graphql::Error::new("No database connection"))?;

                let book = ctx.args.try_get("book")?.string()
                    .map_err(|_| async_graphql::Error::new("book must be a string"))?
                    .to_owned();
                let chapter = ctx.args.try_get("chapter")?.i64()
                    .map_err(|_| async_graphql::Error::new("chapter must be an int"))?;

                let blocks = fetch_phrased_blocks(conn, &book, chapter).await?;
                let values: Vec<FieldValue> = blocks.into_iter().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
}

// ── SQL helper ──────────────────────────────────────────────────────────────

async fn fetch_phrased_blocks(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Result<Vec<PhrasedBlock>, async_graphql::Error> {
    let like_pattern = format!("{}.{}.", book, chapter);

    let sql = r#"
SELECT passage_ref, block_order, lines::text
FROM phrased_blocks
WHERE passage_ref LIKE $1
ORDER BY block_order
"#;

    let rows = conn
        .execute(sql, &[PgValue::Text(format!("{}%", like_pattern))])
        .await
        .map_err(|e| async_graphql::Error::new(format!("phrased_blocks query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| PhrasedBlock {
            passage_ref: text_col(&row, "passage_ref"),
            block_order: int_col(&row, "block_order"),
            lines:       text_col(&row, "lines"),
        })
        .collect())
}

// ── Row accessors ───────────────────────────────────────────────────────────

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
