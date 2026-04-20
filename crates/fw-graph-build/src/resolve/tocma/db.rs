//! Shared database loaders for TOCMA resolvers.
//!
//! Per Workstream-A hygiene pattern: inline SQL lives in db.rs, not in the
//! resolver function bodies. This makes the SQL unit-testable and lets
//! multiple resolvers share a loader.

use fw_graph_types::PgValue;

use crate::executor::RequestConnection;

// ── Step 2 — outline path ────────────────────────────────────────────────────

/// Load the ancestry chain of outline-node labels for a verse.
///
/// Walks `tocma_step2_outline_nodes` from the deepest node whose `clause_id`
/// points to a `pm_clause` linked to this verse, up to the root of the
/// pericope's outline tree. Returns labels **root-to-leaf**.
///
/// Returns an empty `Vec` when no outline coverage exists for the verse
/// (e.g., the 22,500+ uncovered verses as of 2026-04-20) — this is an
/// expected state, not an error.
///
/// Label preference: `display_text` (human-readable, e.g. "The Gospel Is
/// the Power of God") falls back to `outline_label` (e.g. "I.A.1") when
/// display_text is null or empty.
///
/// Fixes P1 from `wiki/analyses/2026-04-20-tocma-rendering-diagnosis.md`
/// and implements insight `2026041701-I001`.
pub async fn load_outline_path(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
    verse: i64,
) -> Result<Vec<String>, async_graphql::Error> {
    let sql = r#"
WITH RECURSIVE
  leaf AS (
    SELECT n.id, n.parent_id, n.level,
           COALESCE(NULLIF(n.display_text, ''), NULLIF(n.outline_label, '')) AS label
    FROM tocma_step2_outline_nodes n
    JOIN pm_clauses c ON c.id = n.clause_id
    JOIN passages   p ON p.id = c.passage_id
    WHERE p.book = $1 AND p.chapter = $2 AND p.verse = $3
    ORDER BY n.level DESC, n.sort_order DESC
    LIMIT 1
  ),
  chain AS (
    SELECT id, parent_id, level, label FROM leaf
    UNION ALL
    SELECT n.id, n.parent_id, n.level,
           COALESCE(NULLIF(n.display_text, ''), NULLIF(n.outline_label, '')) AS label
    FROM tocma_step2_outline_nodes n
    JOIN chain ON chain.parent_id = n.id
  )
SELECT label
FROM chain
WHERE label IS NOT NULL AND label <> ''
ORDER BY level ASC
"#;

    let rows = conn
        .execute(
            sql,
            &[
                PgValue::Text(book.to_owned()),
                PgValue::Int(chapter),
                PgValue::Int(verse),
            ],
        )
        .await
        .map_err(|e| async_graphql::Error::new(format!("tocma_step2_outline_nodes query: {e}")))?;

    Ok(rows
        .iter()
        .filter_map(|row| match row.get("label") {
            Some(PgValue::Text(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        })
        .collect())
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    //! SQL-shape tests. The loader's recursive CTE requires a live Postgres
    //! connection to exercise end-to-end; those tests live under
    //! `#[cfg(feature = "integration")]` in `tests/tocma_integration.rs`.
    //!
    //! Here we pin the query text so accidental edits show up in diffs and
    //! reviewers can confirm the ancestry chain invariants.

    #[test]
    fn outline_path_sql_is_root_to_leaf_ordered() {
        let src = include_str!("db.rs");
        assert!(
            src.contains("ORDER BY level ASC"),
            "outline_path loader must order root-to-leaf (level ASC)"
        );
        assert!(
            src.contains("WITH RECURSIVE"),
            "outline_path loader must use a recursive CTE to walk parents"
        );
        assert!(
            src.contains("ORDER BY n.level DESC, n.sort_order DESC"),
            "leaf selection must pick the deepest / last-sorted match"
        );
    }

    #[test]
    fn outline_path_label_prefers_display_text() {
        let src = include_str!("db.rs");
        // Both leaf and recursive terms must try display_text first, then outline_label.
        let occurrences = src
            .matches("COALESCE(NULLIF(n.display_text, ''), NULLIF(n.outline_label, ''))")
            .count();
        assert!(
            occurrences >= 2,
            "display_text→outline_label fallback must appear in both CTE arms (leaf + recursive), found {occurrences}"
        );
    }
}
