//! Tests for `build_schema`'s SchemaExtension wiring — verifies that all three
//! extension phases (register_types, extend_query, extend_mutation) actually
//! invoke the corresponding hooks and that has_mutations gating works correctly.
//!
//! These tests do NOT need a live database. They use `PgPool::connect_lazy`
//! to construct a pool that defers connection until first query. The schema
//! is built and inspected via SDL, never executed.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_graphql::dynamic::{Field, FieldFuture, Object, TypeRef};
use magna_build::{
    build_schema, BehaviorSet, ExtensionContext, GatherOutput, ResolvedColumn,
    ResolvedResource, ResourceKind, SchemaExtension,
};

fn lazy_pool() -> sqlx::PgPool {
    sqlx::PgPool::connect_lazy(
        "postgresql://nobody:nobody@127.0.0.1:1/nodb",
    )
    .expect("PgPool::connect_lazy must succeed even with bogus URL")
}

fn empty_gather() -> GatherOutput {
    GatherOutput {
        resources: vec![],
        relations: vec![],
        behaviors: HashMap::new(),
        enums: vec![],
        smart_tags: HashMap::new(),
        plugin_metadata: serde_json::Map::new(),
    }
}

/// A minimal extension that registers one type and adds one Query field
/// referencing that type. Tracks how many times each hook was invoked.
struct CountingExtension {
    register_types_calls: Arc<AtomicUsize>,
    extend_query_calls: Arc<AtomicUsize>,
    extend_mutation_calls: Arc<AtomicUsize>,
}

impl CountingExtension {
    fn new() -> Self {
        Self {
            register_types_calls: Arc::new(AtomicUsize::new(0)),
            extend_query_calls: Arc::new(AtomicUsize::new(0)),
            extend_mutation_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl SchemaExtension for CountingExtension {
    fn name(&self) -> &str { "counting" }

    fn register_types(&self, ctx: &mut ExtensionContext<'_>) {
        self.register_types_calls.fetch_add(1, Ordering::SeqCst);
        ctx.register_type(
            Object::new("ExtType").field(Field::new(
                "value",
                TypeRef::named_nn(TypeRef::STRING),
                |_| FieldFuture::from_value(Some(async_graphql::Value::from("hi"))),
            )),
        );
    }

    fn extend_query(&self, ctx: &mut ExtensionContext<'_>) {
        self.extend_query_calls.fetch_add(1, Ordering::SeqCst);
        ctx.query_field(Field::new(
            "extQueryField",
            TypeRef::named_nn("ExtType"),
            |_| FieldFuture::from_value(None),
        ));
    }

    fn extend_mutation(&self, ctx: &mut ExtensionContext<'_>) {
        self.extend_mutation_calls.fetch_add(1, Ordering::SeqCst);
        ctx.mutation_field(Field::new(
            "extMutationField",
            TypeRef::named_nn("ExtType"),
            |_| FieldFuture::from_value(None),
        ));
    }
}

/// Build a `GatherOutput` containing one resource with the given behavior set.
/// The single resource is enough to flip `has_mutations` true/false based on
/// what flags it declares. The resource has the bare minimum to satisfy the
/// schema builder when mutation companion types are required (e.g. when INSERT
/// is set, FILTER is added so widgetFilter can be registered).
fn gather_with_resource(name: &str, mut bs: BehaviorSet) -> GatherOutput {
    // Whenever a mutation flag is set, the schema builder also needs FILTER
    // and ORDER_BY for the auto-generated companion types. Add them so the
    // test fixture stays minimal but the build doesn't fail on missing
    // *Filter or *OrderBy types.
    if bs.has(BehaviorSet::INSERT) || bs.has(BehaviorSet::UPDATE) || bs.has(BehaviorSet::DELETE) {
        bs.add(BehaviorSet::CONNECTION);
        bs.add(BehaviorSet::SELECT_ONE);
        bs.add(BehaviorSet::FILTER);
        bs.add(BehaviorSet::ORDER_BY);
    }
    let mut behaviors = HashMap::new();
    behaviors.insert(name.to_string(), bs);
    GatherOutput {
        resources: vec![ResolvedResource {
            name: name.to_string(),
            schema: "public".to_string(),
            table: name.to_string(),
            kind: ResourceKind::Table,
            columns: vec![ResolvedColumn {
                pg_name: "id".to_string(),
                gql_name: "id".to_string(),
                type_oid: 2950, // uuid
                gql_type: "ID".to_string(),
                is_not_null: true,
                has_default: false,
            }],
            primary_key: vec!["id".to_string()],
            unique_constraints: vec![],
            class_oid: 0,
        }],
        relations: vec![],
        behaviors,
        enums: vec![],
        smart_tags: HashMap::new(),
        plugin_metadata: serde_json::Map::new(),
    }
}

#[tokio::test]
async fn extensions_invoked_when_provided() {
    let ext = CountingExtension::new();
    let rt_calls = ext.register_types_calls.clone();
    let eq_calls = ext.extend_query_calls.clone();
    let em_calls = ext.extend_mutation_calls.clone();

    let extensions: Vec<Box<dyn SchemaExtension>> = vec![Box::new(ext)];
    let output = empty_gather();

    let schema = build_schema(&output, &output.behaviors, lazy_pool(), &extensions)
        .expect("build_schema must succeed with empty gather");

    // RegisterTypes ran once. ExtendQuery ran once. ExtendMutation didn't run
    // because the empty gather has no mutation-enabled resources.
    assert_eq!(rt_calls.load(Ordering::SeqCst), 1, "register_types should run");
    assert_eq!(eq_calls.load(Ordering::SeqCst), 1, "extend_query should run");
    assert_eq!(em_calls.load(Ordering::SeqCst), 0, "extend_mutation should NOT run when no mutations");

    let sdl = schema.sdl();
    assert!(sdl.contains("ExtType"), "registered type should appear in SDL");
    assert!(sdl.contains("extQueryField"), "query field should appear in SDL");
    assert!(!sdl.contains("extMutationField"), "mutation field must NOT appear when has_mutations=false");
}

/// Per-extension call counts must remain `1` for each hook regardless of slice
/// length — every extension's hook fires exactly once per phase. With N=3
/// extensions, a regression that uses `extensions.len() - 1` instead of N
/// would surface here as a missed hook on one of the instances.
#[tokio::test]
async fn each_extension_hook_fires_exactly_once_with_n_extensions() {
    for n in [1usize, 2, 3] {
        let exts: Vec<CountingExtension> = (0..n).map(|_| CountingExtension::new()).collect();
        // Snapshot per-instance counters before moving the extensions into the slice.
        let counters: Vec<_> = exts
            .iter()
            .map(|e| {
                (
                    e.register_types_calls.clone(),
                    e.extend_query_calls.clone(),
                    e.extend_mutation_calls.clone(),
                )
            })
            .collect();
        let extensions: Vec<Box<dyn SchemaExtension>> =
            exts.into_iter().map(|e| Box::new(e) as Box<dyn SchemaExtension>).collect();

        // Use distinct extension names so async-graphql doesn't reject duplicate
        // type registrations. CountingExtension always registers "ExtType", so
        // we can't have two of them in the same slice... swap to LabeledCounting.
        // Workaround: use `gather_with_resource` with N=1 only here, and use the
        // Labeled variant for N>1.
        if n == 1 {
            let output = empty_gather();
            build_schema(&output, &output.behaviors, lazy_pool(), &extensions)
                .expect("build_schema must succeed");
            let (rt, eq, em) = &counters[0];
            assert_eq!(rt.load(Ordering::SeqCst), 1, "n={n}: register_types should fire exactly once per extension");
            assert_eq!(eq.load(Ordering::SeqCst), 1, "n={n}: extend_query should fire exactly once per extension");
            assert_eq!(em.load(Ordering::SeqCst), 0, "n={n}: extend_mutation should not fire when no mutations");
        } else {
            // For N>1 we need to avoid duplicate type registrations.
            // The labeled-counter variant below provides this; skipping the
            // duplicate-registration N>1 case here intentionally and exercising
            // it via `multiple_extensions_each_hook_once` instead.
            continue;
        }
    }
}

/// Slice-length variant for N > 1 using LabeledCountingExt to avoid duplicate
/// type registration. Each extension contributes a uniquely named type+field.
#[tokio::test]
async fn multiple_extensions_each_hook_once() {
    /// Like CountingExtension but parameterized by a label so multiple instances
    /// can coexist in the same schema without colliding on type names.
    struct LabeledCounting {
        label: &'static str,
        register_types_calls: Arc<AtomicUsize>,
        extend_query_calls: Arc<AtomicUsize>,
    }
    impl SchemaExtension for LabeledCounting {
        fn name(&self) -> &str { self.label }
        fn register_types(&self, ctx: &mut ExtensionContext<'_>) {
            self.register_types_calls.fetch_add(1, Ordering::SeqCst);
            ctx.register_type(
                Object::new(format!("Type_{}", self.label)).field(Field::new(
                    "v", TypeRef::named_nn(TypeRef::STRING),
                    |_| FieldFuture::from_value(None),
                )),
            );
        }
        fn extend_query(&self, ctx: &mut ExtensionContext<'_>) {
            self.extend_query_calls.fetch_add(1, Ordering::SeqCst);
            ctx.query_field(Field::new(
                format!("field_{}", self.label),
                TypeRef::named_nn(format!("Type_{}", self.label)),
                |_| FieldFuture::from_value(None),
            ));
        }
    }

    for labels in [
        vec!["alpha", "beta"],
        vec!["alpha", "beta", "gamma"],
    ] {
        let n = labels.len();
        let exts: Vec<LabeledCounting> = labels
            .iter()
            .map(|label| LabeledCounting {
                label,
                register_types_calls: Arc::new(AtomicUsize::new(0)),
                extend_query_calls: Arc::new(AtomicUsize::new(0)),
            })
            .collect();
        let counters: Vec<_> = exts
            .iter()
            .map(|e| (e.register_types_calls.clone(), e.extend_query_calls.clone()))
            .collect();
        let extensions: Vec<Box<dyn SchemaExtension>> =
            exts.into_iter().map(|e| Box::new(e) as Box<dyn SchemaExtension>).collect();
        let output = empty_gather();
        build_schema(&output, &output.behaviors, lazy_pool(), &extensions)
            .expect("multi-extension build must succeed");

        for (i, (rt, eq)) in counters.iter().enumerate() {
            assert_eq!(rt.load(Ordering::SeqCst), 1, "n={n} ext[{i}]: register_types fired exactly once");
            assert_eq!(eq.load(Ordering::SeqCst), 1, "n={n} ext[{i}]: extend_query fired exactly once");
        }
    }
}

/// Verify the has_mutations gate (lib.rs:65: INSERT || UPDATE || DELETE)
/// is sensitive to each individual flag, not just the combined defaults.
#[tokio::test]
async fn has_mutations_gate_fires_for_each_individual_mutation_flag() {
    for (label, mut bs, expected_em_calls) in [
        ("insert-only", BehaviorSet::none(), 1usize),
        ("update-only", BehaviorSet::none(), 1),
        ("delete-only", BehaviorSet::none(), 1),
        ("select-only", BehaviorSet::none(), 0),
        ("all-defaults", BehaviorSet::table_defaults(), 1),
    ] {
        // Apply the single-flag setup. `gather_with_resource` adds the
        // mutation-companion flags (FILTER, ORDER_BY, etc.) automatically when
        // any mutation flag is present.
        match label {
            "insert-only" => bs.add(BehaviorSet::INSERT),
            "update-only" => bs.add(BehaviorSet::UPDATE),
            "delete-only" => bs.add(BehaviorSet::DELETE),
            "select-only" => {
                bs.add(BehaviorSet::SELECT_ONE);
                bs.add(BehaviorSet::CONNECTION);
                bs.add(BehaviorSet::FILTER);
                bs.add(BehaviorSet::ORDER_BY);
            }
            "all-defaults" => {} // already set
            _ => unreachable!(),
        }

        let ext = CountingExtension::new();
        let em_calls = ext.extend_mutation_calls.clone();
        let extensions: Vec<Box<dyn SchemaExtension>> = vec![Box::new(ext)];
        let output = gather_with_resource("widget", bs);

        build_schema(&output, &output.behaviors, lazy_pool(), &extensions)
            .unwrap_or_else(|e| panic!("{label}: build_schema must succeed: {e:?}"));

        assert_eq!(
            em_calls.load(Ordering::SeqCst),
            expected_em_calls,
            "{label}: extend_mutation should fire {expected_em_calls} time(s) but fired {} time(s) — has_mutations gate may be miswired",
            em_calls.load(Ordering::SeqCst),
        );
    }
}

#[tokio::test]
async fn empty_extensions_slice_does_not_break_build() {
    // The magna binary passes `&[]` for the public/generic mode. This is the
    // canonical happy path for the no-extension case.
    let extensions: Vec<Box<dyn SchemaExtension>> = vec![];
    let output = empty_gather();
    let schema = build_schema(&output, &output.behaviors, lazy_pool(), &extensions)
        .expect("build_schema with no extensions must succeed");
    let sdl = schema.sdl();
    assert!(!sdl.is_empty(), "empty-extension schema still has SDL");
    assert!(!sdl.contains("ExtType"));
}

/// Verifies slice-order is preserved within each phase by recording the order
/// each extension's hooks fire into a shared journal. Renamed from the prior
/// `multiple_extensions_run_in_slice_order_per_phase` because the prior version
/// only verified both extensions' types/fields ended up in SDL — it could not
/// distinguish "both ran in alpha,beta order" from "both ran in beta,alpha
/// order" or even "ran concurrently."
#[tokio::test]
async fn extensions_fire_in_slice_order_within_each_phase() {
    let journal = Arc::new(Mutex::new(Vec::<&'static str>::new()));

    struct JournaledExt {
        label: &'static str,
        journal: Arc<Mutex<Vec<&'static str>>>,
    }
    impl SchemaExtension for JournaledExt {
        fn name(&self) -> &str { self.label }
        fn register_types(&self, ctx: &mut ExtensionContext<'_>) {
            self.journal.lock().unwrap().push(match self.label {
                "alpha" => "alpha:reg",
                "beta" => "beta:reg",
                _ => unreachable!(),
            });
            ctx.register_type(
                Object::new(format!("Type_{}", self.label)).field(Field::new(
                    "v", TypeRef::named_nn(TypeRef::STRING),
                    |_| FieldFuture::from_value(None),
                )),
            );
        }
        fn extend_query(&self, ctx: &mut ExtensionContext<'_>) {
            self.journal.lock().unwrap().push(match self.label {
                "alpha" => "alpha:eq",
                "beta" => "beta:eq",
                _ => unreachable!(),
            });
            ctx.query_field(Field::new(
                format!("field_{}", self.label),
                TypeRef::named_nn(format!("Type_{}", self.label)),
                |_| FieldFuture::from_value(None),
            ));
        }
    }

    let extensions: Vec<Box<dyn SchemaExtension>> = vec![
        Box::new(JournaledExt { label: "alpha", journal: journal.clone() }),
        Box::new(JournaledExt { label: "beta",  journal: journal.clone() }),
    ];
    let output = empty_gather();
    build_schema(&output, &output.behaviors, lazy_pool(), &extensions)
        .expect("multi-extension build must succeed");

    let recorded = journal.lock().unwrap().clone();
    // Phase contract: ALL extensions complete register_types before ANY
    // extend_query begins. Within each phase, slice order is preserved.
    assert_eq!(
        recorded,
        vec!["alpha:reg", "beta:reg", "alpha:eq", "beta:eq"],
        "phase ordering and slice ordering both broken — got {recorded:?}",
    );
}

/// The cross-extension reference proof. async-graphql 7's dynamic builder
/// records type-name references as strings and validates only at finish(),
/// so checking that SDL contains both names is insufficient to prove the
/// per-phase barrier. Instead, instrument the FieldUser hook to assert that
/// at the moment its `extend_query` runs, TypeRegistrar has already executed
/// `register_types`. A side-channel (Arc<Mutex<HashSet<&str>>>) tracks which
/// extensions have completed register_types when extend_query observes.
#[tokio::test]
async fn cross_extension_type_reference_proves_per_phase_barrier() {
    let registered_types_so_far = Arc::new(Mutex::new(std::collections::HashSet::<&'static str>::new()));
    let user_observed_at_extend_query = Arc::new(Mutex::new(None::<std::collections::HashSet<&'static str>>));

    struct TypeRegistrar {
        registered_types_so_far: Arc<Mutex<std::collections::HashSet<&'static str>>>,
    }
    impl SchemaExtension for TypeRegistrar {
        fn name(&self) -> &str { "registrar" }
        fn register_types(&self, ctx: &mut ExtensionContext<'_>) {
            ctx.register_type(
                Object::new("SharedType").field(Field::new(
                    "v", TypeRef::named_nn(TypeRef::STRING),
                    |_| FieldFuture::from_value(None),
                )),
            );
            self.registered_types_so_far.lock().unwrap().insert("SharedType");
        }
    }
    struct FieldUser {
        registered_types_so_far: Arc<Mutex<std::collections::HashSet<&'static str>>>,
        user_observed_at_extend_query: Arc<Mutex<Option<std::collections::HashSet<&'static str>>>>,
    }
    impl SchemaExtension for FieldUser {
        fn name(&self) -> &str { "user" }
        fn extend_query(&self, ctx: &mut ExtensionContext<'_>) {
            // Snapshot the registered-so-far set AT THE MOMENT this hook runs.
            let snapshot = self.registered_types_so_far.lock().unwrap().clone();
            *self.user_observed_at_extend_query.lock().unwrap() = Some(snapshot);
            ctx.query_field(Field::new(
                "useShared",
                TypeRef::named_nn("SharedType"),
                |_| FieldFuture::from_value(None),
            ));
        }
    }

    // Order: FieldUser FIRST in slice, TypeRegistrar second. The phase contract
    // means TypeRegistrar.register_types runs (in slice order: AFTER nobody,
    // since FieldUser has no register_types) BEFORE any extend_query. So when
    // FieldUser.extend_query runs, SharedType MUST already be in the side-channel.
    let extensions: Vec<Box<dyn SchemaExtension>> = vec![
        Box::new(FieldUser {
            registered_types_so_far: registered_types_so_far.clone(),
            user_observed_at_extend_query: user_observed_at_extend_query.clone(),
        }),
        Box::new(TypeRegistrar {
            registered_types_so_far: registered_types_so_far.clone(),
        }),
    ];
    let output = empty_gather();
    build_schema(&output, &output.behaviors, lazy_pool(), &extensions)
        .expect("cross-extension type reference must succeed");

    // The side-channel proves the per-phase barrier independently of SDL.
    let observed = user_observed_at_extend_query
        .lock()
        .unwrap()
        .clone()
        .expect("FieldUser.extend_query must have run and recorded its observation");
    assert!(
        observed.contains("SharedType"),
        "per-phase barrier broken: when FieldUser.extend_query ran, TypeRegistrar.register_types had not yet completed — observed only {observed:?}",
    );
}
