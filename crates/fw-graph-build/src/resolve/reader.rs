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

/// Discovery heat score for a concept in a passage (from materialized view).
#[derive(Clone)]
pub struct DiscoveryHeat {
    pub concept_id: String,
    pub heat: f64,
    pub event_count: i64,
}

/// A genre section — verse-range structural identity within a chapter.
#[derive(Clone)]
pub struct GenreSection {
    pub verse_start: i64,
    pub verse_end: i64,
    pub genre: String,
    pub sub_type: Option<String>,
}

/// A commentary main idea — single-sentence pericope summary (ZECNT).
#[derive(Clone)]
pub struct MainIdea {
    pub id: String,
    pub book: String,
    pub verse_start: String,
    pub verse_end: String,
    pub main_idea: String,
    pub series: String,
}

/// Literary context analysis for a pericope.
#[derive(Clone)]
pub struct LiteraryContext {
    pub id: String,
    pub book: String,
    pub verse_start: String,
    pub verse_end: String,
    pub context_prose: String,    // JSON array as string
    pub scripture_refs: String,   // JSON array as string
    pub series: String,
}

/// A literary structure — chiasm, acrostic, inclusio spanning a verse range.
#[derive(Clone)]
pub struct LiteraryStructure {
    pub structure_type: String,
    pub title: Option<String>,
    pub verse_start: i64,
    pub verse_end: i64,
    pub pairs: String,       // JSON string
    pub center_ref: Option<String>,
    pub source: String,
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

    let discovery_heat = Object::new("DiscoveryHeat")
        .field(Field::new("conceptId", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let h = ctx.parent_value.try_downcast_ref::<DiscoveryHeat>()?;
                Ok(Some(FieldValue::value(h.concept_id.clone())))
            })
        }))
        .field(Field::new("heat", TypeRef::named_nn(TypeRef::FLOAT), |ctx| {
            FieldFuture::new(async move {
                let h = ctx.parent_value.try_downcast_ref::<DiscoveryHeat>()?;
                Ok(Some(FieldValue::value(h.heat)))
            })
        }))
        .field(Field::new("eventCount", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let h = ctx.parent_value.try_downcast_ref::<DiscoveryHeat>()?;
                Ok(Some(FieldValue::value(h.event_count)))
            })
        }));

    let genre_section = Object::new("GenreSection")
        .field(Field::new("verseStart", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let g = ctx.parent_value.try_downcast_ref::<GenreSection>()?;
                Ok(Some(FieldValue::value(g.verse_start)))
            })
        }))
        .field(Field::new("verseEnd", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let g = ctx.parent_value.try_downcast_ref::<GenreSection>()?;
                Ok(Some(FieldValue::value(g.verse_end)))
            })
        }))
        .field(Field::new("genre", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let g = ctx.parent_value.try_downcast_ref::<GenreSection>()?;
                Ok(Some(FieldValue::value(g.genre.clone())))
            })
        }))
        .field(Field::new("subType", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let g = ctx.parent_value.try_downcast_ref::<GenreSection>()?;
                Ok(g.sub_type.clone().map(FieldValue::value))
            })
        }));

    let literary_structure = Object::new("LiteraryStructure")
        .field(Field::new("structureType", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let s = ctx.parent_value.try_downcast_ref::<LiteraryStructure>()?;
                Ok(Some(FieldValue::value(s.structure_type.clone())))
            })
        }))
        .field(Field::new("title", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let s = ctx.parent_value.try_downcast_ref::<LiteraryStructure>()?;
                Ok(s.title.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("verseStart", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let s = ctx.parent_value.try_downcast_ref::<LiteraryStructure>()?;
                Ok(Some(FieldValue::value(s.verse_start)))
            })
        }))
        .field(Field::new("verseEnd", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let s = ctx.parent_value.try_downcast_ref::<LiteraryStructure>()?;
                Ok(Some(FieldValue::value(s.verse_end)))
            })
        }))
        .field(Field::new("pairs", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let s = ctx.parent_value.try_downcast_ref::<LiteraryStructure>()?;
                Ok(Some(FieldValue::value(s.pairs.clone())))
            })
        }))
        .field(Field::new("centerRef", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let s = ctx.parent_value.try_downcast_ref::<LiteraryStructure>()?;
                Ok(s.center_ref.clone().map(FieldValue::value))
            })
        }))
        .field(Field::new("source", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let s = ctx.parent_value.try_downcast_ref::<LiteraryStructure>()?;
                Ok(Some(FieldValue::value(s.source.clone())))
            })
        }));

    builder
        .register(concept_alignment)
        .register(depth_insight)
        .register(pericope_unit)
        .register(discovery_heat)
        .register(genre_section)
        .register(literary_structure)
        .register({
            Object::new("MainIdea")
                .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
                    FieldFuture::new(async move {
                        let m = ctx.parent_value.try_downcast_ref::<MainIdea>()?;
                        Ok(Some(FieldValue::value(m.id.clone())))
                    })
                }))
                .field(Field::new("book", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                    FieldFuture::new(async move {
                        let m = ctx.parent_value.try_downcast_ref::<MainIdea>()?;
                        Ok(Some(FieldValue::value(m.book.clone())))
                    })
                }))
                .field(Field::new("verseStart", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                    FieldFuture::new(async move {
                        let m = ctx.parent_value.try_downcast_ref::<MainIdea>()?;
                        Ok(Some(FieldValue::value(m.verse_start.clone())))
                    })
                }))
                .field(Field::new("verseEnd", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                    FieldFuture::new(async move {
                        let m = ctx.parent_value.try_downcast_ref::<MainIdea>()?;
                        Ok(Some(FieldValue::value(m.verse_end.clone())))
                    })
                }))
                .field(Field::new("mainIdea", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                    FieldFuture::new(async move {
                        let m = ctx.parent_value.try_downcast_ref::<MainIdea>()?;
                        Ok(Some(FieldValue::value(m.main_idea.clone())))
                    })
                }))
                .field(Field::new("series", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                    FieldFuture::new(async move {
                        let m = ctx.parent_value.try_downcast_ref::<MainIdea>()?;
                        Ok(Some(FieldValue::value(m.series.clone())))
                    })
                }))
        })
        .register({
            Object::new("LiteraryContext")
                .field(Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
                    FieldFuture::new(async move {
                        let l = ctx.parent_value.try_downcast_ref::<LiteraryContext>()?;
                        Ok(Some(FieldValue::value(l.id.clone())))
                    })
                }))
                .field(Field::new("book", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                    FieldFuture::new(async move {
                        let l = ctx.parent_value.try_downcast_ref::<LiteraryContext>()?;
                        Ok(Some(FieldValue::value(l.book.clone())))
                    })
                }))
                .field(Field::new("verseStart", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                    FieldFuture::new(async move {
                        let l = ctx.parent_value.try_downcast_ref::<LiteraryContext>()?;
                        Ok(Some(FieldValue::value(l.verse_start.clone())))
                    })
                }))
                .field(Field::new("verseEnd", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                    FieldFuture::new(async move {
                        let l = ctx.parent_value.try_downcast_ref::<LiteraryContext>()?;
                        Ok(Some(FieldValue::value(l.verse_end.clone())))
                    })
                }))
                .field(Field::new("contextProse", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                    FieldFuture::new(async move {
                        let l = ctx.parent_value.try_downcast_ref::<LiteraryContext>()?;
                        Ok(Some(FieldValue::value(l.context_prose.clone())))
                    })
                }))
                .field(Field::new("scriptureRefs", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                    FieldFuture::new(async move {
                        let l = ctx.parent_value.try_downcast_ref::<LiteraryContext>()?;
                        Ok(Some(FieldValue::value(l.scripture_refs.clone())))
                    })
                }))
                .field(Field::new("series", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                    FieldFuture::new(async move {
                        let l = ctx.parent_value.try_downcast_ref::<LiteraryContext>()?;
                        Ok(Some(FieldValue::value(l.series.clone())))
                    })
                }))
        })
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

// ── discoveryHeat resolver ──────────────────────────────────────────────────

/// Build `discoveryHeat(book: String!, chapter: Int!): [DiscoveryHeat!]!`
///
/// Returns heat scores from the `discovery_heat` materialized view for a chapter.
pub fn discovery_heat_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "discoveryHeat",
        TypeRef::named_nn_list_nn("DiscoveryHeat"),
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

                let heats = fetch_discovery_heat(conn, &book, chapter).await?;
                let values: Vec<FieldValue> = heats.into_iter().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
}

// ── genreSections resolver ──────────────────────────────────────────────────

/// Build `genreSections(book: String!, chapter: Int!): [GenreSection!]!`
pub fn genre_sections_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "genreSections",
        TypeRef::named_nn_list_nn("GenreSection"),
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
                let sections = fetch_genre_sections(conn, &book, chapter).await?;
                let values: Vec<FieldValue> = sections.into_iter().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
}

// ── literaryStructures resolver ─────────────────────────────────────────────

/// Build `literaryStructures(book: String!, chapter: Int!): [LiteraryStructure!]!`
pub fn literary_structures_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "literaryStructures",
        TypeRef::named_nn_list_nn("LiteraryStructure"),
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
                let structures = fetch_literary_structures(conn, &book, chapter).await?;
                let values: Vec<FieldValue> = structures.into_iter().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
}

// ── SQL helpers ──────────────────────────────────────────────────────────────

/// Fetch genre sections for a chapter.
async fn fetch_genre_sections(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Result<Vec<GenreSection>, async_graphql::Error> {
    let sql = r#"
SELECT verse_start, verse_end, genre, sub_type
FROM genre_sections
WHERE book = $1 AND chapter = $2
ORDER BY verse_start
"#;
    let rows = conn
        .execute(sql, &[PgValue::Text(book.to_owned()), PgValue::Int(chapter)])
        .await
        .map_err(|e| async_graphql::Error::new(format!("genre_sections query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| GenreSection {
            verse_start: int_col(&row, "verse_start"),
            verse_end:   int_col(&row, "verse_end"),
            genre:       text_col(&row, "genre"),
            sub_type:    opt_text_col(&row, "sub_type"),
        })
        .collect())
}

/// Fetch literary structures for a chapter.
async fn fetch_literary_structures(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Result<Vec<LiteraryStructure>, async_graphql::Error> {
    let sql = r#"
SELECT structure_type, title, verse_start, verse_end, pairs::text, center_ref, source
FROM literary_structures
WHERE book = $1 AND chapter_start <= $2 AND chapter_end >= $2
ORDER BY verse_start
"#;
    let rows = conn
        .execute(sql, &[PgValue::Text(book.to_owned()), PgValue::Int(chapter)])
        .await
        .map_err(|e| async_graphql::Error::new(format!("literary_structures query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| LiteraryStructure {
            structure_type: text_col(&row, "structure_type"),
            title:          opt_text_col(&row, "title"),
            verse_start:    int_col(&row, "verse_start"),
            verse_end:      int_col(&row, "verse_end"),
            pairs:          text_col(&row, "pairs"),
            center_ref:     opt_text_col(&row, "center_ref"),
            source:         text_col(&row, "source"),
        })
        .collect())
}

/// Fetch discovery heat scores for a chapter from the materialized view.
async fn fetch_discovery_heat(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Result<Vec<DiscoveryHeat>, async_graphql::Error> {
    let prefix = format!("{}.{}.", book, chapter);

    let sql = r#"
SELECT
  dh.concept_id::text,
  dh.heat,
  dh.event_count
FROM discovery_heat dh
WHERE dh.passage_ref LIKE $1
ORDER BY dh.heat DESC
"#;

    let like_pattern = format!("{}%", prefix);
    let rows = conn
        .execute(sql, &[PgValue::Text(like_pattern)])
        .await
        .map_err(|e| async_graphql::Error::new(format!("discovery_heat query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| DiscoveryHeat {
            concept_id:  text_col(&row, "concept_id"),
            heat:        float_col(&row, "heat"),
            event_count: int_col(&row, "event_count"),
        })
        .collect())
}

/// Fetch depth insights for a chapter using atomic columns + bridge tables.
async fn fetch_depth_insights(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Result<Vec<DepthInsight>, async_graphql::Error> {
    let like_pattern = format!("{}.{}.", book, chapter);

    let sql = r#"
SELECT
  di.id::text,
  di.passage_ref,
  di.insight_type,
  di.title,
  di.body,
  COALESCE(
    (SELECT array_agg(dic.concept_id::text) FROM depth_insight_concepts dic WHERE dic.insight_id = di.id),
    ARRAY[]::text[]
  ) AS related_concept_ids,
  COALESCE(
    (SELECT array_agg(dil.linked_passage_ref) FROM depth_insight_links dil WHERE dil.insight_id = di.id),
    ARRAY[]::text[]
  ) AS related_passage_refs,
  di.confidence
FROM depth_insights di
WHERE (di.book = $1 AND di.chapter = $2)
   OR EXISTS (
     SELECT 1 FROM depth_insight_links dil
     WHERE dil.insight_id = di.id
       AND dil.linked_passage_ref LIKE $3
   )
ORDER BY di.confidence DESC, di.passage_ref
"#;

    let rows = conn
        .execute(sql, &[
            PgValue::Text(book.to_owned()),
            PgValue::Int(chapter),
            PgValue::Text(format!("{}%", like_pattern)),
        ])
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
    let sql = r#"
SELECT
  pu.id::text,
  pu.pericope_title AS title,
  pu.book || '.' || pu.chapter_start || '.' || pu.verse_start AS start_ref,
  pu.book || '.' || pu.chapter_end || '.' || COALESCE(pu.verse_end, pu.verse_start) AS end_ref,
  bg.primary_genre AS genre,
  NULL::text AS structure_note,
  NULL::text AS anchor_concept_id
FROM pericope_units pu
LEFT JOIN book_genre_assignments bga ON bga.book = pu.book AND bga.is_primary = true
LEFT JOIN book_genres bg ON bg.id = bga.genre_id
WHERE pu.book = $1 AND pu.chapter_start = $2
ORDER BY pu.sort_order, pu.verse_start
"#;

    let rows = conn
        .execute(sql, &[PgValue::Text(book.to_owned()), PgValue::Int(chapter)])
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

/// Fetch enriched concept alignments for a chapter using atomic columns.
async fn fetch_concept_alignments(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Result<Vec<ConceptAlignment>, async_graphql::Error> {
    let sql = r#"
SELECT
  ca.id::text,
  ca.passage_ref,
  ca.concept_id::text,
  ca.english_span,
  ca.verse_start,
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
WHERE ca.book = $1 AND ca.chapter = $2
ORDER BY ca.verse_start, ca.english_span
"#;

    let rows = conn
        .execute(sql, &[PgValue::Text(book.to_owned()), PgValue::Int(chapter)])
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
                verse:               int_col(&row, "verse_start"),
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

// ── mainIdeas resolver ──────────────────────────────────────────────────────

/// Build `mainIdeas(book: String!, chapter: Int!): [MainIdea!]!`
pub fn main_ideas_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "mainIdeas",
        TypeRef::named_nn_list_nn("MainIdea"),
        |ctx| {
            FieldFuture::new(async move {
                let conn = ctx
                    .data_opt::<RequestConnection>()
                    .ok_or_else(|| async_graphql::Error::new("No database connection"))?;

                let book = ctx.args.try_get("book")?.string()
                    .map_err(|_| async_graphql::Error::new("book must be a string"))?.to_owned();
                let chapter = ctx.args.try_get("chapter")?.i64()
                    .map_err(|_| async_graphql::Error::new("chapter must be an int"))?;

                let ideas = fetch_main_ideas(conn, &book, chapter).await?;
                let values: Vec<FieldValue> = ideas.into_iter().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
}

/// Build `literaryContext(book: String!, chapter: Int!): [LiteraryContext!]!`
pub fn literary_context_field(_executor: Arc<QueryExecutor>) -> Field {
    Field::new(
        "literaryContext",
        TypeRef::named_nn_list_nn("LiteraryContext"),
        |ctx| {
            FieldFuture::new(async move {
                let conn = ctx
                    .data_opt::<RequestConnection>()
                    .ok_or_else(|| async_graphql::Error::new("No database connection"))?;

                let book = ctx.args.try_get("book")?.string()
                    .map_err(|_| async_graphql::Error::new("book must be a string"))?.to_owned();
                let chapter = ctx.args.try_get("chapter")?.i64()
                    .map_err(|_| async_graphql::Error::new("chapter must be an int"))?;

                let contexts = fetch_literary_context(conn, &book, chapter).await?;
                let values: Vec<FieldValue> = contexts.into_iter().map(FieldValue::owned_any).collect();
                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new("book",    TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("chapter", TypeRef::named_nn(TypeRef::INT)))
}

async fn fetch_main_ideas(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Result<Vec<MainIdea>, async_graphql::Error> {
    let ch_prefix = format!("{}.", chapter);
    let sql = r#"
SELECT id::text, book, verse_start, verse_end, main_idea, series
FROM commentary_main_ideas
WHERE book = $1 AND (verse_start LIKE $2 OR verse_start LIKE $3)
ORDER BY verse_start
"#;
    let rows = conn
        .execute(sql, &[
            PgValue::Text(book.to_owned()),
            PgValue::Text(format!("{}%", ch_prefix)),
            PgValue::Text(format!("{}.%", chapter)),
        ])
        .await
        .map_err(|e| async_graphql::Error::new(format!("main_ideas query failed: {e}")))?;

    Ok(rows.into_iter().map(|row| MainIdea {
        id:          text_col(&row, "id"),
        book:        text_col(&row, "book"),
        verse_start: text_col(&row, "verse_start"),
        verse_end:   text_col(&row, "verse_end"),
        main_idea:   text_col(&row, "main_idea"),
        series:      text_col(&row, "series"),
    }).collect())
}

async fn fetch_literary_context(
    conn: &RequestConnection,
    book: &str,
    chapter: i64,
) -> Result<Vec<LiteraryContext>, async_graphql::Error> {
    let ch_prefix = format!("{}.", chapter);
    let sql = r#"
SELECT id::text, book, verse_start, verse_end, context_prose::text, scripture_refs::text, series
FROM commentary_literary_context
WHERE book = $1 AND (verse_start LIKE $2 OR verse_start LIKE $3)
ORDER BY verse_start
"#;
    let rows = conn
        .execute(sql, &[
            PgValue::Text(book.to_owned()),
            PgValue::Text(format!("{}%", ch_prefix)),
            PgValue::Text(format!("{}.%", chapter)),
        ])
        .await
        .map_err(|e| async_graphql::Error::new(format!("literary_context query failed: {e}")))?;

    Ok(rows.into_iter().map(|row| LiteraryContext {
        id:              text_col(&row, "id"),
        book:            text_col(&row, "book"),
        verse_start:     text_col(&row, "verse_start"),
        verse_end:       text_col(&row, "verse_end"),
        context_prose:   text_col(&row, "context_prose"),
        scripture_refs:  text_col(&row, "scripture_refs"),
        series:          text_col(&row, "series"),
    }).collect())
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
