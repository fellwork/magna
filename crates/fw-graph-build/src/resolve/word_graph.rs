//! Word Graph resolver — assembles complete data for a tapped word.
//!
//! * `wordGraph(vocabId: ID!, book: String!, chapter: Int!, verse: Int!): WordGraph`
//!   — Returns identity, morphology, dictionary, word studies, cross-refs, entities,
//!     clause role, and application data for a single vocabulary item at a specific verse.

use std::sync::Arc;

use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef,
};
use fw_graph_types::PgValue;

use crate::executor::{QueryExecutor, RequestConnection};

// ── Output structs ────────────────────────────────────────────────────────────

/// A word study entry from commentary extraction.
#[derive(Clone)]
pub struct WordStudy {
    pub series: String,
    pub content: String,
}

/// A cross-reference scoped to a verse.
#[derive(Clone)]
pub struct CrossRef {
    pub target_ref: String,
    pub relationship: String,
}

/// An entity appearing in a verse.
#[derive(Clone)]
pub struct VerseEntity {
    pub entity_id: String,
    pub entity_type: String,
}

/// A doctrine tag from pericope excursus data.
#[derive(Clone)]
pub struct DoctrineTag {
    pub topic: String,
    pub category: Option<String>,
    pub subcategory: Option<String>,
    pub key_ref: Option<String>,
}

/// Complete word graph data for a tapped vocabulary item.
#[derive(Clone)]
pub struct WordGraph {
    // From vocabulary_items
    pub lemma: String,
    pub transliteration: Option<String>,
    pub gloss: Option<String>,
    pub part_of_speech: Option<String>,
    pub frequency_count: Option<i64>,
    pub semantic_domain: Option<String>,
    pub audio_url: Option<String>,

    // From passage_tokens (enriched by OpenGNT)
    pub louw_nida_domain: Option<String>,
    pub bdag_catchword: Option<String>,
    pub morphology_human: Option<String>,
    pub is_ot_quotation: bool,
    pub is_reported_speech: bool,

    // From theological_dictionary_entries (NIDNTT)
    pub nidntt_gloss: Option<String>,
    pub nidntt_cl_ot: Option<String>,
    pub nidntt_nt: Option<String>,

    // From word_study_entries (commentary)
    pub word_studies: Vec<WordStudy>,

    // From commentary_cross_references (scoped to verse)
    pub cross_refs: Vec<CrossRef>,

    // From biblical_verse_entities (scoped to verse)
    pub entities: Vec<VerseEntity>,

    // From phrase_structure_nodes (clause role for this token)
    pub clause_role: Option<String>,

    // From commentary_applications (scoped to pericope)
    pub application: Option<String>,

    // From pericope_excursus (doctrine tags for book)
    pub doctrine_tags: Vec<DoctrineTag>,
}

// ── Type registration ─────────────────────────────────────────────────────────

/// Register `WordGraph`, `WordStudy`, `CrossRef`, `VerseEntity` object types.
pub fn register_word_graph_types(
    builder: async_graphql::dynamic::SchemaBuilder,
) -> async_graphql::dynamic::SchemaBuilder {
    let word_study = Object::new("WordStudy")
        .field(Field::new("series", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordStudy>()?;
                Ok(Some(FieldValue::value(w.series.clone())))
            })
        }))
        .field(Field::new("content", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordStudy>()?;
                Ok(Some(FieldValue::value(w.content.clone())))
            })
        }));

    let cross_ref = Object::new("CrossRef")
        .field(Field::new("targetRef", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<CrossRef>()?;
                Ok(Some(FieldValue::value(c.target_ref.clone())))
            })
        }))
        .field(Field::new("relationship", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<CrossRef>()?;
                Ok(Some(FieldValue::value(c.relationship.clone())))
            })
        }));

    let verse_entity = Object::new("VerseEntity")
        .field(Field::new("entityId", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<VerseEntity>()?;
                Ok(Some(FieldValue::value(e.entity_id.clone())))
            })
        }))
        .field(Field::new("entityType", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let e = ctx.parent_value.try_downcast_ref::<VerseEntity>()?;
                Ok(Some(FieldValue::value(e.entity_type.clone())))
            })
        }));

    let doctrine_tag = Object::new("DoctrineTag")
        .field(Field::new("topic", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DoctrineTag>()?;
                Ok(Some(FieldValue::value(d.topic.clone())))
            })
        }))
        .field(Field::new("category", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DoctrineTag>()?;
                Ok(d.category.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("subcategory", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DoctrineTag>()?;
                Ok(d.subcategory.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("keyRef", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let d = ctx.parent_value.try_downcast_ref::<DoctrineTag>()?;
                Ok(d.key_ref.clone().map(FieldValue::value))
            })
        }));

    let word_graph = Object::new("WordGraph")
        // vocabulary_items fields
        .field(Field::new("lemma", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(Some(FieldValue::value(w.lemma.clone())))
            })
        }))
        .field(Field::new("transliteration", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.transliteration.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("gloss", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.gloss.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("partOfSpeech", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.part_of_speech.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("frequencyCount", TypeRef::named(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.frequency_count.map(FieldValue::value))
            })
        }))
        .field(Field::new("semanticDomain", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.semantic_domain.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("audioUrl", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.audio_url.clone().map(FieldValue::value))
            })
        }))
        // passage_tokens enrichment
        .field(Field::new("louwNidaDomain", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.louw_nida_domain.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("bdagCatchword", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.bdag_catchword.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("morphologyHuman", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.morphology_human.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("isOtQuotation", TypeRef::named_nn(TypeRef::BOOLEAN), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(Some(FieldValue::value(w.is_ot_quotation)))
            })
        }))
        .field(Field::new("isReportedSpeech", TypeRef::named_nn(TypeRef::BOOLEAN), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(Some(FieldValue::value(w.is_reported_speech)))
            })
        }))
        // NIDNTT
        .field(Field::new("nidnttGloss", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.nidntt_gloss.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("nidnttClOt", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.nidntt_cl_ot.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("nidnttNt", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.nidntt_nt.clone().map(FieldValue::value))
            })
        }))
        // Nested types
        .field(Field::new("wordStudies", TypeRef::named_nn_list_nn("WordStudy"), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                let values: Vec<FieldValue> = w.word_studies.iter()
                    .map(|s| FieldValue::owned_any(s.clone()))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("crossRefs", TypeRef::named_nn_list_nn("CrossRef"), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                let values: Vec<FieldValue> = w.cross_refs.iter()
                    .map(|c| FieldValue::owned_any(c.clone()))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("entities", TypeRef::named_nn_list_nn("VerseEntity"), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                let values: Vec<FieldValue> = w.entities.iter()
                    .map(|e| FieldValue::owned_any(e.clone()))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        // Clause role
        .field(Field::new("clauseRole", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.clause_role.clone().map(FieldValue::value))
            })
        }))
        // Application
        .field(Field::new("application", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                Ok(w.application.clone().map(FieldValue::value))
            })
        }))
        // Doctrine tags
        .field(Field::new("doctrineTags", TypeRef::named_nn_list_nn("DoctrineTag"), |ctx| {
            FieldFuture::new(async move {
                let w = ctx.parent_value.try_downcast_ref::<WordGraph>()?;
                let values: Vec<FieldValue> = w.doctrine_tags.iter()
                    .map(|d| FieldValue::owned_any(d.clone()))
                    .collect();
                Ok(Some(FieldValue::list(values)))
            })
        }));

    builder
        .register(word_study)
        .register(cross_ref)
        .register(verse_entity)
        .register(doctrine_tag)
        .register(word_graph)
}

// ── wordGraph resolver field ────────────────────────────────────────────────

/// Build `wordGraph(vocabId: ID!, book: String!, chapter: Int!, verse: Int!): WordGraph`
///
/// Assembles the complete data for a tapped word, keyed by vocabulary_item_id.
pub fn word_graph_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "wordGraph",
        TypeRef::named("WordGraph"),
        |ctx| {
            FieldFuture::new(async move {
                let conn = ctx
                    .data_opt::<RequestConnection>()
                    .ok_or_else(|| async_graphql::Error::new("No database connection"))?;

                let vocab_id = ctx.args.try_get("vocabId")?.string()
                    .map_err(|_| async_graphql::Error::new("vocabId must be a string"))?
                    .to_owned();
                let book = ctx.args.try_get("book")?.string()
                    .map_err(|_| async_graphql::Error::new("book must be a string"))?
                    .to_owned();
                let chapter = ctx.args.try_get("chapter")?.i64()
                    .map_err(|_| async_graphql::Error::new("chapter must be an int"))?;
                let verse = ctx.args.try_get("verse")?.i64()
                    .map_err(|_| async_graphql::Error::new("verse must be an int"))?;

                let graph = fetch_word_graph(conn, &vocab_id, &book, chapter, verse).await?;

                match graph {
                    Some(g) => Ok(Some(FieldValue::owned_any(g))),
                    None => Ok(None),
                }
            })
        },
    )
    .argument(InputValue::new("vocabId", TypeRef::named_nn(TypeRef::ID)))
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("verse",   TypeRef::named_nn(TypeRef::INT)))
}

// ── SQL fetch ───────────────────────────────────────────────────────────────

/// Assemble complete word graph data from multiple tables.
async fn fetch_word_graph(
    conn: &RequestConnection,
    vocab_id: &str,
    book: &str,
    chapter: i64,
    verse: i64,
) -> Result<Option<WordGraph>, async_graphql::Error> {
    // 1. Vocabulary item identity
    let vocab_sql = r#"
SELECT lemma, transliteration, short_gloss, part_of_speech,
       frequency_count, semantic_domain, audio_url
FROM vocabulary_items
WHERE id = $1::uuid
"#;
    let vocab_rows = conn
        .execute(vocab_sql, &[PgValue::Text(vocab_id.to_owned())])
        .await
        .map_err(|e| async_graphql::Error::new(format!("vocabulary_items query failed: {e}")))?;

    let vocab_row = match vocab_rows.first() {
        Some(row) => row,
        None => return Ok(None), // No such vocabulary item
    };

    let lemma = text_col(vocab_row, "lemma");
    let transliteration = opt_text_col(vocab_row, "transliteration");
    let gloss = opt_text_col(vocab_row, "short_gloss");
    let part_of_speech = opt_text_col(vocab_row, "part_of_speech");
    let frequency_count = opt_int_col(vocab_row, "frequency_count");
    let semantic_domain = opt_text_col(vocab_row, "semantic_domain");
    let audio_url = opt_text_col(vocab_row, "audio_url");

    // 2. Token-level features from passage_tokens
    let token_sql = r#"
SELECT pt.louw_nida_domain, pt.bdag_catchword, pt.morphology_human,
       pt.ot_quotation, pt.reported_speech
FROM passage_tokens pt
JOIN passages p ON pt.passage_id = p.id
WHERE pt.vocabulary_item_id = $1::uuid
  AND p.book = $2 AND p.chapter = $3 AND p.verse = $4
LIMIT 1
"#;
    let token_rows = conn
        .execute(token_sql, &[
            PgValue::Text(vocab_id.to_owned()),
            PgValue::Text(book.to_owned()),
            PgValue::Int(chapter),
            PgValue::Int(verse),
        ])
        .await
        .map_err(|e| async_graphql::Error::new(format!("passage_tokens query failed: {e}")))?;

    let (louw_nida_domain, bdag_catchword, morphology_human, is_ot_quotation, is_reported_speech) =
        if let Some(row) = token_rows.first() {
            (
                opt_text_col(row, "louw_nida_domain"),
                opt_text_col(row, "bdag_catchword"),
                opt_text_col(row, "morphology_human"),
                opt_text_col(row, "ot_quotation").is_some(),
                opt_text_col(row, "reported_speech").is_some(),
            )
        } else {
            (None, None, None, false, false)
        };

    // 3. NIDNTT theological dictionary
    let nidntt_sql = r#"
SELECT gloss, section_cl_ot, section_nt
FROM theological_dictionary_entries
WHERE vocabulary_item_id = $1::uuid
LIMIT 1
"#;
    let nidntt_rows = conn
        .execute(nidntt_sql, &[PgValue::Text(vocab_id.to_owned())])
        .await
        .map_err(|e| async_graphql::Error::new(format!("theological_dictionary query failed: {e}")))?;

    let (nidntt_gloss, nidntt_cl_ot, nidntt_nt) = if let Some(row) = nidntt_rows.first() {
        (
            opt_text_col(row, "gloss"),
            opt_text_col(row, "section_cl_ot"),
            opt_text_col(row, "section_nt"),
        )
    } else {
        (None, None, None)
    };

    // 4. Word studies — look up by vocabulary_item_id first, fall back to lemma+book
    let ws_sql = r#"
SELECT cs.series_name AS series, ws.editor_summary AS content
FROM word_study_entries ws
JOIN commentary_sources cs ON ws.source_id = cs.id
WHERE (ws.vocabulary_item_id = $1::uuid OR (ws.greek_term = $2 AND ws.book = $3))
  AND ws.editor_summary IS NOT NULL AND ws.editor_summary != ''
ORDER BY cs.series_name
LIMIT 10
"#;
    let ws_rows = conn
        .execute(ws_sql, &[
            PgValue::Text(vocab_id.to_owned()),
            PgValue::Text(lemma.clone()),
            PgValue::Text(book.to_owned()),
        ])
        .await
        .map_err(|e| async_graphql::Error::new(format!("word_study_entries query failed: {e}")))?;

    let word_studies: Vec<WordStudy> = ws_rows
        .into_iter()
        .map(|row| WordStudy {
            series: text_col(&row, "series"),
            content: text_col(&row, "content"),
        })
        .collect();

    // 5. Cross-references scoped to verse
    let verse_ref = format!("{}.{}.{}", book, chapter, verse);
    let xref_sql = r#"
SELECT target_ref, relationship
FROM commentary_cross_references
WHERE book = $1 AND verse_ref = $2
ORDER BY target_ref
LIMIT 20
"#;
    let xref_rows = conn
        .execute(xref_sql, &[
            PgValue::Text(book.to_owned()),
            PgValue::Text(verse_ref.clone()),
        ])
        .await
        .map_err(|e| async_graphql::Error::new(format!("cross_references query failed: {e}")))?;

    let cross_refs: Vec<CrossRef> = xref_rows
        .into_iter()
        .map(|row| CrossRef {
            target_ref: text_col(&row, "target_ref"),
            relationship: text_col(&row, "relationship"),
        })
        .collect();

    // 6. Entities at this verse
    let osis_ref = format!("{}.{}.{}", book, chapter, verse);
    let entity_sql = r#"
SELECT entity_id, entity_type
FROM biblical_verse_entities
WHERE osis_ref = $1
LIMIT 10
"#;
    let entity_rows = conn
        .execute(entity_sql, &[PgValue::Text(osis_ref)])
        .await
        .map_err(|e| async_graphql::Error::new(format!("biblical_verse_entities query failed: {e}")))?;

    let entities: Vec<VerseEntity> = entity_rows
        .into_iter()
        .map(|row| VerseEntity {
            entity_id: text_col(&row, "entity_id"),
            entity_type: text_col(&row, "entity_type"),
        })
        .collect();

    // 7. Clause role from phrase_structure_nodes
    let psn_sql = r#"
SELECT role_label
FROM phrase_structure_nodes
WHERE book = $1 AND chapter = $2 AND verse = $3
  AND role_label IS NOT NULL AND role_label != ''
ORDER BY depth DESC
LIMIT 1
"#;
    let psn_rows = conn
        .execute(psn_sql, &[
            PgValue::Text(book.to_owned()),
            PgValue::Int(chapter),
            PgValue::Int(verse),
        ])
        .await
        .map_err(|e| async_graphql::Error::new(format!("phrase_structure_nodes query failed: {e}")))?;

    let clause_role = psn_rows.first().and_then(|row| opt_text_col(row, "role_label"));

    // 8. Application — find pericope covering this verse, get application
    let app_sql = r#"
SELECT ca.content
FROM commentary_applications ca
WHERE ca.book = $1
  AND ca.verse_ref LIKE $2
ORDER BY ca.verse_ref
LIMIT 1
"#;
    let verse_prefix = format!("{}.{}", chapter, verse);
    let app_rows = conn
        .execute(app_sql, &[
            PgValue::Text(book.to_owned()),
            PgValue::Text(format!("%{}%", verse_prefix)),
        ])
        .await
        .map_err(|e| async_graphql::Error::new(format!("commentary_applications query failed: {e}")))?;

    let application = app_rows.first().and_then(|row| opt_text_col(row, "content"));

    // 9. Doctrine tags from pericope_excursus
    let doctrine_sql = r#"
SELECT topic, systematic_doctrine_category, systematic_doctrine_subcategory, key_ref
FROM pericope_excursus
WHERE book = $1
LIMIT 20
"#;
    let doctrine_rows = conn
        .execute(doctrine_sql, &[PgValue::Text(book.to_owned())])
        .await
        .map_err(|e| async_graphql::Error::new(format!("pericope_excursus query failed: {e}")))?;

    let doctrine_tags: Vec<DoctrineTag> = doctrine_rows
        .into_iter()
        .map(|row| DoctrineTag {
            topic: text_col(&row, "topic"),
            category: opt_text_col(&row, "systematic_doctrine_category"),
            subcategory: opt_text_col(&row, "systematic_doctrine_subcategory"),
            key_ref: opt_text_col(&row, "key_ref"),
        })
        .collect();

    Ok(Some(WordGraph {
        lemma,
        transliteration,
        gloss,
        part_of_speech,
        frequency_count,
        semantic_domain,
        audio_url,
        louw_nida_domain,
        bdag_catchword,
        morphology_human,
        is_ot_quotation,
        is_reported_speech,
        nidntt_gloss,
        nidntt_cl_ot,
        nidntt_nt,
        word_studies,
        cross_refs,
        entities,
        clause_role,
        application,
        doctrine_tags,
    }))
}

// ── Row accessors ────────────────────────────────────────────────────────────

fn text_col(row: &fw_graph_types::PgRow, col: &str) -> String {
    match row.get(col) {
        Some(PgValue::Text(s)) => s.clone(),
        Some(PgValue::Uuid(u)) => u.to_string(),
        _ => String::new(),
    }
}

fn opt_text_col(row: &fw_graph_types::PgRow, col: &str) -> Option<String> {
    match row.get(col) {
        Some(PgValue::Text(s)) if !s.is_empty() => Some(s.clone()),
        Some(PgValue::Null) | None => None,
        _ => None,
    }
}

fn opt_int_col(row: &fw_graph_types::PgRow, col: &str) -> Option<i64> {
    match row.get(col) {
        Some(PgValue::Int(n)) => Some(*n),
        Some(PgValue::Null) | None => None,
        _ => None,
    }
}
