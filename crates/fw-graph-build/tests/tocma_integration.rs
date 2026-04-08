//! Integration tests for TOCMA resolvers.
//! Run unit tests normally. DB tests require `--features integration` and
//! DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:54322/postgres

#[test]
fn tocma_structs_are_clonable() {
    use fw_graph_build::resolve::tocma::structs::*;

    let token = PassageToken {
        verse: 1,
        position: 1,
        original_word: "בְּרֵאשִׁית".to_string(),
        transliteration: "bereshit".to_string(),
        morphology_code: "Vqp3ms".to_string(),
        morphology_decoded: String::new(),
        morphology_plain: String::new(),
        fellwork_gloss_english: String::new(),
        fellwork_gloss_prefix: None,
        fellwork_gloss_subject: None,
        fellwork_gloss_core: String::new(),
        fellwork_gloss_suffix: None,
        fellwork_gloss_source: String::new(),
        significance: 0.0,
        louw_nida_domain: None,
        louw_nida_domain_name: None,
    };
    let _ = token.clone();

    let arc = ArcLink {
        r#ref: "Gen.1.1".to_string(),
        link_type: "lexical_echo".to_string(),
        link_type_explained: String::new(),
        direction: String::new(),
        shared_lemma: None,
        concept: None,
    };
    let _ = arc.clone();

    let doctrine = DoctrineEntry {
        locus: "christology".to_string(),
        label: "Christology".to_string(),
        explained: "The doctrine of Christ".to_string(),
    };
    let _ = doctrine.clone();
}
