//! Integration tests for fw-graph-build against a real Postgres database.
//!
//! Requires a running local Supabase:
//!   DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:54322/postgres
//!
//! Run with: cargo test -p fw-graph-build --test integration
//! (These tests are NOT ignored — they run if the DB is reachable.)

use fw_graph_build::{gather, build_schema, BehaviorSet, GatherOutput, ResourceKind};
use fw_graph_dataplan::PgResourceRegistry;
use fw_graph_introspect::{introspect, IntrospectionResult};

const DATABASE_URL: &str = "postgresql://postgres:postgres@127.0.0.1:54322/postgres";

async fn connect() -> Option<sqlx::PgPool> {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(3))
        .connect(DATABASE_URL)
        .await
        .ok()
}

async fn setup() -> Option<(sqlx::PgPool, IntrospectionResult, GatherOutput)> {
    let pool = connect().await?;
    let introspection = introspect(&pool, &["public"]).await.ok()?;
    let registry = PgResourceRegistry::from_introspection(&introspection);
    let preset = fw_graph_config::Preset {
        pg_schemas: vec!["public".into()],
        ..fw_graph_config::Preset::default()
    };
    let output = gather(&introspection, &registry, &preset).ok()?;
    Some((pool, introspection, output))
}

// ─── Introspection Tests ────────────────────────────────────────

#[tokio::test]
async fn test_introspect_local_supabase() {
    let pool = match connect().await {
        Some(p) => p,
        None => {
            eprintln!("SKIP: local Supabase not reachable at {DATABASE_URL}");
            return;
        }
    };

    let result = introspect(&pool, &["public"]).await.unwrap();

    // Should find a substantial number of tables
    let table_count = result.classes.len();
    println!("Introspected {table_count} classes (tables/views)");
    assert!(table_count > 50, "Expected 50+ tables, got {table_count}");

    // Should find key tables
    let table_names: Vec<&str> = result.classes.iter().map(|c| c.name.as_str()).collect();
    assert!(table_names.contains(&"concepts"), "Missing 'concepts' table");
    assert!(table_names.contains(&"passages"), "Missing 'passages' table");
    assert!(table_names.contains(&"passage_tokens"), "Missing 'passage_tokens' table");
    assert!(table_names.contains(&"phrased_blocks"), "Missing 'phrased_blocks' table");
    assert!(table_names.contains(&"vocabulary_items"), "Missing 'vocabulary_items' table");

    // Should find attributes for concepts table
    let concepts_class = result.classes.iter().find(|c| c.name == "concepts").unwrap();
    let concepts_attrs: Vec<&str> = result
        .attributes
        .iter()
        .filter(|a| a.class_oid == concepts_class.oid && a.num > 0)
        .map(|a| a.name.as_str())
        .collect();
    println!("concepts columns: {concepts_attrs:?}");
    assert!(concepts_attrs.contains(&"id"), "concepts missing 'id' column");
    assert!(concepts_attrs.contains(&"lemma"), "concepts missing 'lemma' column");

    // Should find constraints (PKs, FKs)
    assert!(!result.constraints.is_empty(), "Should have constraints");
    let pk_count = result
        .constraints
        .iter()
        .filter(|c| c.kind == fw_graph_introspect::PgConstraintKind::PrimaryKey)
        .count();
    println!("Found {pk_count} primary keys");
    assert!(pk_count > 30, "Expected 30+ primary keys, got {pk_count}");

    // Should find types
    assert!(!result.types.is_empty(), "Should have types");
    println!(
        "Introspection summary: {} classes, {} attributes, {} constraints, {} types",
        result.classes.len(),
        result.attributes.len(),
        result.constraints.len(),
        result.types.len()
    );
}

// ─── Gather Tests ───────────────────────────────────────────────

#[tokio::test]
async fn test_gather_from_local_supabase() {
    let (_pool, _intro, output) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("SKIP: local Supabase not reachable");
            return;
        }
    };

    println!("Gathered {} resources", output.resources.len());
    assert!(
        output.resources.len() > 50,
        "Expected 50+ resources, got {}",
        output.resources.len()
    );

    // Check key resources exist with correct names
    let resource_names: Vec<&str> = output.resources.iter().map(|r| r.name.as_str()).collect();
    println!("Sample resources: {:?}", &resource_names[..10.min(resource_names.len())]);

    assert!(
        resource_names.contains(&"Concept"),
        "Missing 'Concept' resource (from 'concepts' table)"
    );
    assert!(
        resource_names.contains(&"Passage"),
        "Missing 'Passage' resource (from 'passages' table)"
    );
    assert!(
        resource_names.contains(&"PassageToken"),
        "Missing 'PassageToken' resource (from 'passage_tokens' table)"
    );
    assert!(
        resource_names.contains(&"VocabularyItem"),
        "Missing 'VocabularyItem' resource (from 'vocabulary_items' table)"
    );

    // Check a resource has correct structure
    let concept = output.resources.iter().find(|r| r.name == "Concept").unwrap();
    assert_eq!(concept.table, "concepts");
    assert_eq!(concept.schema, "public");
    assert_eq!(concept.kind, ResourceKind::Table);
    assert!(!concept.columns.is_empty(), "Concept should have columns");
    assert!(!concept.primary_key.is_empty(), "Concept should have a PK");
    println!(
        "Concept: {} columns, PK: {:?}",
        concept.columns.len(),
        concept.primary_key
    );

    // Check columns have correct GQL type mappings
    let id_col = concept.columns.iter().find(|c| c.pg_name == "id");
    if let Some(col) = id_col {
        println!("Concept.id: gql_name={}, gql_type={}", col.gql_name, col.gql_type);
        assert_eq!(col.gql_name, "id");
        assert!(col.is_not_null, "id should be NOT NULL");
    }

    // Check relations exist
    println!("Found {} relations", output.relations.len());
    assert!(
        output.relations.len() > 5,
        "Expected 5+ FK relations, got {}",
        output.relations.len()
    );

    // Check behaviors
    let concept_behavior = output.behaviors.get("Concept");
    assert!(concept_behavior.is_some(), "Concept should have behaviors");
    let b = concept_behavior.unwrap();
    assert!(b.has(BehaviorSet::CONNECTION), "Concept should have CONNECTION");
    assert!(b.has(BehaviorSet::SELECT_ONE), "Concept should have SELECT_ONE");
}

// ─── Schema Build Tests ─────────────────────────────────────────

#[tokio::test]
async fn test_build_schema_from_local_supabase() {
    let (pool, _intro, output) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("SKIP: local Supabase not reachable");
            return;
        }
    };

    let schema = build_schema(&output, &output.behaviors, pool.clone());
    assert!(schema.is_ok(), "Schema build failed: {:?}", schema.err());

    let schema = schema.unwrap();
    let sdl = schema.sdl();

    println!("Schema SDL length: {} chars", sdl.len());
    assert!(sdl.len() > 1000, "SDL should be substantial");

    // Core types exist
    assert!(sdl.contains("type Concept"), "SDL missing 'type Concept'");
    assert!(sdl.contains("type Passage"), "SDL missing 'type Passage'");
    assert!(sdl.contains("type PassageToken"), "SDL missing 'type PassageToken'");
    assert!(sdl.contains("type VocabularyItem"), "SDL missing 'type VocabularyItem'");
    assert!(sdl.contains("type PhrasedBlock"), "SDL missing 'type PhrasedBlock'");

    // Connection types
    assert!(sdl.contains("type ConceptsConnection"), "SDL missing ConceptsConnection");
    assert!(sdl.contains("type PassagesConnection"), "SDL missing PassagesConnection");

    // Query fields
    assert!(sdl.contains("allConcepts"), "SDL missing allConcepts query");
    assert!(sdl.contains("allPassages"), "SDL missing allPassages query");

    // Mutation fields
    assert!(sdl.contains("createConcept"), "SDL missing createConcept mutation");
    assert!(sdl.contains("updateConcept"), "SDL missing updateConcept mutation");
    assert!(sdl.contains("deleteConcept"), "SDL missing deleteConcept mutation");

    // Relay types
    assert!(sdl.contains("type PageInfo"), "SDL missing PageInfo");
    assert!(sdl.contains("interface Node"), "SDL missing Node interface");

    // Filter types
    assert!(sdl.contains("ConceptFilter"), "SDL missing ConceptFilter");

    // OrderBy types
    assert!(sdl.contains("ConceptsOrderBy"), "SDL missing ConceptsOrderBy");

    // Print a sample of the SDL for manual inspection
    let first_500: String = sdl.chars().take(500).collect();
    println!("SDL preview:\n{first_500}...");
}

#[tokio::test]
async fn test_schema_sdl_stats() {
    let (pool, _intro, output) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("SKIP: local Supabase not reachable");
            return;
        }
    };

    let schema = build_schema(&output, &output.behaviors, pool.clone()).unwrap();
    let sdl = schema.sdl();

    // Count types, queries, mutations
    let type_count = sdl.matches("\ntype ").count();
    let input_count = sdl.matches("\ninput ").count();
    let enum_count = sdl.matches("\nenum ").count();
    let interface_count = sdl.matches("\ninterface ").count();

    println!("Schema statistics:");
    println!("  Object types:  {type_count}");
    println!("  Input types:   {input_count}");
    println!("  Enum types:    {enum_count}");
    println!("  Interfaces:    {interface_count}");
    println!("  SDL size:      {} KB", sdl.len() / 1024);

    // With 160 tables, we should have a substantial schema
    assert!(type_count > 100, "Expected 100+ object types, got {type_count}");
    assert!(input_count > 50, "Expected 50+ input types, got {input_count}");
}

// ─── GraphQL Execution Test ─────────────────────────────────────

#[tokio::test]
async fn test_execute_introspection_query() {
    let (pool, _intro, output) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("SKIP: local Supabase not reachable");
            return;
        }
    };

    let schema = build_schema(&output, &output.behaviors, pool.clone()).unwrap();

    // Execute a standard introspection query
    let query = r#"{ __schema { queryType { name } mutationType { name } types { name } } }"#;
    let request = async_graphql::Request::new(query);
    let response = schema.execute(request).await;

    assert!(
        response.errors.is_empty(),
        "Introspection query returned errors: {:?}",
        response.errors
    );

    let data = response.data;
    let json = serde_json::to_value(&data).unwrap_or_default();
    println!("Introspection response (truncated): {}...",
        &serde_json::to_string(&json).unwrap_or_default()[..500.min(serde_json::to_string(&json).unwrap_or_default().len())]
    );

    // Verify we got data back
    assert!(json.is_object(), "Response data should be an object");
}
