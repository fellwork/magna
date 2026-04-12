//! Word Study resolver — standalone lemma-scoped word study data.
//!
//! * `wordStudy(lemma: String!, languageCode: String!): WordStudyResult`
//!   — Returns comprehensive word study data for a lemma: identity, semantic range,
//!     concept relationships, concordance with English spans, depth insights,
//!     linguistic roots/senses/definitions, commentary studies, cross-references,
//!     scholarly positions, discourse notes, and theological applications.
//!
//! This is the data layer for the `/word/:lang/:lemma` spoke page.
//! Queries ~12 tables to assemble the complete word study.

use std::sync::Arc;

use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputValue, Object, TypeRef,
};
use fw_graph_types::PgValue;

use crate::executor::{QueryExecutor, RequestConnection};

// ══════════════════════════════════════════════════════════════════════════════
// Output structs
// ══════════════════════════════════════════════════════════════════════════════

/// Concept data — semantic range and theological note (from `concepts` table).
#[derive(Clone)]
struct ConceptData {
    pub concept_id: String,
    pub semantic_range: Vec<String>, // JSON array → Vec
    pub theological_note: Option<String>,
    pub occurrence_count: Option<i64>,
}

/// Concept relationship — synonym, antonym, bridge (from `concept_relationships`).
#[derive(Clone)]
struct ConceptRelation {
    pub target_lemma: String,
    pub target_transliteration: Option<String>,
    pub relationship_type: String, // synonym / antonym / semantic_field / theological_arc / cross_testament_bridge
    pub strength: Option<f64>,
    pub note: Option<String>,
}

/// A concordance occurrence with the English span (from `concept_alignments`).
#[derive(Clone)]
struct ConcordanceEntry {
    pub passage_ref: String,
    pub english_span: String,
    pub role: Option<String>,
    pub alignment_note: Option<String>,
}

/// Book-level concordance count.
#[derive(Clone)]
struct BookCount {
    pub book: String,
    pub count: i64,
}

/// Depth insight linked to this concept.
#[derive(Clone)]
struct DepthInsight {
    pub passage_ref: String,
    pub insight_type: String, // story_arc / redemption_echo / narrative_connection
    pub title: String,
    pub body: String,
    pub confidence: Option<f64>,
    pub linked_passages: Vec<String>, // from depth_insight_links
}

/// Linguistic root family (from `linguistic_roots`).
#[derive(Clone)]
struct LinguisticRoot {
    pub root_letters: String,
    pub transliteration: Option<String>,
    pub meaning_summary: Option<String>,
    pub cognates: Vec<String>, // JSON array → Vec
}

/// Sense with definitions from multiple lexicons (from `linguistic_senses` + `sense_definitions`).
#[derive(Clone)]
struct LexiconSense {
    pub binyan: Option<String>,
    pub sense_index: i64,
    pub definitions: Vec<SenseDefinition>,
}

/// A single definition from a specific source (BDB, HALOT, Thayer).
#[derive(Clone)]
struct SenseDefinition {
    pub source: String, // BDB / HALOT / Thayer
    pub definition: String,
    pub citations: Option<String>,
    pub bdb_page: Option<String>,
    pub notes: Option<String>,
}

/// Token-level enrichment aggregated across occurrences (from `passage_tokens`).
#[derive(Clone)]
struct TokenEnrichment {
    pub louw_nida_domain: Option<String>,
    pub bdag_catchword: Option<String>,
    pub morphology_forms: Vec<String>, // distinct morphology_human values
}

/// A word study entry from commentary extraction.
#[derive(Clone)]
struct StudyEntry {
    pub series: String,
    pub book: String,
    pub issue_type: String,
    pub issue_description: String,
    pub content: String,
    pub fallacy_warning: Option<String>,
    pub lexical_domain: Option<String>,
}

/// A cross-reference linked to this word's occurrences.
#[derive(Clone)]
struct StudyCrossRef {
    pub book: String,
    pub verse_ref: String,
    pub target_ref: String,
    pub relationship: String,
    pub note: Option<String>,
}

/// Scholarly exegetical position on an interpretive question.
#[derive(Clone)]
struct ScholarlyPosition {
    pub book: String,
    pub passage_ref: String,
    pub issue_type: String,
    pub issue_description: String,
    pub harris_conclusion: Option<String>,
}

/// Discourse/grammar observation from commentary.
#[derive(Clone)]
struct DiscourseNote {
    pub book: String,
    pub verse_ref: String,
    pub note_type: String,
    pub description: String,
}

/// Theological application from commentary.
#[derive(Clone)]
struct Application {
    pub book: String,
    pub verse_ref: String,
    pub theme: Option<String>,
    pub content: String,
}

/// Discovery heat — community engagement signal.
#[derive(Clone)]
struct HeatInfo {
    pub heat: f64,
    pub event_count: i64,
}

/// Complete word study result.
#[derive(Clone)]
struct WordStudyResult {
    // Identity (vocabulary_items)
    pub id: String,
    pub lemma: String,
    pub language_code: String,
    pub transliteration: Option<String>,
    pub ipa: Option<String>,
    pub short_gloss: Option<String>,
    pub part_of_speech: Option<String>,
    pub strong_number: Option<String>,
    pub frequency_count: Option<i64>,
    pub frequency_rank: Option<i64>,
    pub audio_url: Option<String>,
    pub notes: Option<String>,
    pub extended_notes: Option<String>,

    // Concept layer
    pub concept: Option<ConceptData>,
    pub relationships: Vec<ConceptRelation>,

    // Concordance
    pub concordance: Vec<ConcordanceEntry>,
    pub book_counts: Vec<BookCount>,

    // Depth insights
    pub depth_insights: Vec<DepthInsight>,

    // Linguistic graph
    pub root: Option<LinguisticRoot>,
    pub senses: Vec<LexiconSense>,

    // Token enrichment
    pub token_enrichment: Option<TokenEnrichment>,

    // Commentary
    pub study_entries: Vec<StudyEntry>,
    pub cross_refs: Vec<StudyCrossRef>,
    pub scholarly_positions: Vec<ScholarlyPosition>,
    pub discourse_notes: Vec<DiscourseNote>,
    pub applications: Vec<Application>,

    // Social
    pub heat: Option<HeatInfo>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════════════════════

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

fn opt_float_col(row: &fw_graph_types::PgRow, col: &str) -> Option<f64> {
    match row.get(col) {
        Some(PgValue::Float(f)) => Some(*f),
        _ => None,
    }
}

/// Parse a JSON text array like '["a","b","c"]' into Vec<String>.
fn json_text_array(row: &fw_graph_types::PgRow, col: &str) -> Vec<String> {
    match row.get(col) {
        Some(PgValue::Text(s)) => {
            // Simple JSON array parse
            serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
        }
        Some(PgValue::Json(v)) => {
            if let Some(arr) = v.as_array() {
                arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Macro for simple field registration (reduces boilerplate)
// ══════════════════════════════════════════════════════════════════════════════

macro_rules! string_field {
    ($obj:expr, $name:literal, $type:ty, $field:ident) => {
        $obj = $obj.field(Field::new($name, TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<$type>()?;
                Ok(Some(FieldValue::value(v.$field.clone())))
            })
        }));
    };
}

macro_rules! opt_string_field {
    ($obj:expr, $name:literal, $type:ty, $field:ident) => {
        $obj = $obj.field(Field::new($name, TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<$type>()?;
                Ok(v.$field.clone().map(FieldValue::value))
            })
        }));
    };
}

macro_rules! opt_int_field {
    ($obj:expr, $name:literal, $type:ty, $field:ident) => {
        $obj = $obj.field(Field::new($name, TypeRef::named(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<$type>()?;
                Ok(v.$field.map(FieldValue::value))
            })
        }));
    };
}

macro_rules! opt_float_field {
    ($obj:expr, $name:literal, $type:ty, $field:ident) => {
        $obj = $obj.field(Field::new($name, TypeRef::named(TypeRef::FLOAT), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<$type>()?;
                Ok(v.$field.map(FieldValue::value))
            })
        }));
    };
}

// ══════════════════════════════════════════════════════════════════════════════
// Type registration
// ══════════════════════════════════════════════════════════════════════════════

pub fn register_word_study_types(
    builder: async_graphql::dynamic::SchemaBuilder,
) -> async_graphql::dynamic::SchemaBuilder {
    // ── ConceptData ──
    let mut concept_data = Object::new("ConceptData");
    string_field!(concept_data, "conceptId", ConceptData, concept_id);
    opt_string_field!(concept_data, "theologicalNote", ConceptData, theological_note);
    opt_int_field!(concept_data, "occurrenceCount", ConceptData, occurrence_count);
    concept_data = concept_data.field(Field::new("semanticRange", TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
        FieldFuture::new(async move {
            let c = ctx.parent_value.try_downcast_ref::<ConceptData>()?;
            Ok(Some(FieldValue::list(c.semantic_range.iter().map(|s| FieldValue::value(s.clone())).collect::<Vec<_>>())))
        })
    }));

    // ── ConceptRelation ──
    let mut concept_rel = Object::new("ConceptRelation");
    string_field!(concept_rel, "targetLemma", ConceptRelation, target_lemma);
    opt_string_field!(concept_rel, "targetTransliteration", ConceptRelation, target_transliteration);
    string_field!(concept_rel, "relationshipType", ConceptRelation, relationship_type);
    opt_float_field!(concept_rel, "strength", ConceptRelation, strength);
    opt_string_field!(concept_rel, "note", ConceptRelation, note);

    // ── ConcordanceEntry ──
    let mut conc_entry = Object::new("ConcordanceEntry");
    string_field!(conc_entry, "passageRef", ConcordanceEntry, passage_ref);
    string_field!(conc_entry, "englishSpan", ConcordanceEntry, english_span);
    opt_string_field!(conc_entry, "role", ConcordanceEntry, role);
    opt_string_field!(conc_entry, "alignmentNote", ConcordanceEntry, alignment_note);

    // ── BookCount ──
    let mut book_count = Object::new("BookCount");
    string_field!(book_count, "book", BookCount, book);
    book_count = book_count.field(Field::new("count", TypeRef::named_nn(TypeRef::INT), |ctx| {
        FieldFuture::new(async move {
            let b = ctx.parent_value.try_downcast_ref::<BookCount>()?;
            Ok(Some(FieldValue::value(b.count)))
        })
    }));

    // ── DepthInsight ──
    let mut depth_insight = Object::new("WordStudyDepthInsight");
    string_field!(depth_insight, "passageRef", DepthInsight, passage_ref);
    string_field!(depth_insight, "insightType", DepthInsight, insight_type);
    string_field!(depth_insight, "title", DepthInsight, title);
    string_field!(depth_insight, "body", DepthInsight, body);
    opt_float_field!(depth_insight, "confidence", DepthInsight, confidence);
    depth_insight = depth_insight.field(Field::new("linkedPassages", TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
        FieldFuture::new(async move {
            let d = ctx.parent_value.try_downcast_ref::<DepthInsight>()?;
            Ok(Some(FieldValue::list(d.linked_passages.iter().map(|s| FieldValue::value(s.clone())).collect::<Vec<_>>())))
        })
    }));

    // ── LinguisticRoot ──
    let mut ling_root = Object::new("LinguisticRoot");
    string_field!(ling_root, "rootLetters", LinguisticRoot, root_letters);
    opt_string_field!(ling_root, "transliteration", LinguisticRoot, transliteration);
    opt_string_field!(ling_root, "meaningSummary", LinguisticRoot, meaning_summary);
    ling_root = ling_root.field(Field::new("cognates", TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
        FieldFuture::new(async move {
            let r = ctx.parent_value.try_downcast_ref::<LinguisticRoot>()?;
            Ok(Some(FieldValue::list(r.cognates.iter().map(|s| FieldValue::value(s.clone())).collect::<Vec<_>>())))
        })
    }));

    // ── SenseDefinition ──
    let mut sense_def = Object::new("SenseDefinition");
    string_field!(sense_def, "source", SenseDefinition, source);
    string_field!(sense_def, "definition", SenseDefinition, definition);
    opt_string_field!(sense_def, "citations", SenseDefinition, citations);
    opt_string_field!(sense_def, "bdbPage", SenseDefinition, bdb_page);
    opt_string_field!(sense_def, "notes", SenseDefinition, notes);

    // ── LexiconSense ──
    let mut lex_sense = Object::new("LexiconSense");
    opt_string_field!(lex_sense, "binyan", LexiconSense, binyan);
    lex_sense = lex_sense.field(Field::new("senseIndex", TypeRef::named_nn(TypeRef::INT), |ctx| {
        FieldFuture::new(async move {
            let s = ctx.parent_value.try_downcast_ref::<LexiconSense>()?;
            Ok(Some(FieldValue::value(s.sense_index)))
        })
    }));
    lex_sense = lex_sense.field(Field::new("definitions", TypeRef::named_nn_list_nn("SenseDefinition"), |ctx| {
        FieldFuture::new(async move {
            let s = ctx.parent_value.try_downcast_ref::<LexiconSense>()?;
            Ok(Some(FieldValue::list(s.definitions.iter().map(|d| FieldValue::owned_any(d.clone())).collect::<Vec<_>>())))
        })
    }));

    // ── TokenEnrichment ──
    let mut token_enrich = Object::new("TokenEnrichment");
    opt_string_field!(token_enrich, "louwNidaDomain", TokenEnrichment, louw_nida_domain);
    opt_string_field!(token_enrich, "bdagCatchword", TokenEnrichment, bdag_catchword);
    token_enrich = token_enrich.field(Field::new("morphologyForms", TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
        FieldFuture::new(async move {
            let t = ctx.parent_value.try_downcast_ref::<TokenEnrichment>()?;
            Ok(Some(FieldValue::list(t.morphology_forms.iter().map(|s| FieldValue::value(s.clone())).collect::<Vec<_>>())))
        })
    }));

    // ── StudyEntry ──
    let mut study_entry = Object::new("StudyEntry");
    string_field!(study_entry, "series", StudyEntry, series);
    string_field!(study_entry, "book", StudyEntry, book);
    string_field!(study_entry, "issueType", StudyEntry, issue_type);
    string_field!(study_entry, "issueDescription", StudyEntry, issue_description);
    string_field!(study_entry, "content", StudyEntry, content);
    opt_string_field!(study_entry, "fallacyWarning", StudyEntry, fallacy_warning);
    opt_string_field!(study_entry, "lexicalDomain", StudyEntry, lexical_domain);

    // ── StudyCrossRef ──
    let mut study_xref = Object::new("StudyCrossRef");
    string_field!(study_xref, "book", StudyCrossRef, book);
    string_field!(study_xref, "verseRef", StudyCrossRef, verse_ref);
    string_field!(study_xref, "targetRef", StudyCrossRef, target_ref);
    string_field!(study_xref, "relationship", StudyCrossRef, relationship);
    opt_string_field!(study_xref, "note", StudyCrossRef, note);

    // ── ScholarlyPosition ──
    let mut scholarly = Object::new("ScholarlyPosition");
    string_field!(scholarly, "book", ScholarlyPosition, book);
    string_field!(scholarly, "passageRef", ScholarlyPosition, passage_ref);
    string_field!(scholarly, "issueType", ScholarlyPosition, issue_type);
    string_field!(scholarly, "issueDescription", ScholarlyPosition, issue_description);
    opt_string_field!(scholarly, "harrisConclusion", ScholarlyPosition, harris_conclusion);

    // ── DiscourseNote ──
    let mut discourse = Object::new("DiscourseNote");
    string_field!(discourse, "book", DiscourseNote, book);
    string_field!(discourse, "verseRef", DiscourseNote, verse_ref);
    string_field!(discourse, "noteType", DiscourseNote, note_type);
    string_field!(discourse, "description", DiscourseNote, description);

    // ── Application ──
    let mut application = Object::new("WordStudyApplication");
    string_field!(application, "book", Application, book);
    string_field!(application, "verseRef", Application, verse_ref);
    opt_string_field!(application, "theme", Application, theme);
    string_field!(application, "content", Application, content);

    // ── HeatInfo ──
    let heat_info = Object::new("HeatInfo")
        .field(Field::new("heat", TypeRef::named_nn(TypeRef::FLOAT), |ctx| {
            FieldFuture::new(async move {
                let h = ctx.parent_value.try_downcast_ref::<HeatInfo>()?;
                Ok(Some(FieldValue::value(h.heat)))
            })
        }))
        .field(Field::new("eventCount", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let h = ctx.parent_value.try_downcast_ref::<HeatInfo>()?;
                Ok(Some(FieldValue::value(h.event_count)))
            })
        }));

    // ── WordStudyResult (root type) ──
    let mut result_type = Object::new("WordStudyResult");

    // Identity fields
    result_type = result_type.field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
        FieldFuture::new(async move {
            let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
            Ok(Some(FieldValue::value(r.id.clone())))
        })
    }));
    string_field!(result_type, "lemma", WordStudyResult, lemma);
    string_field!(result_type, "languageCode", WordStudyResult, language_code);
    opt_string_field!(result_type, "transliteration", WordStudyResult, transliteration);
    opt_string_field!(result_type, "ipa", WordStudyResult, ipa);
    opt_string_field!(result_type, "shortGloss", WordStudyResult, short_gloss);
    opt_string_field!(result_type, "partOfSpeech", WordStudyResult, part_of_speech);
    opt_string_field!(result_type, "strongNumber", WordStudyResult, strong_number);
    opt_int_field!(result_type, "frequencyCount", WordStudyResult, frequency_count);
    opt_int_field!(result_type, "frequencyRank", WordStudyResult, frequency_rank);
    opt_string_field!(result_type, "audioUrl", WordStudyResult, audio_url);
    opt_string_field!(result_type, "notes", WordStudyResult, notes);
    opt_string_field!(result_type, "extendedNotes", WordStudyResult, extended_notes);

    // Concept
    result_type = result_type.field(Field::new("concept", TypeRef::named("ConceptData"), |ctx| {
        FieldFuture::new(async move {
            let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
            Ok(r.concept.clone().map(FieldValue::owned_any))
        })
    }));

    // Lists — macro can't handle these easily, so inline
    macro_rules! list_field {
        ($obj:expr, $name:literal, $gql_type:literal, $type:ty, $field:ident) => {
            $obj = $obj.field(Field::new($name, TypeRef::named_nn_list_nn($gql_type), |ctx| {
                FieldFuture::new(async move {
                    let r = ctx.parent_value.try_downcast_ref::<$type>()?;
                    Ok(Some(FieldValue::list(r.$field.iter().map(|x| FieldValue::owned_any(x.clone())).collect::<Vec<_>>())))
                })
            }));
        };
    }

    list_field!(result_type, "relationships", "ConceptRelation", WordStudyResult, relationships);
    list_field!(result_type, "concordance", "ConcordanceEntry", WordStudyResult, concordance);
    list_field!(result_type, "bookCounts", "BookCount", WordStudyResult, book_counts);
    list_field!(result_type, "depthInsights", "WordStudyDepthInsight", WordStudyResult, depth_insights);
    list_field!(result_type, "senses", "LexiconSense", WordStudyResult, senses);
    list_field!(result_type, "studyEntries", "StudyEntry", WordStudyResult, study_entries);
    list_field!(result_type, "crossRefs", "StudyCrossRef", WordStudyResult, cross_refs);
    list_field!(result_type, "scholarlyPositions", "ScholarlyPosition", WordStudyResult, scholarly_positions);
    list_field!(result_type, "discourseNotes", "DiscourseNote", WordStudyResult, discourse_notes);
    list_field!(result_type, "applications", "WordStudyApplication", WordStudyResult, applications);

    // Optional nested objects
    result_type = result_type.field(Field::new("root", TypeRef::named("LinguisticRoot"), |ctx| {
        FieldFuture::new(async move {
            let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
            Ok(r.root.clone().map(FieldValue::owned_any))
        })
    }));
    result_type = result_type.field(Field::new("tokenEnrichment", TypeRef::named("TokenEnrichment"), |ctx| {
        FieldFuture::new(async move {
            let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
            Ok(r.token_enrichment.clone().map(FieldValue::owned_any))
        })
    }));
    result_type = result_type.field(Field::new("heat", TypeRef::named("HeatInfo"), |ctx| {
        FieldFuture::new(async move {
            let r = ctx.parent_value.try_downcast_ref::<WordStudyResult>()?;
            Ok(r.heat.clone().map(FieldValue::owned_any))
        })
    }));

    builder
        .register(concept_data)
        .register(concept_rel)
        .register(conc_entry)
        .register(book_count)
        .register(depth_insight)
        .register(ling_root)
        .register(sense_def)
        .register(lex_sense)
        .register(token_enrich)
        .register(study_entry)
        .register(study_xref)
        .register(scholarly)
        .register(discourse)
        .register(application)
        .register(heat_info)
        .register(result_type)
}

// ══════════════════════════════════════════════════════════════════════════════
// Field factory
// ══════════════════════════════════════════════════════════════════════════════

pub fn word_study_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "wordStudy",
        TypeRef::named("WordStudyResult"),
        |ctx| {
            FieldFuture::new(async move {
                let conn = ctx
                    .data_opt::<RequestConnection>()
                    .ok_or_else(|| async_graphql::Error::new("No database connection"))?;

                let lemma = ctx.args.try_get("lemma")?.string()
                    .map_err(|_| async_graphql::Error::new("lemma must be a string"))?
                    .to_owned();
                let language_code = ctx.args.try_get("languageCode")?.string()
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

// ══════════════════════════════════════════════════════════════════════════════
// Data fetch — queries ~12 tables
// ══════════════════════════════════════════════════════════════════════════════

async fn fetch_word_study(
    conn: &RequestConnection,
    lemma: &str,
    language_code: &str,
) -> Result<Option<WordStudyResult>, async_graphql::Error> {
    // ── 1. Vocabulary item (identity) ────────────────────────────────────────
    let vocab_rows = conn
        .execute(
            "SELECT id, lemma, language_code, transliteration, ipa, short_gloss,
                    part_of_speech, strong_number, frequency_count, frequency_rank,
                    audio_url, notes, extended_notes, root_id
             FROM vocabulary_items
             WHERE lemma = $1 AND language_code = $2
             LIMIT 1",
            &[PgValue::Text(lemma.to_owned()), PgValue::Text(language_code.to_owned())],
        )
        .await
        .map_err(|e| async_graphql::Error::new(format!("vocabulary_items: {e}")))?;

    let vocab = match vocab_rows.first() {
        Some(r) => r,
        None => return Ok(None),
    };

    let vocab_id = text_col(vocab, "id");
    let root_id = opt_text_col(vocab, "root_id");

    // ── 2. Concept (semantic range + theological note) ───────────────────────
    let concept = {
        let rows = conn
            .execute(
                "SELECT id, semantic_range, theological_note, occurrence_count
                 FROM concepts WHERE vocabulary_item_id = $1::uuid LIMIT 1",
                &[PgValue::Text(vocab_id.clone())],
            )
            .await
            .unwrap_or_default();
        rows.first().map(|r| ConceptData {
            concept_id: text_col(r, "id"),
            semantic_range: json_text_array(r, "semantic_range"),
            theological_note: opt_text_col(r, "theological_note"),
            occurrence_count: opt_int_col(r, "occurrence_count"),
        })
    };

    let concept_id = concept.as_ref().map(|c| c.concept_id.clone());

    // ── 3. Concept relationships (synonym/antonym/bridge) ────────────────────
    let relationships = if let Some(ref cid) = concept_id {
        let rows = conn
            .execute(
                "SELECT vi.lemma AS target_lemma, vi.transliteration AS target_transliteration,
                        cr.relationship_type, cr.strength, cr.note
                 FROM concept_relationships cr
                 JOIN concepts tc ON tc.id = cr.target_concept_id
                 JOIN vocabulary_items vi ON vi.id = tc.vocabulary_item_id
                 WHERE cr.source_concept_id = $1::uuid
                 ORDER BY cr.strength DESC NULLS LAST
                 LIMIT 20",
                &[PgValue::Text(cid.clone())],
            )
            .await
            .unwrap_or_default();
        rows.iter().map(|r| ConceptRelation {
            target_lemma: text_col(r, "target_lemma"),
            target_transliteration: opt_text_col(r, "target_transliteration"),
            relationship_type: text_col(r, "relationship_type"),
            strength: opt_float_col(r, "strength"),
            note: opt_text_col(r, "note"),
        }).collect()
    } else {
        Vec::new()
    };

    // ── 4. Concordance entries (FIXED: correct column names) ─────────────────
    let concordance = if let Some(ref cid) = concept_id {
        let rows = conn
            .execute(
                "SELECT passage_ref, english_span, role, alignment_note
                 FROM concept_alignments
                 WHERE concept_id = $1::uuid
                 ORDER BY passage_ref
                 LIMIT 200",
                &[PgValue::Text(cid.clone())],
            )
            .await
            .unwrap_or_default();
        rows.iter().map(|r| ConcordanceEntry {
            passage_ref: text_col(r, "passage_ref"),
            english_span: text_col(r, "english_span"),
            role: opt_text_col(r, "role"),
            alignment_note: opt_text_col(r, "alignment_note"),
        }).collect()
    } else {
        Vec::new()
    };

    // ── 5. Book counts (FIXED: go through concepts, parse passage_ref) ───────
    let book_counts = if let Some(ref cid) = concept_id {
        let rows = conn
            .execute(
                "SELECT split_part(passage_ref, '.', 1) AS book, COUNT(*) AS count
                 FROM concept_alignments
                 WHERE concept_id = $1::uuid
                 GROUP BY split_part(passage_ref, '.', 1)
                 ORDER BY count DESC",
                &[PgValue::Text(cid.clone())],
            )
            .await
            .unwrap_or_default();
        rows.iter().map(|r| BookCount {
            book: text_col(r, "book"),
            count: int_col(r, "count"),
        }).collect()
    } else {
        Vec::new()
    };

    // ── 6. Depth insights (via related_concept_ids) ──────────────────────────
    let depth_insights = if let Some(ref cid) = concept_id {
        let rows = conn
            .execute(
                "SELECT di.passage_ref, di.insight_type, di.title, di.body, di.confidence,
                        COALESCE(
                            (SELECT array_agg(dil.linked_passage_ref)
                             FROM depth_insight_links dil WHERE dil.insight_id = di.id),
                            '{}'
                        ) AS linked
                 FROM depth_insights di
                 WHERE $1::uuid = ANY(di.related_concept_ids)
                 ORDER BY di.confidence DESC NULLS LAST
                 LIMIT 20",
                &[PgValue::Text(cid.clone())],
            )
            .await
            .unwrap_or_default();
        rows.iter().map(|r| DepthInsight {
            passage_ref: text_col(r, "passage_ref"),
            insight_type: text_col(r, "insight_type"),
            title: text_col(r, "title"),
            body: text_col(r, "body"),
            confidence: opt_float_col(r, "confidence"),
            linked_passages: json_text_array(r, "linked"),
        }).collect()
    } else {
        Vec::new()
    };

    // ── 7. Linguistic root (if root_id exists) ───────────────────────────────
    let root = if let Some(ref rid) = root_id {
        let rows = conn
            .execute(
                "SELECT root_letters, transliteration, meaning_summary, cognates
                 FROM linguistic_roots WHERE id = $1::uuid LIMIT 1",
                &[PgValue::Text(rid.clone())],
            )
            .await
            .unwrap_or_default();
        rows.first().map(|r| LinguisticRoot {
            root_letters: text_col(r, "root_letters"),
            transliteration: opt_text_col(r, "transliteration"),
            meaning_summary: opt_text_col(r, "meaning_summary"),
            cognates: json_text_array(r, "cognates"),
        })
    } else {
        None
    };

    // ── 8. Lexicon senses + definitions ──────────────────────────────────────
    let senses = {
        let sense_rows = conn
            .execute(
                "SELECT ls.id, ls.binyan, ls.sense_index
                 FROM linguistic_senses ls
                 WHERE ls.vocabulary_item_id = $1::uuid
                 ORDER BY ls.sense_index",
                &[PgValue::Text(vocab_id.clone())],
            )
            .await
            .unwrap_or_default();

        let mut result = Vec::new();
        for sr in &sense_rows {
            let sense_id = text_col(sr, "id");
            let def_rows = conn
                .execute(
                    "SELECT source, definition, citations, bdb_page, notes
                     FROM sense_definitions
                     WHERE sense_id = $1::uuid
                     ORDER BY source",
                    &[PgValue::Text(sense_id)],
                )
                .await
                .unwrap_or_default();

            result.push(LexiconSense {
                binyan: opt_text_col(sr, "binyan"),
                sense_index: int_col(sr, "sense_index"),
                definitions: def_rows.iter().map(|d| SenseDefinition {
                    source: text_col(d, "source"),
                    definition: text_col(d, "definition"),
                    citations: opt_text_col(d, "citations"),
                    bdb_page: opt_text_col(d, "bdb_page"),
                    notes: opt_text_col(d, "notes"),
                }).collect(),
            });
        }
        result
    };

    // ── 9. Token enrichment (aggregate across passage_tokens) ────────────────
    let token_enrichment = {
        let rows = conn
            .execute(
                "SELECT DISTINCT ON (louw_nida_domain)
                        louw_nida_domain, bdag_catchword
                 FROM passage_tokens
                 WHERE vocabulary_item_id = $1::uuid AND louw_nida_domain IS NOT NULL
                 LIMIT 1",
                &[PgValue::Text(vocab_id.clone())],
            )
            .await
            .unwrap_or_default();

        let morph_rows = conn
            .execute(
                "SELECT DISTINCT morphology_human
                 FROM passage_tokens
                 WHERE vocabulary_item_id = $1::uuid AND morphology_human IS NOT NULL
                 LIMIT 10",
                &[PgValue::Text(vocab_id.clone())],
            )
            .await
            .unwrap_or_default();

        if let Some(r) = rows.first() {
            Some(TokenEnrichment {
                louw_nida_domain: opt_text_col(r, "louw_nida_domain"),
                bdag_catchword: opt_text_col(r, "bdag_catchword"),
                morphology_forms: morph_rows.iter().map(|m| text_col(m, "morphology_human")).collect(),
            })
        } else if !morph_rows.is_empty() {
            Some(TokenEnrichment {
                louw_nida_domain: None,
                bdag_catchword: None,
                morphology_forms: morph_rows.iter().map(|m| text_col(m, "morphology_human")).collect(),
            })
        } else {
            None
        }
    };

    // ── 10. Word study entries (commentary) ──────────────────────────────────
    let study_entries = {
        let rows = conn
            .execute(
                "SELECT ws.book,
                        COALESCE(ws.issue_type::text, 'word_meaning') AS issue_type,
                        COALESCE(ws.issue_description, '') AS issue_description,
                        COALESCE(ws.harris_summary, ws.editor_summary, '') AS content,
                        ws.fallacy_warning, ws.lexical_domain,
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
        rows.iter().map(|r| StudyEntry {
            series: text_col(r, "series"),
            book: text_col(r, "book"),
            issue_type: text_col(r, "issue_type"),
            issue_description: text_col(r, "issue_description"),
            content: text_col(r, "content"),
            fallacy_warning: opt_text_col(r, "fallacy_warning"),
            lexical_domain: opt_text_col(r, "lexical_domain"),
        }).collect()
    };

    // ── 11. Cross-references (from books where this word has studies) ────────
    let cross_refs = {
        let rows = conn
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
        rows.iter().map(|r| StudyCrossRef {
            book: text_col(r, "book"),
            verse_ref: text_col(r, "verse_ref"),
            target_ref: text_col(r, "target_ref"),
            relationship: text_col(r, "relationship"),
            note: opt_text_col(r, "note"),
        }).collect()
    };

    // ── 12. Scholarly positions (from books where this word appears) ─────────
    let scholarly_positions = {
        let rows = conn
            .execute(
                "SELECT cp.book, cp.passage_ref, cp.issue_type::text, cp.issue_description,
                        cp.harris_conclusion
                 FROM commentator_positions cp
                 WHERE cp.book IN (
                     SELECT DISTINCT ws3.book FROM word_study_entries ws3
                     WHERE ws3.vocabulary_item_id = $1::uuid
                 )
                 ORDER BY cp.book
                 LIMIT 20",
                &[PgValue::Text(vocab_id.clone())],
            )
            .await
            .unwrap_or_default();
        rows.iter().map(|r| ScholarlyPosition {
            book: text_col(r, "book"),
            passage_ref: text_col(r, "passage_ref"),
            issue_type: text_col(r, "issue_type"),
            issue_description: text_col(r, "issue_description"),
            harris_conclusion: opt_text_col(r, "harris_conclusion"),
        }).collect()
    };

    // ── 13. Discourse notes ──────────────────────────────────────────────────
    let discourse_notes = {
        let rows = conn
            .execute(
                "SELECT dn.book, dn.verse_ref, dn.note_type, dn.description
                 FROM commentary_discourse_notes dn
                 WHERE dn.book IN (
                     SELECT DISTINCT ws4.book FROM word_study_entries ws4
                     WHERE ws4.vocabulary_item_id = $1::uuid
                 )
                 ORDER BY dn.book, dn.verse_ref
                 LIMIT 30",
                &[PgValue::Text(vocab_id.clone())],
            )
            .await
            .unwrap_or_default();
        rows.iter().map(|r| DiscourseNote {
            book: text_col(r, "book"),
            verse_ref: text_col(r, "verse_ref"),
            note_type: text_col(r, "note_type"),
            description: text_col(r, "description"),
        }).collect()
    };

    // ── 14. Theological applications ─────────────────────────────────────────
    let applications = {
        let rows = conn
            .execute(
                "SELECT ca.book, ca.verse_ref, ca.theme, ca.content
                 FROM commentary_applications ca
                 WHERE ca.book IN (
                     SELECT DISTINCT ws5.book FROM word_study_entries ws5
                     WHERE ws5.vocabulary_item_id = $1::uuid
                 )
                 ORDER BY ca.book, ca.verse_ref
                 LIMIT 20",
                &[PgValue::Text(vocab_id.clone())],
            )
            .await
            .unwrap_or_default();
        rows.iter().map(|r| Application {
            book: text_col(r, "book"),
            verse_ref: text_col(r, "verse_ref"),
            theme: opt_text_col(r, "theme"),
            content: text_col(r, "content"),
        }).collect()
    };

    // ── 15. Discovery heat ───────────────────────────────────────────────────
    let heat = if let Some(ref cid) = concept_id {
        let rows = conn
            .execute(
                "SELECT AVG(normalized_heat) AS heat, SUM(event_count) AS event_count
                 FROM discovery_heat WHERE concept_id = $1::uuid",
                &[PgValue::Text(cid.clone())],
            )
            .await
            .unwrap_or_default();
        rows.first().and_then(|r| {
            let h = opt_float_col(r, "heat")?;
            Some(HeatInfo {
                heat: h,
                event_count: int_col(r, "event_count"),
            })
        })
    } else {
        None
    };

    Ok(Some(WordStudyResult {
        id: vocab_id,
        lemma: text_col(vocab, "lemma"),
        language_code: text_col(vocab, "language_code"),
        transliteration: opt_text_col(vocab, "transliteration"),
        ipa: opt_text_col(vocab, "ipa"),
        short_gloss: opt_text_col(vocab, "short_gloss"),
        part_of_speech: opt_text_col(vocab, "part_of_speech"),
        strong_number: opt_text_col(vocab, "strong_number"),
        frequency_count: opt_int_col(vocab, "frequency_count"),
        frequency_rank: opt_int_col(vocab, "frequency_rank"),
        audio_url: opt_text_col(vocab, "audio_url"),
        notes: opt_text_col(vocab, "notes"),
        extended_notes: opt_text_col(vocab, "extended_notes"),
        concept,
        relationships,
        concordance,
        book_counts,
        depth_insights,
        root,
        senses,
        token_enrichment,
        study_entries,
        cross_refs,
        scholarly_positions,
        discourse_notes,
        applications,
        heat,
    }))
}
