//! End-to-end proof of the plan-based execution wiring — no database.
//!
//! This is the test the planner never had: a GraphQL request served THROUGH
//! the plan path. It exercises the full lifecycle:
//!
//!   plan_field registration (ExtensionContext) → build_schema attaches the
//!   plan extension → prepare_request inserts PlanResults → parse_query
//!   captures the operation → execute() plans root fields, runs the DAG,
//!   publishes outputs → field resolvers read results by response key.
//!
//! It also pins the two properties that make planning worth having:
//!
//!   * **Cross-field dedup**: two root fields whose steps carry equal
//!     fingerprints execute ONE step, and both fields still resolve — via
//!     the optimizer's remap, which `PlanContext::get_result` follows.
//!   * **Argument isolation**: fields planned with different arguments carry
//!     different fingerprints and never share an execution.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_graphql::dynamic::{Field, FieldFuture, TypeRef};
use async_trait::async_trait;
use magna_build::{
    build_schema, ExtensionContext, GatherOutput, PlanResults, SchemaExtension,
};
use magna_core::{
    ExecutableStep, ExecutionContext, StepFingerprint, StepInputs, StepOutput,
};
use magna_types::{FwGraphError, StepId};

fn lazy_pool() -> sqlx::PgPool {
    sqlx::PgPool::connect_lazy("postgresql://nobody:nobody@127.0.0.1:1/nodb")
        .expect("connect_lazy accepts a bogus URL")
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

/// A DB-free step: returns `base + offset` as its single value and counts
/// its executions. `base` participates in the fingerprint (it stands in for
/// "the arguments this step was built from"); `offset` deliberately does
/// not (it stands in for closure state the fingerprint cannot see — which
/// is exactly why equal fingerprints MUST imply identical outputs, and why
/// these tests only give equal bases equal offsets).
struct ConstStep {
    id: StepId,
    base: i64,
    executions: Arc<AtomicUsize>,
}

#[async_trait]
impl ExecutableStep for ConstStep {
    fn id(&self) -> StepId {
        self.id
    }

    fn dependencies(&self) -> &[StepId] {
        &[]
    }

    fn fingerprint(&self) -> StepFingerprint {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.base.hash(&mut h);
        StepFingerprint::new(std::any::TypeId::of::<ConstStep>(), vec![], h.finish())
    }

    async fn execute(
        &self,
        _ctx: &ExecutionContext,
        _inputs: StepInputs,
    ) -> Result<StepOutput, FwGraphError> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        Ok(StepOutput::from_values(vec![Arc::new(self.base)]))
    }
}

/// The embedder side, written exactly the way fw-resolvers would write it:
/// each planned field gets (a) a schema entry whose resolver reads
/// `PlanResults` by its response key, and (b) a plan fn registering its step.
struct PlannedFieldsExtension {
    executions: Arc<AtomicUsize>,
}

/// A resolver that reads the field's planned output. This is the ONLY data
/// path — there is no fallback fetch. A planned field with no output is a
/// hard error, never silently empty.
fn planned_resolver(field_name: &'static str) -> impl Fn(async_graphql::dynamic::ResolverContext) -> FieldFuture + Send + Sync + 'static
{
    move |ctx| {
        FieldFuture::new(async move {
            let results = ctx.data::<Arc<PlanResults>>()?;
            let key = ctx
                .field()
                .alias()
                .unwrap_or(field_name)
                .to_string();
            let output = results.field_output(&key).ok_or_else(|| {
                async_graphql::Error::new(format!("field '{key}' was not planned"))
            })?;
            let value = output.values[0]
                .downcast_ref::<i64>()
                .copied()
                .ok_or_else(|| async_graphql::Error::new("planned output type mismatch"))?;
            Ok(Some(async_graphql::Value::from(value)))
        })
    }
}

impl SchemaExtension for PlannedFieldsExtension {
    fn name(&self) -> &str {
        "planned-fields"
    }

    fn extend_query(&self, ctx: &mut ExtensionContext<'_>) {
        // `answer(n: Int!)` — planned from its argument value.
        ctx.query_field(
            Field::new("answer", TypeRef::named_nn(TypeRef::INT), planned_resolver("answer"))
                .argument(async_graphql::dynamic::InputValue::new(
                    "n",
                    TypeRef::named_nn(TypeRef::INT),
                )),
        );

        let executions = Arc::clone(&self.executions);
        ctx.plan_field("Query", "answer", move |scope, args| {
            let n = args.get("n").and_then(|v| v.as_i64()).unwrap_or(0);
            scope.register(Arc::new(ConstStep {
                id: scope.next_step_id(),
                base: n,
                executions: Arc::clone(&executions),
            }))
        });
    }
}

async fn planned_schema(executions: Arc<AtomicUsize>) -> async_graphql::dynamic::Schema {
    let gather = empty_gather();
    build_schema(
        &gather,
        &gather.behaviors,
        lazy_pool(),
        &[Box::new(PlannedFieldsExtension { executions })],
    )
    .expect("schema build")
}

#[tokio::test]
async fn a_field_is_served_entirely_through_the_plan_path() {
    let executions = Arc::new(AtomicUsize::new(0));
    let schema = planned_schema(Arc::clone(&executions)).await;

    let resp = schema.execute("{ answer(n: 42) }").await;
    assert!(resp.errors.is_empty(), "unexpected errors: {:?}", resp.errors);
    assert_eq!(resp.data.to_string(), r#"{answer: 42}"#);
    assert_eq!(executions.load(Ordering::SeqCst), 1, "exactly one step execution");
}

#[tokio::test]
async fn equal_arguments_across_fields_execute_once_and_both_fields_resolve() {
    // THE dedup proof, end to end: two aliased root fields, same argument,
    // so their steps carry equal fingerprints. The optimizer collapses them
    // to one execution — and the field whose step was deduplicated away
    // still resolves, because get_result follows the optimizer's remap.
    let executions = Arc::new(AtomicUsize::new(0));
    let schema = planned_schema(Arc::clone(&executions)).await;

    let resp = schema
        .execute("{ a: answer(n: 7) b: answer(n: 7) }")
        .await;
    assert!(resp.errors.is_empty(), "unexpected errors: {:?}", resp.errors);
    assert_eq!(resp.data.to_string(), r#"{a: 7, b: 7}"#);
    assert_eq!(
        executions.load(Ordering::SeqCst),
        1,
        "equal fingerprints must collapse to ONE execution shared by both fields",
    );
}

#[tokio::test]
async fn different_arguments_never_share_an_execution() {
    // The other half of the §fingerprint contract: different arguments ⇒
    // different fingerprints ⇒ no sharing. If this ever fails, one field is
    // being served another field's rows — the data-bleed class.
    let executions = Arc::new(AtomicUsize::new(0));
    let schema = planned_schema(Arc::clone(&executions)).await;

    let resp = schema
        .execute("{ a: answer(n: 1) b: answer(n: 2) }")
        .await;
    assert!(resp.errors.is_empty(), "unexpected errors: {:?}", resp.errors);
    assert_eq!(resp.data.to_string(), r#"{a: 1, b: 2}"#);
    assert_eq!(executions.load(Ordering::SeqCst), 2, "two distinct executions");
}

#[tokio::test]
async fn variables_resolve_into_plan_arguments() {
    // Arguments arriving via GraphQL variables (the common client shape)
    // must reach the plan fn resolved, not as AST variable references.
    let executions = Arc::new(AtomicUsize::new(0));
    let schema = planned_schema(Arc::clone(&executions)).await;

    let req = async_graphql::Request::new("query Q($n: Int!) { answer(n: $n) }")
        .variables(async_graphql::Variables::from_json(serde_json::json!({ "n": 9 })));
    let resp = schema.execute(req).await;
    assert!(resp.errors.is_empty(), "unexpected errors: {:?}", resp.errors);
    assert_eq!(resp.data.to_string(), r#"{answer: 9}"#);
}

/// A step that depends on another step, and echoes its dependency's value.
/// This is the document-assembly shape: leaves fetch, an assembler composes.
struct EchoDepStep {
    id: StepId,
    deps: Vec<StepId>,
    tag: i64,
}

#[async_trait]
impl ExecutableStep for EchoDepStep {
    fn id(&self) -> StepId {
        self.id
    }

    fn dependencies(&self) -> &[StepId] {
        &self.deps
    }

    fn is_unary(&self) -> bool {
        true
    }

    fn fingerprint(&self) -> StepFingerprint {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.tag.hash(&mut h);
        StepFingerprint::new(
            std::any::TypeId::of::<EchoDepStep>(),
            self.deps.clone(),
            h.finish(),
        )
    }

    async fn execute(
        &self,
        _ctx: &ExecutionContext,
        inputs: StepInputs,
    ) -> Result<StepOutput, FwGraphError> {
        // Positional downcast — exactly what a document assembler does, and
        // what silently breaks if a dependency's output goes missing.
        let dep = inputs
            .dep_outputs
            .first()
            .ok_or_else(|| FwGraphError::ExecutionError("assembler got no inputs".into()))?;
        let base = dep.values[0]
            .downcast_ref::<i64>()
            .copied()
            .ok_or_else(|| FwGraphError::ExecutionError("dependency had wrong type".into()))?;
        Ok(StepOutput::from_values(vec![Arc::new(base + self.tag)]))
    }
}

/// Two roots, each registering a leaf (same key ⇒ deduped) plus its own
/// assembler that depends on that leaf.
struct AssemblerExtension {
    executions: Arc<AtomicUsize>,
}

impl SchemaExtension for AssemblerExtension {
    fn name(&self) -> &str {
        "assemblers"
    }

    fn extend_query(&self, ctx: &mut ExtensionContext<'_>) {
        for (field, tag) in [("left", 100i64), ("right", 200i64)] {
            ctx.query_field(Field::new(
                field,
                TypeRef::named_nn(TypeRef::INT),
                planned_resolver(field),
            ));

            let executions = Arc::clone(&self.executions);
            ctx.plan_field("Query", field, move |scope, _args| {
                // Identical leaf in both roots ⇒ the SECOND one is
                // deduplicated away, and its assembler's dependency id
                // becomes stale.
                let leaf = scope.register(Arc::new(ConstStep {
                    id: scope.next_step_id(),
                    base: 5,
                    executions: Arc::clone(&executions),
                }))?;
                scope.register(Arc::new(EchoDepStep {
                    id: scope.next_step_id(),
                    deps: vec![leaf],
                    tag,
                }))
            });
        }
    }
}

/// Regression: an assembly step whose dependency was deduplicated away must
/// still receive that dependency's output.
///
/// Dedup removes the duplicate leaf and rebuilds graph edges, but a surviving
/// step still NAMES the id it was constructed with. The executor therefore
/// has to resolve dependency ids through the remap before looking them up —
/// and must treat a genuinely missing output as an error rather than
/// dropping it, because `StepInputs` is positional and a silent drop shifts
/// every later input into the wrong slot.
#[tokio::test]
async fn an_assembler_whose_dependency_was_deduped_still_gets_its_input() {
    let executions = Arc::new(AtomicUsize::new(0));
    let gather = empty_gather();
    let schema = build_schema(
        &gather,
        &gather.behaviors,
        lazy_pool(),
        &[Box::new(AssemblerExtension {
            executions: Arc::clone(&executions),
        })],
    )
    .expect("schema build");

    let resp = schema.execute("{ left right }").await;
    assert!(resp.errors.is_empty(), "unexpected errors: {:?}", resp.errors);
    // Both assemblers saw the shared leaf's value (5), and added their tags.
    assert_eq!(resp.data.to_string(), r#"{left: 105, right: 205}"#);
    assert_eq!(
        executions.load(Ordering::SeqCst),
        1,
        "the shared leaf must execute once",
    );
}

#[tokio::test]
async fn unplanned_operations_pay_nothing() {
    // A query that touches no planned field must not fail and must not
    // execute any step.
    let executions = Arc::new(AtomicUsize::new(0));
    let schema = planned_schema(Arc::clone(&executions)).await;

    let resp = schema.execute("{ __typename }").await;
    assert!(resp.errors.is_empty());
    assert_eq!(executions.load(Ordering::SeqCst), 0, "no steps executed");
}
