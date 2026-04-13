//! Steps 1-5 resolver: genre, literary units, text criticism, translation, clause grammar.
//!
//! All SQL uses RequestConnection + PgValue — same pattern as reader.rs.
//! passage_tokens joined to passages for book/chapter/verse lookup.

use async_graphql::dynamic::*;
use fw_decode::{
    decode_hebrew_morphology, decode_greek_morphology,
    hebrew_plain, greek_plain,
    decode_witness, decode_operation, decode_masorah, decode_apparatus_source,
};
use fw_decode::theology::{
    reading_posture, genre_context, domain_name,
    compute_significance, SignificanceFactors,
};
use fw_graph_types::PgValue;

use crate::executor::RequestConnection;
use super::structs::*;

// ── Column helpers (mirrors reader.rs) ──────────────────────────────────────

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

fn int_col(row: &fw_graph_types::PgRow, col: &str) -> i64 {
    match row.get(col) {
        Some(PgValue::Int(n)) => *n,
        _ => 0,
    }
}

fn float_col(row: &fw_graph_types::PgRow, col: &str) -> f64 {
    match row.get(col) {
        Some(PgValue::Float(f)) => *f,
        Some(PgValue::Int(n)) => *n as f64,
        _ => 0.0,
    }
}

fn bool_col(row: &fw_graph_types::PgRow, col: &str) -> bool {
    match row.get(col) {
        Some(PgValue::Bool(b)) => *b,
        _ => false,
    }
}

// ── SQL Fetch Functions ──────────────────────────────────────────────────────

/// Fetch genre classification for a verse.
pub async fn fetch_genre_step(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
    verse: i64,
) -> Result<Option<GenreStep>, async_graphql::Error> {
    let sql = r#"
SELECT genre, sub_type
FROM genre_sections
WHERE book = $1 AND chapter = $2
  AND verse_start <= $3 AND verse_end >= $3
ORDER BY verse_start
LIMIT 1
"#;
    let rows = conn.execute(sql, &[
        PgValue::Text(book.to_owned()),
        PgValue::Int(chapter),
        PgValue::Int(verse),
    ]).await.map_err(|e| async_graphql::Error::new(format!("genre_sections query: {e}")))?;

    Ok(rows.first().map(|row| {
        let primary = text_col(row, "genre");
        let sub_type = opt_text_col(row, "sub_type");
        let posture = reading_posture(book).to_string();
        let ctx = genre_context(&primary, book).to_string();
        GenreStep { primary, sub_type, reading_posture: posture, genre_context: ctx }
    }))
}

/// Fetch literary unit info for a verse (pericope + outline path).
pub async fn fetch_literary_units_step(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
    verse: i64,
) -> Result<Option<LiteraryUnitsStep>, async_graphql::Error> {
    let sql = r#"
SELECT
  book || '.' || chapter_start || '.' || verse_start
    || CASE WHEN verse_end IS NOT NULL THEN '-' || verse_end ELSE '' END AS pericope_ref,
  pericope_title AS title
FROM pericope_units
WHERE book = $1
  AND chapter_start <= $2
  AND (chapter_end IS NULL OR chapter_end >= $2)
  AND verse_start <= $3
  AND (verse_end IS NULL OR verse_end >= $3)
ORDER BY sort_order
LIMIT 1
"#;
    let rows = conn.execute(sql, &[
        PgValue::Text(book.to_owned()),
        PgValue::Int(chapter),
        PgValue::Int(verse),
    ]).await.map_err(|e| async_graphql::Error::new(format!("pericope_units query: {e}")))?;

    Ok(rows.first().map(|row| {
        let pericope_ref = text_col(row, "pericope_ref");
        let title = text_col(row, "title");
        let pericope = if pericope_ref.is_empty() {
            None
        } else {
            Some(PericopeInfoTocma { r#ref: pericope_ref, title })
        };
        LiteraryUnitsStep {
            pericope,
            outline_path: vec![],
            clause_depth: None,
        }
    }))
}

/// Fetch apparatus entries for a verse, decoding witness sigla.
/// tc_apparatus.witnesses is stored as text[] — cast to text for PgValue access.
pub async fn fetch_apparatus_entries(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
    verse: i64,
) -> Result<Vec<ApparatusEntry>, async_graphql::Error> {
    let sql = r#"
SELECT
  ta.note_text,
  array_to_string(ta.witnesses, ',') AS witnesses_csv,
  ta.variant_type::text AS operation,
  ta.apparatus_source::text AS source
FROM tc_apparatus ta
JOIN passages p ON p.id = ta.passage_id
WHERE p.book = $1 AND p.chapter = $2 AND p.verse = $3
ORDER BY ta.bhs_page
"#;
    let rows = conn.execute(sql, &[
        PgValue::Text(book.to_owned()),
        PgValue::Int(chapter),
        PgValue::Int(verse),
    ]).await.map_err(|e| async_graphql::Error::new(format!("tc_apparatus query: {e}")))?;

    Ok(rows.iter().map(|row| {
        let note_text = text_col(row, "note_text");
        let source = text_col(row, "source");
        let source_decoded = decode_apparatus_source(&source).to_string();
        let operation_raw = opt_text_col(row, "operation");
        let operation = operation_raw.as_deref().map(|op| decode_operation(op).to_string());

        let witnesses_csv = text_col(row, "witnesses_csv");
        let witnesses: Vec<DecodedWitness> = witnesses_csv
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|siglum| {
                let dw = decode_witness(siglum);
                DecodedWitness {
                    siglum: dw.siglum.clone(),
                    full_name: dw.full_name.clone(),
                    tradition: dw.tradition.to_string(),
                    language: dw.language.to_string(),
                    date_range: dw.date_range.map(|s| s.to_string()),
                }
            })
            .collect();

        ApparatusEntry { note_text, witnesses, operation, source, source_decoded }
    }).collect())
}

/// Fetch masorah entries for a verse, decoding each note.
pub async fn fetch_masorah_entries(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
    verse: i64,
) -> Result<Vec<MasorahEntry>, async_graphql::Error> {
    let sql = r#"
SELECT
  tm.mp_note_text,
  tm.is_hapax,
  tm.xlit
FROM tc_masorah tm
JOIN passages p ON p.id = tm.passage_id
WHERE p.book = $1 AND p.chapter = $2 AND p.verse = $3
ORDER BY tm.word_num
"#;
    let rows = conn.execute(sql, &[
        PgValue::Text(book.to_owned()),
        PgValue::Int(chapter),
        PgValue::Int(verse),
    ]).await.map_err(|e| async_graphql::Error::new(format!("tc_masorah query: {e}")))?;

    Ok(rows.iter().map(|row| {
        let mp_note_text = text_col(row, "mp_note_text");
        let decoded = decode_masorah(&mp_note_text);
        MasorahEntry {
            mp_note_text,
            decoded,
            is_hapax: bool_col(row, "is_hapax"),
            word: opt_text_col(row, "xlit"),
        }
    }).collect())
}

/// Fetch BSB + KJV + phrased translation for a verse.
pub async fn fetch_translation_step(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
    verse: i64,
) -> Result<Option<TranslationStep>, async_graphql::Error> {
    // passages table has original_text + translation_text
    let sql = r#"
SELECT
  p.original_text AS original,
  p.pointed_text AS pointed,
  p.translation_text AS bsb,
  NULL::text AS kjv
FROM passages p
WHERE p.book = $1 AND p.chapter = $2 AND p.verse = $3
LIMIT 1
"#;
    let rows = conn.execute(sql, &[
        PgValue::Text(book.to_owned()),
        PgValue::Int(chapter),
        PgValue::Int(verse),
    ]).await.map_err(|e| async_graphql::Error::new(format!("passages query: {e}")))?;

    Ok(rows.first().map(|row| TranslationStep {
        bsb: opt_text_col(row, "bsb"),
        kjv: opt_text_col(row, "kjv"),
        original: opt_text_col(row, "original"),
        pointed: opt_text_col(row, "pointed"),
        phrased_text: None,
        phrased_indent: None,
        divergences: vec![],
    }))
}

/// Fetch passage tokens for a verse range, applying fw-decode + fw-translate.
pub async fn fetch_passage_tokens(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> Result<Vec<PassageToken>, async_graphql::Error> {
    let sql = r#"
SELECT
  p.verse,
  pt.position,
  pt.surface_form AS original_word,
  COALESCE(pt.transliteration, '') AS transliteration,
  COALESCE(pt.morphology_code, '') AS morphology_code,
  COALESCE(pt.gloss, '') AS gloss,
  COALESCE(pt.strong_number, '') AS strong_number,
  p.language_code::text AS lang,
  COALESCE(vi.short_gloss, pt.gloss, '') AS short_gloss,
  COALESCE(vi.semantic_domain, '') AS semantic_domain,
  COALESCE(vi.frequency_count, 0) AS frequency_count
FROM passage_tokens pt
JOIN passages p ON p.id = pt.passage_id
LEFT JOIN vocabulary_items vi ON vi.id = pt.vocabulary_item_id
WHERE p.book = $1 AND p.chapter = $2
  AND p.verse >= $3 AND p.verse <= $4
ORDER BY p.verse, pt.position
"#;
    let rows = conn.execute(sql, &[
        PgValue::Text(book.to_owned()),
        PgValue::Int(chapter),
        PgValue::Int(verse_start),
        PgValue::Int(verse_end),
    ]).await.map_err(|e| async_graphql::Error::new(format!("passage_tokens query: {e}")))?;

    Ok(rows.iter().map(|row| {
        let lang = text_col(row, "lang");
        let morph = text_col(row, "morphology_code");
        let orig = text_col(row, "original_word");
        let xlit = text_col(row, "transliteration");
        let gloss = text_col(row, "short_gloss");

        let (morphology_decoded, morphology_plain) = if lang.contains("grc") {
            (decode_greek_morphology(&morph), greek_plain(&morph).to_string())
        } else {
            (decode_hebrew_morphology(&morph), hebrew_plain(&morph).to_string())
        };

        let (gloss_english, gloss_prefix, gloss_subject, gloss_core, gloss_suffix, gloss_src) =
            translate_token(&lang, &morph, &orig, &xlit, book, &gloss);

        let freq = float_col(row, "frequency_count").max(1.0);
        let rarity = (1.0 - (freq.ln() / 10.0_f64.ln())).clamp(0.0, 1.0) as f32;
        let significance = compute_significance(&SignificanceFactors {
            frequency_rarity: rarity,
            ..Default::default()
        });

        let sem_domain = opt_text_col(row, "semantic_domain")
            .filter(|s| !s.is_empty());
        let domain_name_str = sem_domain.as_deref().map(|d| {
            let first = d.split(',').next().unwrap_or(d).trim();
            let major = first.split('.').next().unwrap_or(first);
            domain_name(major).to_string()
        });

        PassageToken {
            verse: int_col(row, "verse"),
            position: int_col(row, "position"),
            original_word: orig,
            transliteration: xlit,
            morphology_code: morph,
            morphology_decoded,
            morphology_plain,
            fellwork_gloss_english: gloss_english,
            fellwork_gloss_prefix: gloss_prefix,
            fellwork_gloss_subject: gloss_subject,
            fellwork_gloss_core: gloss_core,
            fellwork_gloss_suffix: gloss_suffix,
            fellwork_gloss_source: gloss_src,
            significance,
            louw_nida_domain: sem_domain,
            louw_nida_domain_name: domain_name_str,
        }
    }).collect())
}

/// Call fw-translate for a single token.
fn translate_token(
    lang: &str,
    morph: &str,
    _orig: &str,
    _xlit: &str,
    _book: &str,
    base_gloss: &str,
) -> (String, Option<String>, Option<String>, String, Option<String>, String) {
    if lang.contains("grc") {
        let parsed = fw_translate::greek::parse(morph);
        let result = fw_translate::greek::render(&parsed, base_gloss, base_gloss, None, &fw_translate::greek::RenderContext::default());
        (
            result.english.clone(),
            result.components.prefix.clone(),
            result.components.subject.clone(),
            result.components.core.clone(),
            result.components.suffix.clone(),
            format!("{:?}", result.components.gloss_source),
        )
    } else {
        let parsed = fw_translate::hebrew::parse(morph);
        let result = fw_translate::hebrew::render(&parsed, base_gloss, None, base_gloss, None, None);
        (
            result.english.clone(),
            result.components.prefix.clone(),
            result.components.subject.clone(),
            result.components.core.clone(),
            result.components.suffix.clone(),
            format!("{:?}", result.components.gloss_source),
        )
    }
}

// ── GraphQL Type Registration ────────────────────────────────────────────────

pub fn register_verse_types(builder: SchemaBuilder) -> SchemaBuilder {
    let decoded_witness = Object::new("TocmaDecodedWitness")
        .field(Field::new("siglum",    TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DecodedWitness>()?;
                Ok(Some(FieldValue::value(v.siglum.clone())))
            })
        }))
        .field(Field::new("fullName",  TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DecodedWitness>()?;
                Ok(Some(FieldValue::value(v.full_name.clone())))
            })
        }))
        .field(Field::new("tradition", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DecodedWitness>()?;
                Ok(Some(FieldValue::value(v.tradition.clone())))
            })
        }))
        .field(Field::new("language",  TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DecodedWitness>()?;
                Ok(Some(FieldValue::value(v.language.clone())))
            })
        }))
        .field(Field::new("dateRange", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DecodedWitness>()?;
                Ok(v.date_range.clone().map(FieldValue::value))
            })
        }));

    let apparatus_entry = Object::new("TocmaApparatusEntry")
        .field(Field::new("noteText",     TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ApparatusEntry>()?;
                Ok(Some(FieldValue::value(v.note_text.clone())))
            })
        }))
        .field(Field::new("sourceDecoded", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ApparatusEntry>()?;
                Ok(Some(FieldValue::value(v.source_decoded.clone())))
            })
        }))
        .field(Field::new("operation",    TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ApparatusEntry>()?;
                Ok(v.operation.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("witnesses",    TypeRef::named_nn_list_nn("TocmaDecodedWitness"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ApparatusEntry>()?;
                let values: Vec<FieldValue> = v.witnesses.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }));

    let masorah_entry = Object::new("TocmaMasorahEntry")
        .field(Field::new("mpNoteText", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<MasorahEntry>()?;
                Ok(Some(FieldValue::value(v.mp_note_text.clone())))
            })
        }))
        .field(Field::new("decoded",    TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<MasorahEntry>()?;
                Ok(Some(FieldValue::value(v.decoded.clone())))
            })
        }))
        .field(Field::new("isHapax",    TypeRef::named_nn(TypeRef::BOOLEAN), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<MasorahEntry>()?;
                Ok(Some(FieldValue::value(v.is_hapax)))
            })
        }))
        .field(Field::new("word",       TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<MasorahEntry>()?;
                Ok(v.word.clone().map(FieldValue::value))
            })
        }));

    let passage_token = Object::new("TocmaPassageToken")
        .field(Field::new("verse",             TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(Some(FieldValue::value(v.verse)))
            })
        }))
        .field(Field::new("position",          TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(Some(FieldValue::value(v.position)))
            })
        }))
        .field(Field::new("originalWord",      TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(Some(FieldValue::value(v.original_word.clone())))
            })
        }))
        .field(Field::new("transliteration",   TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(Some(FieldValue::value(v.transliteration.clone())))
            })
        }))
        .field(Field::new("morphologyCode",    TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(Some(FieldValue::value(v.morphology_code.clone())))
            })
        }))
        .field(Field::new("morphologyDecoded", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(Some(FieldValue::value(v.morphology_decoded.clone())))
            })
        }))
        .field(Field::new("morphologyPlain",   TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(Some(FieldValue::value(v.morphology_plain.clone())))
            })
        }))
        .field(Field::new("glossEnglish",      TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(Some(FieldValue::value(v.fellwork_gloss_english.clone())))
            })
        }))
        .field(Field::new("glossCore",         TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(Some(FieldValue::value(v.fellwork_gloss_core.clone())))
            })
        }))
        .field(Field::new("significance",      TypeRef::named_nn(TypeRef::FLOAT), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(Some(FieldValue::value(v.significance as f64)))
            })
        }))
        .field(Field::new("louwNidaDomain",    TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(v.louw_nida_domain.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("louwNidaDomainName", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PassageToken>()?;
                Ok(v.louw_nida_domain_name.clone().map(FieldValue::value))
            })
        }));

    let pericope_info = Object::new("TocmaPericopeInfo")
        .field(Field::new("ref",   TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeInfoTocma>()?;
                Ok(Some(FieldValue::value(v.r#ref.clone())))
            })
        }))
        .field(Field::new("title", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<PericopeInfoTocma>()?;
                Ok(Some(FieldValue::value(v.title.clone())))
            })
        }));

    let literary_units = Object::new("TocmaLiteraryUnitsStep")
        .field(Field::new("pericope",    TypeRef::named("TocmaPericopeInfo"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<LiteraryUnitsStep>()?;
                Ok(v.pericope.clone().map(FieldValue::owned_any))
            })
        }))
        .field(Field::new("outlinePath", TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<LiteraryUnitsStep>()?;
                let values: Vec<FieldValue> = v.outline_path.iter().cloned().map(FieldValue::value).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }));

    let genre_step = Object::new("TocmaGenreStep")
        .field(Field::new("primary",        TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<GenreStep>()?;
                Ok(Some(FieldValue::value(v.primary.clone())))
            })
        }))
        .field(Field::new("subType",        TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<GenreStep>()?;
                Ok(v.sub_type.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("readingPosture", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<GenreStep>()?;
                Ok(Some(FieldValue::value(v.reading_posture.clone())))
            })
        }))
        .field(Field::new("genreContext",   TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<GenreStep>()?;
                Ok(Some(FieldValue::value(v.genre_context.clone())))
            })
        }));

    let translation_step = Object::new("TocmaTranslationStep")
        .field(Field::new("bsb",      TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TranslationStep>()?;
                Ok(v.bsb.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("original", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TranslationStep>()?;
                Ok(v.original.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("pointed",  TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TranslationStep>()?;
                Ok(v.pointed.clone().map(FieldValue::value))
            })
        }));

    // VerseCard object
    let verse_card = Object::new("TocmaVerseCard")
        .field(Field::new("ref",           TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseCardOutput>()?;
                Ok(Some(FieldValue::value(v.r#ref.clone())))
            })
        }))
        .field(Field::new("pericopeRef",   TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseCardOutput>()?;
                Ok(v.pericope_ref.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("genre",         TypeRef::named("TocmaGenreStep"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseCardOutput>()?;
                Ok(v.genre.clone().map(FieldValue::owned_any))
            })
        }))
        .field(Field::new("literaryUnits", TypeRef::named("TocmaLiteraryUnitsStep"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseCardOutput>()?;
                Ok(v.literary_units.clone().map(FieldValue::owned_any))
            })
        }))
        .field(Field::new("apparatus",     TypeRef::named_nn_list_nn("TocmaApparatusEntry"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseCardOutput>()?;
                let values: Vec<FieldValue> = v.apparatus.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("masorah",       TypeRef::named_nn_list_nn("TocmaMasorahEntry"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseCardOutput>()?;
                let values: Vec<FieldValue> = v.masorah.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("translation",   TypeRef::named("TocmaTranslationStep"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseCardOutput>()?;
                Ok(v.translation.clone().map(FieldValue::owned_any))
            })
        }))
        .field(Field::new("tokens",        TypeRef::named_nn_list_nn("TocmaPassageToken"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<VerseCardOutput>()?;
                let values: Vec<FieldValue> = v.tokens.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }));

    builder
        .register(decoded_witness)
        .register(apparatus_entry)
        .register(masorah_entry)
        .register(passage_token)
        .register(pericope_info)
        .register(literary_units)
        .register(genre_step)
        .register(translation_step)
        .register(verse_card)
}

// ── VerseCard output aggregate ────────────────────────────────────────────────

/// Assembled verse card returned by the `tocmaVerse` query.
#[derive(Clone)]
pub struct VerseCardOutput {
    pub r#ref: String,
    pub pericope_ref: Option<String>,
    pub genre: Option<GenreStep>,
    pub literary_units: Option<LiteraryUnitsStep>,
    pub apparatus: Vec<ApparatusEntry>,
    pub masorah: Vec<MasorahEntry>,
    pub translation: Option<TranslationStep>,
    pub tokens: Vec<PassageToken>,
}

// ── Root query field ─────────────────────────────────────────────────────────

/// `tocmaVerse(book: String!, chapter: Int!, verse: Int!): TocmaVerseCard`
pub fn tocma_verse_field() -> Field {
    Field::new(
        "tocmaVerse",
        TypeRef::named("TocmaVerseCard"),
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
                let verse = ctx.args.try_get("verse")?.i64()
                    .map_err(|_| async_graphql::Error::new("verse must be an int"))?;

                let verse_ref = format!("{book}.{chapter}.{verse}");

                let (genre, literary_units, apparatus, masorah, translation, tokens) = tokio::join!(
                    fetch_genre_step(conn, &book, chapter, verse),
                    fetch_literary_units_step(conn, &book, chapter, verse),
                    fetch_apparatus_entries(conn, &book, chapter, verse),
                    fetch_masorah_entries(conn, &book, chapter, verse),
                    fetch_translation_step(conn, &book, chapter, verse),
                    fetch_passage_tokens(conn, &book, chapter, verse, verse),
                );

                let pericope_ref = literary_units.as_ref().ok().and_then(|lu| {
                    lu.as_ref().and_then(|u| u.pericope.as_ref().map(|p| p.r#ref.clone()))
                });

                let card = VerseCardOutput {
                    r#ref: verse_ref,
                    pericope_ref,
                    genre: genre.unwrap_or(None),
                    literary_units: literary_units.unwrap_or(None),
                    apparatus: apparatus.unwrap_or_default(),
                    masorah: masorah.unwrap_or_default(),
                    translation: translation.unwrap_or(None),
                    tokens: tokens.unwrap_or_default(),
                };

                Ok(Some(FieldValue::owned_any(card)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("verse",   TypeRef::named_nn(TypeRef::INT)))
}
