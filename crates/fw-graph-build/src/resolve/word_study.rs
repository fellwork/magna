//! Word Study resolver — standalone lemma-scoped word study data.
//!
//! * `wordStudy(lemma: String!, languageCode: String!): WordStudyResult`
//!   — Returns vocabulary identity, word study commentary entries, cross-references,
//!     and concordance book counts for a lemma without requiring a verse context.
//!
//! This is the data layer for the `/word/:lang/:lemma` spoke page.

use std::sync::Arc;

use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef,
};
use fw_graph_types::PgValue;

use crate::executor::{QueryExecutor, RequestConnection};

// ── Output structs ────────────────────────────────────────────────────────────

/// A word study entry from commentary extraction.
#[derive(Clone)]
struct StudyEntry {
    pub series: String,
    pub book: String,
    pub issue_type: String,
    pub issue_description: String,
    pub content: String,
}

/// A cross-reference linked to this vocabulary item's occurrences.
#[derive(Clone)]
struct StudyCrossRef {
    pub book: String,
    pub verse_ref: String,
    pub target_ref: String,
    pub relationship: String,
    pub note: Option<String>,
}

/// Concordance count per book.
#[derive(Clone)]
struct BookCount {
    pub book: String,
    pub count: i64,
}

/// Complete word study result for a lemma.
#[derive(Clone)]
struct WordStudyResult {
    // From vocabulary_items
    pub id: String,
    pub lemma: String,
    pub language_code: String,
    pub transliteration: Option<String>,
    pub ipa: Option<String>,
    pub short_gloss: Option<String>,
    pub part_of_speech: Option<String>,
    pub frequency_count: Option<i64>,
    pub frequency_rank: Option<i64>,
    pub audio_url: Option<String>,
    pub notes: Option<String>,
    pub extended_notes: Option<String>,

    // Aggregated
    pub study_entries: Vec<StudyEntry>,
    pub cross_refs: Vec<StudyCrossRef>,
    pub book_counts: Vec<BookCount>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn text_col(row: &fw_graph_types::PgRow, col: &str) -> String {
    match row.get(col) {
        Some(PgValue::Text(s)) => s.clone(),
        _ => String::new(),
    }
}

fn opt_text_col(row: &fw_graph_types::PgRow, col: &str) -> Option<String> {
    match row.get(col) {
        Some(PgValue::Text(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn opt_int_col(row: &fw_graph_types::PgRow, col: &str) -> Option<i64> {
    match row.get(col) {
        Some(PgValue::Int(n)) => Some(*n),
        _ => None,
    }
}

fn int_col(row: &fw_graph_types::PgRow, col: &str) -> i64 {
    match row.get(col) {
        Some(PgValue::Int(n)) => *n,
        _ => 0,
    }
}

// ── Type registration ─────────────────────────────────────────────────────────

pub fn register_word_study_types(
    builder: async_graphql::dynamic::SchemaBuilder,
) -> async_graphql::dynamic::SchemaBuilder {
    let study_entry = Object::new("StudyEntry")
        .field(Field::new("series", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<StudyEntry>()?;
                Ok(Some(FieldValue::value(e.series.clone())))
            })
        }))
        .field(Field::new("book", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<StudyEntry>()?;
                Ok(Some(FieldValue::value(e.book.clone())))
            })
        }))
        .field(Field::new("issueType", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<StudyEntry>()?;
                Ok(Some(FieldValue::value(e.issue_type.clone())))
            })
        }))
        .field(Field::new("issueDescription", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<StudyEntry>()?;
                Ok(Some(FieldValue::value(e.issue_description.clone())))
            })
        }))
        .field(Field::new("content", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<StudyEntry>()?;
                Ok(Some(FieldValue::value(e.content.clone())))
            })
        }));

    let study_cross_ref = Object::new("StudyCrossRef")
        .field(Field::new("book", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<StudyCrossRef>()?;
                Ok(Some(FieldValue::value(c.book.clone())))
            })
        }))
        .field(Field::new("verseRef", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<StudyCrossRef>()?;
                Ok(Some(FieldValue::value(c.verse_ref.clone())))
            })
        }))
        .field(Field::new("targetRef", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<StudyCrossRef>()?;
                Ok(Some(FieldValue::value(c.target_ref.clone())))
            })
        }))
        .field(Field::new("relationship", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<StudyCrossRef>()?;
                Ok(Some(FieldValue::value(c.relationship.clone())))
            })
        }))
        .field(Field::new("note", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<StudyCrossRef>()?;
                Ok(c.note.clone().map(FieldValue::value))
            })
        }));

    let book_count = Object::new("BookCount")
        .field(Field::new("book", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let b = ctx.parent_value.try_downcast_ref::<BookCount>()?;
                Ok(Some(FieldValue::value(b.book.clone())))
            })
        }))
        .field(Field::new("count", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let b = ctx.parent_value.try_downcast_ref::<BookCount>()?;
                Ok(Some(FieldValue::value(b.count)))
            })
        }));

    let word_study_result = Object::new("WordStudyResult")
        .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(Some(FieldValue::value(r.id.clone())))
            })
        }))
        .field(Field::new("lemma", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(Some(FieldValue::value(r.lemma.clone())))
            })
        }))
        .field(Field::new("languageCode", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(Some(FieldValue::value(r.language_code.clone())))
            })
        }))
        .field(Field::new("transliteration", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(r.transliteration.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("ipa", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(r.ipa.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("shortGloss", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(r.short_gloss.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("partOfSpeech", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(r.part_of_speech.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("frequencyCount", TypeRef::named(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(r.frequency_count.map(FieldValue::value))
            })
        }))
        .field(Field::new("frequencyRank", TypeRef::named(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(r.frequency_rank.map(FieldValue::value))
            })
        }))
        .field(Field::new("audioUrl", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(r.audio_url.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("notes", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(r.notes.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("extendedNotes", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(r.extended_notes.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("studyEntries", TypeRef::named_nn_list_nn("StudyEntry"), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(Some(FieldValue::list(
                    r.study_entries.iter().map(|e| FieldValue::owned_any(e.clone())).collect::<Vec<_>>(),
                )))
            })
        }))
        .field(Field::new("crossRefs", TypeRef::named_nn_list_nn("StudyCrossRef"), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(Some(FieldValue::list(
                    r.cross_refs.iter().map(|c| FieldValue::owned_any(c.clone())).collect::<Vec<_>>(),
                )))
            })
        }))
        .field(Field::new("bookCounts", TypeRef::named_nn_list_nn("BookCount"), |ctx| {
            FieldFuture::new(async move {
                let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
                Ok(Some(FieldValue::list(
                    r.book_counts.iter().map(|b| FieldValue::owned_any(b.clone())).collect::<Vec<_>>(),
                )))
            })
        }));

    builder
        .register(study_entry)
        .register(study_cross_ref)
        .register(book_count)
        .register(word_study_result)
}

// ── Field factory ─────────────────────────────────────────────────────────────

pub fn word_study_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "wordStudy",
        TypeRef::named("WordStudyResult"),
        |ctx| {
            FieldFuture::new(async move {
                let conn = ctx
                    .data_opt::<RequestConnection>()
                    .ok_or_else(|| async_graphql::Error::new("No database connection"))?;

                let lemma = ctx
                    .args
                    .try_get("lemma")?
                    .string()
                    .map_err(|_| async_graphql::Error::new("lemma must be a string"))?
                    .to_owned();

                let language_code = ctx
                    .args
                    .try_get("languageCode")?
                    .string()
                    .map_err(|_| async_graphql::Error::new("languageCode must be a string"))?
                    .to_owned();

                let result = fetch_word_study(conn, &lemma, &language_code).await?;
                match result {
                    Some(r) => Ok(Some(FieldValue::owned_any(r))),
                    None => Ok(None),
                }
            })
        },
    )
    .argument(InputValue::new("lemma", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("languageCode", TypeRef::named_nn(TypeRef::STRING)))
}

// ── Data fetch ────────────────────────────────────────────────────────────────

async fn fetch_word_study(
    conn: &RequestConnection,
    lemma: &str,
    language_code: &str,
) -> Result<Option<WordStudyResult>, async_graphql::Error> {
    // 1. Fetch the vocabulary item by lemma + language_code
    let vocab_rows = conn
        .execute(
            "SELECT id, lemma, language_code, transliteration, ipa, short_gloss,
                    part_of_speech, frequency_count, frequency_rank, audio_url,
                    notes, extended_notes
             FROM vocabulary_items
             WHERE lemma = $1 AND language_code = $2
             LIMIT 1",
            &[PgValue::Text(lemma.to_owned()), PgValue::Text(language_code.to_owned())],
        )
        .await
        .map_err(|e| async_graphql::Error::new(format!("vocabulary_items query: {e}")))?;

    let vocab_row = match vocab_rows.first() {
        Some(r) => r,
        None => return Ok(None), // lemma not found
    };

    let vocab_id = text_col(vocab_row, "id");

    // 2. Fetch word study entries linked to this vocabulary item
    //    COALESCE handles schema difference: prod uses harris_summary, local uses editor_summary
    let study_rows = conn
        .execute(
            "SELECT ws.book,
                    COALESCE(ws.issue_type::text, 'word_meaning') AS issue_type,
                    COALESCE(ws.issue_description, '') AS issue_description,
                    COALESCE(ws.harris_summary, ws.editor_summary, '') AS content,
                    COALESCE(cs.series_name, es.series_name, 'Unknown') AS series
             FROM word_study_entries ws
             LEFT JOIN commentary_sources cs ON cs.id = ws.source_id
             LEFT JOIN eggnt_sources es ON es.id = ws.source_id
             WHERE ws.vocabulary_item_id = $1::uuid
             ORDER BY ws.book, ws.created_at",
            &[PgValue::Text(vocab_id.clone())],
        )
        .await
        .unwrap_or_default();

    let study_entries: Vec<StudyEntry> = study_rows
        .iter()
        .map(|r| StudyEntry {
            series: text_col(r, "series"),
            book: text_col(r, "book"),
            issue_type: text_col(r, "issue_type"),
            issue_description: text_col(r, "issue_description"),
            content: text_col(r, "content"),
        })
        .collect();

    // 3. Fetch cross-references from books where this word appears
    let cross_ref_rows = conn
        .execute(
            "SELECT cr.book, cr.verse_ref, cr.target_ref, cr.relationship, cr.note
             FROM commentary_cross_references cr
             WHERE cr.book IN (
                 SELECT DISTINCT ws2.book FROM word_study_entries ws2
                 WHERE ws2.vocabulary_item_id = $1::uuid
             )
             ORDER BY cr.book, cr.verse_ref
             LIMIT 50",
            &[PgValue::Text(vocab_id.clone())],
        )
        .await
        .unwrap_or_default();

    let cross_refs: Vec<StudyCrossRef> = cross_ref_rows
        .iter()
        .map(|r| StudyCrossRef {
            book: text_col(r, "book"),
            verse_ref: text_col(r, "verse_ref"),
            target_ref: text_col(r, "target_ref"),
            relationship: text_col(r, "relationship"),
            note: opt_text_col(r, "note"),
        })
        .collect();

    // 4. Fetch concordance book counts via concept_alignments
    let count_rows = conn
        .execute(
            "SELECT p.book, COUNT(*) AS count
             FROM concept_alignments ca
             JOIN passages p ON p.id = ca.passage_id
             WHERE ca.vocabulary_item_id = $1::uuid
             GROUP BY p.book
             ORDER BY count DESC",
            &[PgValue::Text(vocab_id.clone())],
        )
        .await
        .unwrap_or_default();

    let book_counts: Vec<BookCount> = count_rows
        .iter()
        .map(|r| BookCount {
            book: text_col(r, "book"),
            count: int_col(r, "count"),
        })
        .collect();

    Ok(Some(WordStudyResult {
        id: vocab_id,
        lemma: text_col(vocab_row, "lemma"),
        language_code: text_col(vocab_row, "language_code"),
        transliteration: opt_text_col(vocab_row, "transliteration"),
        ipa: opt_text_col(vocab_row, "ipa"),
        short_gloss: opt_text_col(vocab_row, "short_gloss"),
        part_of_speech: opt_text_col(vocab_row, "part_of_speech"),
        frequency_count: opt_int_col(vocab_row, "frequency_count"),
        frequency_rank: opt_int_col(vocab_row, "frequency_rank"),
        audio_url: opt_text_col(vocab_row, "audio_url"),
        notes: opt_text_col(vocab_row, "notes"),
        extended_notes: opt_text_col(vocab_row, "extended_notes"),
        study_entries,
        cross_refs,
        book_counts,
    }))
}
