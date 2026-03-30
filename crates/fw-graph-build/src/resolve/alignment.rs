//! Passage alignment resolver — treebank-driven interlinear + indented display.
//!
//! Registers one top-level Query field:
//!
//! * `passageAlignment(book: String!, chapterStart: Int!, verseStart: Int!,
//!                     chapterEnd: Int!, verseEnd: Int!): [AlignedClause!]!`
//!
//! Each `AlignedClause` is one depth-1 clause node from `phrase_structure_nodes`
//! (BHSA for OT, OpenText for NT), with its tokens joined to `english_tokens`
//! via position. The `indent` field is derived from the clause's `role_label`:
//! main narrative clauses → 0, subordinate/background → 1.
//!
//! The entire verse range is fetched in a single batched SQL query.

use std::sync::Arc;

use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef,
};
use fw_graph_types::PgValue;

use crate::executor::{QueryExecutor, RequestConnection};

// ── Output structs ────────────────────────────────────────────────────────────

/// One token within a clause: the original-language token paired with its
/// English equivalent from the BSB interlinear.
#[derive(Clone)]
pub struct AlignedToken {
    pub position:       i32,
    pub surface_form:   String,
    pub lemma:          String,
    pub strong_number:  String,
    pub morphology_code: String,
    pub transliteration: String,
    pub english_text:   String,
}

/// One clause node from the treebank, with its tokens and display indent.
#[derive(Clone)]
pub struct AlignedClause {
    pub book:       String,
    pub chapter:    i32,
    pub verse:      i32,
    pub indent:     i32,
    pub role_label: String,
    pub tokens:     Vec<AlignedToken>,
}

// ── Type registration ─────────────────────────────────────────────────────────

/// Register `AlignedToken` and `AlignedClause` object types.
/// Must be called before `builder.finish()`.
pub fn register_alignment_types(
    builder: async_graphql::dynamic::SchemaBuilder,
) -> async_graphql::dynamic::SchemaBuilder {
    let aligned_token = Object::new("AlignedToken")
        .field(Field::new("position", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let t = ctx.parent_value.try_downcast_ref::<AlignedToken>()?;
                Ok(Some(FieldValue::value(t.position as i64)))
            })
        }))
        .field(Field::new("surfaceForm", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let t = ctx.parent_value.try_downcast_ref::<AlignedToken>()?;
                Ok(Some(FieldValue::value(t.surface_form.clone())))
            })
        }))
        .field(Field::new("lemma", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let t = ctx.parent_value.try_downcast_ref::<AlignedToken>()?;
                Ok(Some(FieldValue::value(t.lemma.clone())))
            })
        }))
        .field(Field::new("strongNumber", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let t = ctx.parent_value.try_downcast_ref::<AlignedToken>()?;
                Ok(Some(FieldValue::value(t.strong_number.clone())))
            })
        }))
        .field(Field::new("morphologyCode", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let t = ctx.parent_value.try_downcast_ref::<AlignedToken>()?;
                Ok(Some(FieldValue::value(t.morphology_code.clone())))
            })
        }))
        .field(Field::new("transliteration", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let t = ctx.parent_value.try_downcast_ref::<AlignedToken>()?;
                Ok(Some(FieldValue::value(t.transliteration.clone())))
            })
        }))
        .field(Field::new("englishText", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let t = ctx.parent_value.try_downcast_ref::<AlignedToken>()?;
                Ok(Some(FieldValue::value(t.english_text.clone())))
            })
        }));

    let aligned_clause = Object::new("AlignedClause")
        .field(Field::new("book", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<AlignedClause>()?;
                Ok(Some(FieldValue::value(c.book.clone())))
            })
        }))
        .field(Field::new("chapter", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<AlignedClause>()?;
                Ok(Some(FieldValue::value(c.chapter as i64)))
            })
        }))
        .field(Field::new("verse", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<AlignedClause>()?;
                Ok(Some(FieldValue::value(c.verse as i64)))
            })
        }))
        .field(Field::new("indent", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<AlignedClause>()?;
                Ok(Some(FieldValue::value(c.indent as i64)))
            })
        }))
        .field(Field::new("roleLabel", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<AlignedClause>()?;
                Ok(Some(FieldValue::value(c.role_label.clone())))
            })
        }))
        .field(Field::new("tokens", TypeRef::named_nn_list_nn("AlignedToken"), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<AlignedClause>()?;
                let values: Vec<FieldValue> = c.tokens
                    .iter()
                    .cloned()
                    .map(FieldValue::owned_any)
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        }));

    builder
        .register(aligned_token)
        .register(aligned_clause)
}

// ── passageAlignment resolver ─────────────────────────────────────────────────

/// Build `passageAlignment(book, chapterStart, verseStart, chapterEnd, verseEnd): [AlignedClause!]!`
pub fn build_passage_alignment_resolver(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "passageAlignment",
        TypeRef::named_nn_list_nn("AlignedClause"),
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

                let chapter_start = ctx
                    .args
                    .try_get("chapterStart")?
                    .i64()
                    .map_err(|_| async_graphql::Error::new("chapterStart must be an int"))?
                    as i32;

                let verse_start = ctx
                    .args
                    .try_get("verseStart")?
                    .i64()
                    .map_err(|_| async_graphql::Error::new("verseStart must be an int"))?
                    as i32;

                let chapter_end = ctx
                    .args
                    .try_get("chapterEnd")?
                    .i64()
                    .map_err(|_| async_graphql::Error::new("chapterEnd must be an int"))?
                    as i32;

                let verse_end = ctx
                    .args
                    .try_get("verseEnd")?
                    .i64()
                    .map_err(|_| async_graphql::Error::new("verseEnd must be an int"))?
                    as i32;

                let clauses = fetch_passage_alignment(
                    conn,
                    &book,
                    chapter_start,
                    verse_start,
                    chapter_end,
                    verse_end,
                )
                .await?;

                let values: Vec<FieldValue> = clauses
                    .into_iter()
                    .map(FieldValue::owned_any)
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("book",         TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapterStart", TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("verseStart",   TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("chapterEnd",   TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("verseEnd",     TypeRef::named_nn(TypeRef::INT)))
}

// ── SQL ───────────────────────────────────────────────────────────────────────

/// Single batched query: fetch all depth-1 clause nodes for the verse range,
/// unnest their token_positions[], join to passage_tokens + english_tokens,
/// and derive indent from role_label.
///
/// `english_tokens` uses 1-based position; `passage_tokens` uses 0-based.
/// The join is: `et.position = pt.position + 1` within the same (book, chapter, verse).
///
/// Indent logic:
///   - depth-0 root clause nodes → always indent 0 (they frame the verse)
///   - depth-1 nodes with a main-clause role_label pattern → indent 0
///   - depth-1 nodes that are subordinate/background → indent 1
///
/// Main clause indicators in BHSA role_label: contains "V-" (predicate-verb)
/// and does NOT start with "Prepositional", "Object", "Subject" alone.
/// Disjunctive/subordinate: "S-P" (subject-predicator without verb),
/// prepositional phrases, object phrases, "PP-" patterns.
async fn fetch_passage_alignment(
    conn: &RequestConnection,
    book: &str,
    chapter_start: i32,
    verse_start: i32,
    chapter_end: i32,
    verse_end: i32,
) -> Result<Vec<AlignedClause>, async_graphql::Error> {
    // Fetch the clause nodes with their token positions for the range.
    // We treat (chapter, verse) as a linear position using chapter*1000+verse
    // so a single comparison handles multi-chapter spans cleanly.
    let clause_sql = r#"
SELECT
    psn.id::text          AS node_id,
    psn.book,
    psn.chapter,
    psn.verse,
    psn.depth,
    psn.role_label,
    psn.sibling_order,
    psn.token_positions,
    CASE
        -- depth-0 root is always the outer frame, rendered at 0
        WHEN psn.depth = 0 THEN 0
        -- depth-1: main narrative clause (has a verbal predicate role)
        WHEN psn.depth = 1
             AND psn.role_label ~ '(^Clause\s+(V-|S-V|V-S|S-V-O|V-O|S-V-PP|V-S-PP|V-S-O|PP-V-S|PP-V-O))'
             THEN 0
        -- depth-1: subordinate, background, or purely nominal
        ELSE 1
    END AS indent
FROM phrase_structure_nodes psn
WHERE psn.book = $1
  AND (psn.chapter * 1000 + psn.verse) >= ($2 * 1000 + $3)
  AND (psn.chapter * 1000 + psn.verse) <= ($4 * 1000 + $5)
  AND psn.depth <= 1
ORDER BY psn.chapter, psn.verse, psn.depth, psn.sibling_order
"#;

    let clause_rows = conn
        .execute(
            clause_sql,
            &[
                PgValue::Text(book.to_owned()),
                PgValue::Int(chapter_start as i64),
                PgValue::Int(verse_start as i64),
                PgValue::Int(chapter_end as i64),
                PgValue::Int(verse_end as i64),
            ],
        )
        .await
        .map_err(|e| async_graphql::Error::new(format!("clause query failed: {e}")))?;

    if clause_rows.is_empty() {
        return Ok(vec![]);
    }

    // Fetch all tokens for the verse range in one query, joining
    // passage_tokens (0-based position) to english_tokens (1-based position).
    let token_sql = r#"
SELECT
    pt.position                          AS position,
    p.chapter,
    p.verse,
    pt.surface_form,
    pt.lemma,
    COALESCE(pt.strong_number, '')       AS strong_number,
    COALESCE(pt.morphology_code, '')     AS morphology_code,
    COALESCE(pt.transliteration, '')     AS transliteration,
    COALESCE(et.english_text, '')        AS english_text
FROM passages p
JOIN passage_tokens pt ON pt.passage_id = p.id
LEFT JOIN english_tokens et
       ON et.book    = p.book
      AND et.chapter = p.chapter
      AND et.verse   = p.verse
      AND et.position = pt.position + 1
WHERE p.book = $1
  AND (p.chapter * 1000 + p.verse) >= ($2 * 1000 + $3)
  AND (p.chapter * 1000 + p.verse) <= ($4 * 1000 + $5)
ORDER BY p.chapter, p.verse, pt.position
"#;

    let token_rows = conn
        .execute(
            token_sql,
            &[
                PgValue::Text(book.to_owned()),
                PgValue::Int(chapter_start as i64),
                PgValue::Int(verse_start as i64),
                PgValue::Int(chapter_end as i64),
                PgValue::Int(verse_end as i64),
            ],
        )
        .await
        .map_err(|e| async_graphql::Error::new(format!("token query failed: {e}")))?;

    // Index tokens by (chapter, verse, position) for fast lookup.
    let mut token_index: std::collections::HashMap<(i32, i32, i32), AlignedToken> =
        std::collections::HashMap::new();
    for row in &token_rows {
        let chapter  = int_col(row, "chapter") as i32;
        let verse    = int_col(row, "verse") as i32;
        let position = int_col(row, "position") as i32;
        token_index.insert(
            (chapter, verse, position),
            AlignedToken {
                position,
                surface_form:    text_col(row, "surface_form"),
                lemma:           text_col(row, "lemma"),
                strong_number:   text_col(row, "strong_number"),
                morphology_code: text_col(row, "morphology_code"),
                transliteration: text_col(row, "transliteration"),
                english_text:    text_col(row, "english_text"),
            },
        );
    }

    // Assemble AlignedClause objects from clause rows + token lookups.
    // Skip depth-0 root nodes unless they are the only node for a verse
    // (prevents double-rendering the verse frame and its children).
    let mut clauses: Vec<AlignedClause> = Vec::new();

    // Track which verses have at least one depth-1 clause so we can decide
    // whether to include the depth-0 frame.
    let mut verses_with_depth1: std::collections::HashSet<(i32, i32)> =
        std::collections::HashSet::new();
    for row in &clause_rows {
        if int_col(row, "depth") == 1 {
            verses_with_depth1.insert((
                int_col(row, "chapter") as i32,
                int_col(row, "verse") as i32,
            ));
        }
    }

    for row in &clause_rows {
        let chapter = int_col(row, "chapter") as i32;
        let verse   = int_col(row, "verse") as i32;
        let depth   = int_col(row, "depth") as i32;

        // Skip depth-0 root when depth-1 children exist — children carry
        // the structural information; root would duplicate the whole verse.
        if depth == 0 && verses_with_depth1.contains(&(chapter, verse)) {
            continue;
        }

        let indent     = int_col(row, "indent") as i32;
        let role_label = text_col(row, "role_label");

        // Collect tokens for this clause node by looking up each position
        // in token_positions[].
        let mut tokens: Vec<AlignedToken> = Vec::new();
        if let Some(fw_graph_types::PgValue::Array(positions)) = row.get("token_positions") {
            for pos_val in positions {
                let pos = match pos_val {
                    fw_graph_types::PgValue::Int(n) => *n as i32,
                    _ => continue,
                };
                // phrase_structure_nodes token_positions are 1-based to match
                // passage_tokens 0-based: subtract 1.
                if let Some(token) = token_index.get(&(chapter, verse, pos - 1)) {
                    tokens.push(token.clone());
                }
            }
        }

        // Sort tokens by position so they render left-to-right (or RTL for
        // Hebrew — the UI handles directionality via CSS/lang attributes).
        tokens.sort_by_key(|t| t.position);

        if !tokens.is_empty() {
            clauses.push(AlignedClause {
                book: book.to_owned(),
                chapter,
                verse,
                indent,
                role_label,
                tokens,
            });
        }
    }

    Ok(clauses)
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
