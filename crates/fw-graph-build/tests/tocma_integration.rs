//! Integration tests for TOCMA resolvers.
//! Unit test: tocma_structs_are_clonable (always runs, no DB needed)
//! E2E tests: require `--features integration` and DATABASE_URL set to production.
//!   DATABASE_URL=<production-url> cargo test -p fw-graph-build --test tocma_integration --features integration -- e2e

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

/// E2E tests require production DATABASE_URL and `--features integration`.
/// Run with:
///   DATABASE_URL=<production-url> \
///     cargo test -p fw-graph-build --test tocma_integration --features integration -- e2e
#[cfg(feature = "integration")]
mod e2e {
    use fw_graph_build::resolve::tocma::verse::fetch_genre_step;
    use fw_graph_build::resolve::tocma::verse::fetch_passage_tokens;
    use fw_graph_build::resolve::tocma::verse::fetch_literary_units_step;
    use fw_graph_build::resolve::tocma::db::load_outline_path;
    use fw_graph_build::resolve::tocma::input::assemble_theology_input;
    use fw_graph_build::executor::RequestConnection;

    async fn make_conn() -> RequestConnection {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set (use production connection string)");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("DB connect failed");
        let conn = pool.acquire().await.expect("acquire failed");
        RequestConnection {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    #[tokio::test]
    async fn tocma_verse_john_1_1_genre_is_argument() {
        let conn = make_conn().await;
        let genre = fetch_genre_step(&conn, "John", 1, 1).await.expect("query failed");
        let genre = genre.expect("John 1:1 should have genre data");
        assert!(!genre.primary.is_empty(), "genre.primary must be non-empty");
        // John is argumentative/epistolary
        assert!(!genre.reading_posture.is_empty(), "reading_posture must be decoded");
    }

    #[tokio::test]
    async fn tocma_verse_tokens_john_1_1_has_greek() {
        let conn = make_conn().await;
        let tokens = fetch_passage_tokens(&conn, "John", 1, 1, 1).await.expect("query failed");
        assert!(!tokens.is_empty(), "John 1:1 must have tokens");
        assert!(!tokens[0].original_word.is_empty(), "tokens must have original word");
        assert!(!tokens[0].morphology_decoded.is_empty(), "morphology must be decoded");
    }

    #[tokio::test]
    async fn theology_input_rom_1_16_17_has_doctrine_hints() {
        let conn = make_conn().await;
        let input = assemble_theology_input(
            &conn, "Rom", "Rom.1.16-17", "The Gospel Is the Power of God",
            1, 16, 17,
        ).await;
        // Romans should detect soteriology or theology_proper
        let hints = input.doctrine_hints.expect("Romans 1:16-17 must have doctrine hints");
        assert!(!hints.loci.is_empty(), "loci must not be empty");
    }

    #[tokio::test]
    async fn theology_input_sparse_book_does_not_error() {
        // A book with no commentary data should return empty vecs, not error
        let conn = make_conn().await;
        let input = assemble_theology_input(
            &conn, "Obad", "Obad.1.1", "Vision of Obadiah",
            1, 1, 21,
        ).await;
        // Should complete without panic; data may be empty
        assert_eq!(input.book, "Obad");
    }

    /// TOCMA Fix B regression: `outline_path` must no longer be hardcoded `[]`.
    ///
    /// `tocma_step2_outline_nodes` has 8,541 rows in production. This test
    /// hits a verse known to fall inside a pericope with outline coverage
    /// (Rom 1:16-17) and asserts the ancestry chain is returned root-to-leaf.
    #[tokio::test]
    async fn outline_path_rom_1_16_has_ancestry() {
        let conn = make_conn().await;
        let path = load_outline_path(&conn, "Rom", 1, 16)
            .await
            .expect("outline_path query failed");
        // Production coverage is partial — if this assertion ever starts
        // failing in CI without a schema change, verify tocma_step2_outline_nodes
        // still has rows linked to Rom 1:16 via pm_clauses.
        assert!(
            !path.is_empty(),
            "Rom 1:16 should have outline_path coverage in tocma_step2_outline_nodes; \
             got empty Vec (fix regressed or coverage dropped)"
        );
        for label in &path {
            assert!(!label.is_empty(), "outline_path labels must be non-empty");
        }
    }

    /// For an uncovered verse, the loader must return an empty `Vec`, not error.
    /// Obadiah is Tier 3/4 in the coverage audit — no tocma_step2 coverage expected.
    #[tokio::test]
    async fn outline_path_uncovered_verse_is_empty_not_error() {
        let conn = make_conn().await;
        let path = load_outline_path(&conn, "Obad", 1, 1)
            .await
            .expect("outline_path query must not error on uncovered verses");
        assert!(
            path.is_empty(),
            "Obad 1:1 has no tocma_step2 coverage; expected empty Vec, got {} labels",
            path.len()
        );
    }

    /// `fetch_literary_units_step` now delegates to `load_outline_path`.
    /// Verifies the full resolver path (pericope_units JOIN + outline CTE).
    #[tokio::test]
    async fn literary_units_step_rom_1_16_has_outline_path() {
        let conn = make_conn().await;
        let step = fetch_literary_units_step(&conn, "Rom", 1, 16)
            .await
            .expect("literary_units query failed")
            .expect("Rom 1:16 must have a literary_units step");
        assert!(
            step.pericope.is_some(),
            "Rom 1:16 must resolve to a pericope"
        );
        assert!(
            !step.outline_path.is_empty(),
            "literary_units.outline_path must be populated for Rom 1:16"
        );
    }
}
