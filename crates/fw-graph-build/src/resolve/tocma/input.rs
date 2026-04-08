//! TheologyInput aggregate — curated entry point for Spec 2 (AI theology pipeline).
//!
//! Assembles a ranked, capped subset of Steps 6-12 data for efficient AI processing.
//! Priority: cross-testament arcs > word studies (top 3 by significance) > depth insights (top 3).

use async_graphql::dynamic::*;
use fw_graph_types::PgValue;

use crate::executor::RequestConnection;
use super::structs::*;
use super::theology::{
    fetch_biblical_theology, classify_doctrine_step, fetch_practical_theology,
    BiblicalTheologyOutput, SystematicTheologyOutput,
};
use super::verse::fetch_passage_tokens;

// ── Column helpers ───────────────────────────────────────────────────────────

fn text_col(row: &fw_graph_types::PgRow, col: &str) -> String {
    match row.get(col) {
        Some(PgValue::Text(s)) => s.clone(),
        _ => String::new(),
    }
}

fn opt_text_col(row: &fw_graph_types::PgRow, col: &str) -> Option<String> {
    match row.get(col) {
        Some(PgValue::Text(s)) => Some(s.clone()),
        _ => None,
    }
}

fn float_col(row: &fw_graph_types::PgRow, col: &str) -> f64 {
    match row.get(col) {
        Some(PgValue::Float(f)) => *f,
        Some(PgValue::Int(n)) => *n as f64,
        _ => 0.0,
    }
}

// ── Arc priority for ranking ─────────────────────────────────────────────────

fn arc_priority(link_type: &str) -> u8 {
    match link_type {
        "cross_testament_bridge" => 0,
        "typological_fulfillment" => 1,
        "canonical_reinterpretation" => 2,
        "intertextual_allusion" => 3,
        "concept_bridge" => 4,
        _ => 5, // lexical_echo and unknowns
    }
}

// ── Fetch depth insights (top 3 by confidence) ───────────────────────────────

async fn fetch_depth_insights(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Vec<DepthInsightSummary> {
    let sql = r#"
SELECT title, body, insight_type, COALESCE(confidence, 0.5) AS confidence
FROM depth_insights
WHERE book = $1 AND chapter = $2
ORDER BY confidence DESC, created_at DESC
LIMIT 3
"#;
    let rows = conn.execute(sql, &[
        PgValue::Text(book.to_owned()),
        PgValue::Int(chapter),
    ]).await.unwrap_or_default();

    rows.iter().map(|row| DepthInsightSummary {
        title: text_col(row, "title"),
        body: text_col(row, "body"),
        insight_type: text_col(row, "insight_type"),
        confidence: float_col(row, "confidence"),
    }).collect()
}

/// Fetch any existing AI synthesis from tocma_pericope_synthesis (if Spec 2 has run).
async fn fetch_existing_synthesis(
    conn: &RequestConnection,
    pericope_ref: &str,
) -> ExistingSynthesis {
    let sql = r#"
SELECT
  biblical_theology_synthesis,
  systematic_theology_content,
  practical_theology_synthesis
FROM tocma_pericope_synthesis
WHERE pericope_ref = $1
LIMIT 1
"#;
    let rows = conn.execute(sql, &[PgValue::Text(pericope_ref.to_owned())])
        .await.unwrap_or_default();

    rows.first().map(|row| ExistingSynthesis {
        biblical_theology: opt_text_col(row, "biblical_theology_synthesis"),
        systematic_theology: opt_text_col(row, "systematic_theology_content"),
        practical_theology: opt_text_col(row, "practical_theology_synthesis"),
    }).unwrap_or(ExistingSynthesis {
        biblical_theology: None,
        systematic_theology: None,
        practical_theology: None,
    })
}

// ── TheologyInput aggregate ──────────────────────────────────────────────────

/// The curated input package for Spec 2 (AI theology pipeline).
#[derive(Clone)]
pub struct TheologyInput {
    /// Pericope reference (e.g. "John.1.1-18").
    pub r#ref: String,
    /// Pericope title.
    pub title: String,
    /// Book name (for context).
    pub book: String,

    /// Steps 1-5: key tokens for this pericope (all verses).
    pub tokens: Vec<PassageToken>,

    /// Step 6: Discourse notes.
    pub discourse_notes: Vec<DiscourseNote>,
    /// Step 6: Logical flow labels.
    pub logical_flow: Vec<String>,

    /// Step 7: Top 3 word studies by significance.
    pub top_word_studies: Vec<WordStudyEntry>,

    /// Step 8: Historical context notes.
    pub historical_notes: Vec<CommentaryNote>,

    /// Step 9: Intertextual cross-references.
    pub intertextual_refs: Vec<String>,

    /// Step 10: Biblical theology arcs (sorted by priority, capped at 10).
    pub biblical_theology: Option<BiblicalTheologyOutput>,

    /// Step 11: Systematic theology doctrine hints.
    pub doctrine_hints: Option<SystematicTheologyOutput>,

    /// Step 12: Practical theology applications.
    pub applications: Vec<ApplicationEntry>,

    /// Depth insights for this chapter (top 3).
    pub depth_insights: Vec<DepthInsightSummary>,

    /// Any existing AI synthesis from a previous Spec 2 run.
    pub existing_synthesis: ExistingSynthesis,

    /// Literary position context.
    pub literary_position: Option<LiteraryPosition>,
}

pub async fn assemble_theology_input(
    conn: &RequestConnection,
    book: &str,
    pericope_ref: &str,
    title: &str,
    chapter: i64,
    verse_start: i64,
    verse_end: i64,
) -> TheologyInput {
    use super::pericope::{
        fetch_argument_tracing, fetch_word_studies,
        fetch_historical_context, fetch_literary_context,
    };

    // Parallel fetch of all data sources
    let (
        arg_res,
        word_studies_res,
        hist_res,
        lit_res,
        bt_res,
        st_res,
        pt_res,
        tokens_res,
        insights,
        synthesis,
    ) = tokio::join!(
        fetch_argument_tracing(conn, book, pericope_ref),
        fetch_word_studies(conn, book),
        fetch_historical_context(conn, book),
        fetch_literary_context(conn, book),
        fetch_biblical_theology(conn, book),
        classify_doctrine_step(conn, book, chapter, verse_start, verse_end),
        fetch_practical_theology(conn, book),
        fetch_passage_tokens(conn, book, chapter, verse_start, verse_end),
        fetch_depth_insights(conn, book, chapter),
        fetch_existing_synthesis(conn, pericope_ref),
    );

    let (discourse_notes, logical_flow) = arg_res.unwrap_or_default();
    let (historical_notes, _entities) = hist_res.unwrap_or(None).unwrap_or_default();
    let (_outline_pos, _pericope_summary, intertextual_refs) =
        lit_res.unwrap_or(None).unwrap_or((None, None, vec![]));

    // Top 3 word studies by significance (desc)
    let mut word_studies = word_studies_res.unwrap_or_default();
    word_studies.sort_by(|a, b| b.significance.partial_cmp(&a.significance).unwrap_or(std::cmp::Ordering::Equal));
    let top_word_studies = word_studies.into_iter().take(3).collect();

    // Sort biblical theology arcs by priority, cap at 10
    let biblical_theology = bt_res.unwrap_or(None).map(|mut bt| {
        for theme in &mut bt.themes {
            theme.arc.sort_by_key(|a| arc_priority(&a.link_type));
            theme.arc.truncate(10);
        }
        bt
    });

    TheologyInput {
        r#ref: pericope_ref.to_string(),
        title: title.to_string(),
        book: book.to_string(),
        tokens: tokens_res.unwrap_or_default(),
        discourse_notes,
        logical_flow,
        top_word_studies,
        historical_notes,
        intertextual_refs,
        biblical_theology,
        doctrine_hints: st_res.unwrap_or(None),
        applications: pt_res.unwrap_or(None).unwrap_or_default(),
        depth_insights: insights,
        existing_synthesis: synthesis,
        literary_position: None,
    }
}

// ── GraphQL Type Registration ─────────────────────────────────────────────────

pub fn register_input_types(builder: SchemaBuilder) -> SchemaBuilder {
    let depth_insight_summary = Object::new("TocmaDepthInsightSummary")
        .field(Field::new("title",       TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DepthInsightSummary>()?;
                Ok(Some(FieldValue::value(v.title.clone())))
            })
        }))
        .field(Field::new("body",        TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DepthInsightSummary>()?;
                Ok(Some(FieldValue::value(v.body.clone())))
            })
        }))
        .field(Field::new("insightType", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DepthInsightSummary>()?;
                Ok(Some(FieldValue::value(v.insight_type.clone())))
            })
        }))
        .field(Field::new("confidence",  TypeRef::named_nn(TypeRef::FLOAT), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<DepthInsightSummary>()?;
                Ok(Some(FieldValue::value(v.confidence)))
            })
        }));

    let existing_synthesis = Object::new("TocmaExistingSynthesis")
        .field(Field::new("biblicalTheology",   TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ExistingSynthesis>()?;
                Ok(v.biblical_theology.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("systematicTheology", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ExistingSynthesis>()?;
                Ok(v.systematic_theology.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("practicalTheology",  TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<ExistingSynthesis>()?;
                Ok(v.practical_theology.clone().map(FieldValue::value))
            })
        }));

    let theology_input = Object::new("TocmaTheologyInput")
        .field(Field::new("ref",              TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                Ok(Some(FieldValue::value(v.r#ref.clone())))
            })
        }))
        .field(Field::new("title",            TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                Ok(Some(FieldValue::value(v.title.clone())))
            })
        }))
        .field(Field::new("book",             TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                Ok(Some(FieldValue::value(v.book.clone())))
            })
        }))
        .field(Field::new("tokens",           TypeRef::named_nn_list_nn("TocmaPassageToken"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                let values: Vec<FieldValue> = v.tokens.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("discourseNotes",   TypeRef::named_nn_list_nn("TocmaDiscourseNote"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                let values: Vec<FieldValue> = v.discourse_notes.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("logicalFlow",      TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                let values: Vec<FieldValue> = v.logical_flow.iter().cloned().map(FieldValue::value).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("topWordStudies",   TypeRef::named_nn_list_nn("TocmaWordStudyEntry"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                let values: Vec<FieldValue> = v.top_word_studies.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("historicalNotes",  TypeRef::named_nn_list_nn("TocmaCommentaryNote"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                let values: Vec<FieldValue> = v.historical_notes.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("intertextualRefs", TypeRef::named_nn_list_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                let values: Vec<FieldValue> = v.intertextual_refs.iter().cloned().map(FieldValue::value).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("biblicalTheology", TypeRef::named("TocmaBiblicalTheologyStep"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                Ok(v.biblical_theology.clone().map(FieldValue::owned_any))
            })
        }))
        .field(Field::new("doctrineHints",    TypeRef::named("TocmaSystematicTheologyStep"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                Ok(v.doctrine_hints.clone().map(FieldValue::owned_any))
            })
        }))
        .field(Field::new("applications",     TypeRef::named_nn_list_nn("TocmaApplicationEntry"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                let values: Vec<FieldValue> = v.applications.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("depthInsights",    TypeRef::named_nn_list_nn("TocmaDepthInsightSummary"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                let values: Vec<FieldValue> = v.depth_insights.iter().cloned().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        }))
        .field(Field::new("existingSynthesis", TypeRef::named_nn("TocmaExistingSynthesis"), |ctx| {
            FieldFuture::new(async move {
                let v = ctx.parent_value.try_downcast_ref::<TheologyInput>()?;
                Ok(Some(FieldValue::owned_any(v.existing_synthesis.clone())))
            })
        }));

    builder
        .register(depth_insight_summary)
        .register(existing_synthesis)
        .register(theology_input)
}

// ── Root query field ─────────────────────────────────────────────────────────

/// `theologyInput(book: String!, pericopeRef: String!, chapter: Int!, verseStart: Int!, verseEnd: Int!, title: String): TocmaTheologyInput`
pub fn theology_input_field() -> Field {
    Field::new(
        "theologyInput",
        TypeRef::named("TocmaTheologyInput"),
        |ctx| {
            FieldFuture::new(async move {
                let conn = ctx
                    .data_opt::<RequestConnection>()
                    .ok_or_else(|| async_graphql::Error::new("No database connection"))?;
                let book = ctx.args.try_get("book")?.string()
                    .map_err(|_| async_graphql::Error::new("book must be a string"))?
                    .to_owned();
                let pericope_ref = ctx.args.try_get("pericopeRef")?.string()
                    .map_err(|_| async_graphql::Error::new("pericopeRef must be a string"))?
                    .to_owned();
                let chapter = ctx.args.try_get("chapter")?.i64()
                    .map_err(|_| async_graphql::Error::new("chapter must be an int"))?;
                let verse_start = ctx.args.try_get("verseStart")?.i64()
                    .map_err(|_| async_graphql::Error::new("verseStart must be an int"))?;
                let verse_end = ctx.args.try_get("verseEnd")?.i64()
                    .map_err(|_| async_graphql::Error::new("verseEnd must be an int"))?;
                let title = ctx.args.try_get("title").ok()
                    .and_then(|v| v.string().ok().map(|s| s.to_owned()))
                    .unwrap_or_default();

                let input = assemble_theology_input(
                    conn, &book, &pericope_ref, &title,
                    chapter, verse_start, verse_end,
                ).await;

                Ok(Some(FieldValue::owned_any(input)))
            })
        },
    )
    .argument(InputValue::new("book",        TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("pericopeRef", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter",     TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("verseStart",  TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("verseEnd",    TypeRef::named_nn(TypeRef::INT)))
    .argument(InputValue::new("title",       TypeRef::named(TypeRef::STRING)))
}
