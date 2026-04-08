//! Output structs for all 12 TOCMA steps.
//! All fields are public. Options/Vecs handle missing data gracefully.

// ── Shared ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PericopeInfoTocma {
    pub r#ref: String,
    pub title: String,
}

#[derive(Clone)]
pub struct FoundLexiconRef {
    pub raw: String,
    pub abbreviation: String,
    pub full_title: String,
    pub page_or_section: Option<String>,
    pub description: String,
}

#[derive(Clone)]
pub struct DoctrineEntry {
    pub locus: String,
    pub label: String,
    pub explained: String,
}

#[derive(Clone)]
pub struct ArcLink {
    pub r#ref: String,
    pub link_type: String,
    pub link_type_explained: String,
    pub direction: String,
    pub shared_lemma: Option<String>,
    pub concept: Option<String>,
}

#[derive(Clone)]
pub struct LiteraryStructureItem {
    pub structure_type: String,
    pub title: String,
    pub center_ref: Option<String>,
    pub source: String,
}

// ── Verse Card (Steps 1-5) ───────────────────────────────────────────────────

#[derive(Clone)]
pub struct GenreStep {
    pub primary: String,
    pub sub_type: Option<String>,
    pub reading_posture: String,
    pub genre_context: String,
}

#[derive(Clone)]
pub struct LiteraryUnitsStep {
    pub pericope: Option<PericopeInfoTocma>,
    pub outline_path: Vec<String>,
    pub clause_depth: Option<i64>,
}

#[derive(Clone)]
pub struct DecodedWitness {
    pub siglum: String,
    pub full_name: String,
    pub tradition: String,
    pub language: String,
    pub date_range: Option<String>,
}

#[derive(Clone)]
pub struct ApparatusEntry {
    pub note_text: String,
    pub witnesses: Vec<DecodedWitness>,
    pub operation: Option<String>,
    pub source: String,
    pub source_decoded: String,
}

#[derive(Clone)]
pub struct MasorahEntry {
    pub mp_note_text: String,
    pub decoded: String,
    pub is_hapax: bool,
    pub word: Option<String>,
}

#[derive(Clone)]
pub struct TranslationDivergence {
    pub position: i64,
    pub readings: Vec<DivergenceReading>,
    pub divergence_type: Option<String>,
}

#[derive(Clone)]
pub struct DivergenceReading {
    pub version: String,
    pub text: String,
}

#[derive(Clone)]
pub struct TranslationStep {
    pub bsb: Option<String>,
    pub kjv: Option<String>,
    pub original: Option<String>,
    pub pointed: Option<String>,
    pub phrased_text: Option<String>,
    pub phrased_indent: Option<i64>,
    pub divergences: Vec<TranslationDivergence>,
}

#[derive(Clone)]
pub struct DecodedRoleLabel {
    pub label: String,
    pub explained: String,
    pub genre: Option<String>,
}

#[derive(Clone)]
pub struct PassageToken {
    pub verse: i64,
    pub position: i64,
    pub original_word: String,
    pub transliteration: String,
    pub morphology_code: String,
    pub morphology_decoded: String,
    pub morphology_plain: String,
    pub fellwork_gloss_english: String,
    pub fellwork_gloss_prefix: Option<String>,
    pub fellwork_gloss_subject: Option<String>,
    pub fellwork_gloss_core: String,
    pub fellwork_gloss_suffix: Option<String>,
    pub fellwork_gloss_source: String,
    pub significance: f32,
    pub louw_nida_domain: Option<String>,
    pub louw_nida_domain_name: Option<String>,
}

// ── Pericope Card (Steps 6-12) ───────────────────────────────────────────────

#[derive(Clone)]
pub struct DiscourseNote {
    pub note: String,
    pub series: String,
}

#[derive(Clone)]
pub struct SeriesStudy {
    pub series: String,
    pub content: String,
    pub lexicon_refs: Vec<FoundLexiconRef>,
}

#[derive(Clone)]
pub struct WordStudyEntry {
    pub lemma: String,
    pub transliteration: String,
    pub strong: String,
    pub gloss: String,
    pub significance: f32,
    pub louw_nida_domain: Option<String>,
    pub louw_nida_domain_name: Option<String>,
    pub studies: Vec<SeriesStudy>,
    pub lexicon_refs: Vec<FoundLexiconRef>,
}

#[derive(Clone)]
pub struct CommentaryNote {
    pub content: String,
    pub series: String,
}

#[derive(Clone)]
pub struct EntityRef {
    pub name: String,
    pub entity_type: String,
    pub role: String,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

#[derive(Clone)]
pub struct ThemeArc {
    pub theme: String,
    pub arc: Vec<ArcLink>,
}

#[derive(Clone)]
pub struct ApplicationEntry {
    pub content: String,
    pub series: String,
    pub lexicon_refs: Vec<FoundLexiconRef>,
}

#[derive(Clone)]
pub struct DepthInsightSummary {
    pub title: String,
    pub body: String,
    pub insight_type: String,
    pub confidence: f64,
}

// ── TheologyInput (Spec 2 entry point) ──────────────────────────────────────

#[derive(Clone)]
pub struct LiteraryPosition {
    pub outline_path: Vec<String>,
    pub preceding_pericope: Option<PericopeInfoTocma>,
    pub following_pericope: Option<PericopeInfoTocma>,
}

#[derive(Clone)]
pub struct ExistingSynthesis {
    pub biblical_theology: Option<String>,
    pub systematic_theology: Option<String>,
    pub practical_theology: Option<String>,
}
