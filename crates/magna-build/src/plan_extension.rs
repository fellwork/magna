//! Plan-based execution wiring — the bridge between magna-core's two-phase
//! planner and async-graphql's request lifecycle.
//!
//! Everything below this module already existed and was tested: the planner
//! DAG (`magna-core`), fingerprint deduplication (`optimize`), batched
//! Postgres steps (`magna-dataplan::PgSelectStep`), and the register→execute→
//! read bridge (`PlanContext`). What was missing was the piece that turns an
//! **incoming GraphQL operation** into a plan and hands the executed results
//! to field resolvers. This module is that piece.
//!
//! # Lifecycle
//!
//! 1. At schema-build time an embedder registers plan fns via
//!    [`crate::ExtensionContext::plan_field`], keyed by `(type, field)`.
//!    If at least one is registered, `build_schema` attaches
//!    [`PlanExtensionFactory`] to the schema.
//! 2. Per request, [`PlanExtension::prepare_request`] inserts a shared
//!    [`PlanResults`] handle into the request data.
//! 3. [`PlanExtension::parse_query`] captures the parsed operation and its
//!    variables.
//! 4. [`PlanExtension::execute`] walks the operation's **root selections**,
//!    calls every registered plan fn (with variable-resolved field
//!    arguments), executes the resulting plan — one DAG for the whole
//!    operation, so steps shared between root fields deduplicate and batch —
//!    and publishes the outputs into the [`PlanResults`] handle.
//! 5. Field resolvers read their output via
//!    [`PlanResults::field_output`], keyed by the field's **response key**
//!    (alias if present, else name).
//!
//! # Scope (first pass)
//!
//! * Root `query` fields only. Fields reached through fragment spreads or
//!   inline fragments at the root are **not** planned — a planned field
//!   inside a fragment will find no output and should error loudly, not
//!   silently fall back. Mutations and subscriptions are unplanned.
//! * The plan executes **before** field resolution begins (no lazy
//!   per-field planning). This is the Grafast shape: plan the whole
//!   operation, then run it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use async_graphql::extensions::{
    Extension, ExtensionContext as RequestContext, ExtensionFactory, NextExecute, NextParseQuery,
    NextPrepareRequest,
};
use async_graphql::parser::types::{ExecutableDocument, OperationType, Selection};
use async_graphql::{Request, Response, ServerError, ServerResult, Variables};
use magna_core::{ExecutableStep, ExecutionContext, StepOutput};
use magna_types::{FwGraphError, JwtClaims, StepId};

use crate::plan_resolver::PlanContext;

/// Batch size handed to the planner. Bounds how many parent ids a single
/// batched child query carries; not a hard result limit.
const PLAN_BATCH_SIZE: usize = 64;

/// A planned field's arguments: name → JSON value, with GraphQL variables
/// already substituted. Plan fns never see raw AST.
pub type FieldArgs = serde_json::Map<String, serde_json::Value>;

/// The registration surface a [`PlanFn`] works against: allocates step ids
/// and registers steps into the operation's single shared plan.
pub struct PlanScope<'a> {
    ctx: &'a PlanContext,
    next_id: &'a AtomicU32,
}

impl PlanScope<'_> {
    /// Allocate a fresh step id, unique within this operation's plan.
    pub fn next_step_id(&self) -> StepId {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Register a step. Steps registered by different fields land in the
    /// same plan, which is what lets the optimizer deduplicate shared work
    /// across fields.
    pub fn register(&self, step: Arc<dyn ExecutableStep>) -> Result<StepId, FwGraphError> {
        self.ctx.register_step(step)
    }
}

/// A field's plan function: registers the steps that produce the field's
/// data and returns the id of the step whose output the resolver should
/// read. Runs once per request, before any resolver.
pub type PlanFn =
    Arc<dyn Fn(&PlanScope<'_>, &FieldArgs) -> Result<StepId, FwGraphError> + Send + Sync>;

/// Plan fns keyed by `(type_name, field_name)`. Populated at schema-build
/// time via [`crate::ExtensionContext::plan_field`]; immutable afterwards.
#[derive(Default)]
pub struct PlanRegistry {
    fields: HashMap<(String, String), PlanFn>,
}

impl PlanRegistry {
    /// Register a plan fn for `type_name.field_name`. Last registration wins,
    /// matching the "later extensions may override earlier ones" convention.
    pub fn plan_field(
        &mut self,
        type_name: impl Into<String>,
        field_name: impl Into<String>,
        f: impl Fn(&PlanScope<'_>, &FieldArgs) -> Result<StepId, FwGraphError> + Send + Sync + 'static,
    ) {
        self.fields
            .insert((type_name.into(), field_name.into()), Arc::new(f));
    }

    /// True when no plan fns are registered — `build_schema` skips attaching
    /// the extension entirely, so unplanned schemas pay nothing.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    fn get(&self, type_name: &str, field_name: &str) -> Option<&PlanFn> {
        // Keyed lookup without allocating a (String, String) per call.
        self.fields
            .iter()
            .find(|((t, f), _)| t == type_name && f == field_name)
            .map(|(_, v)| v)
    }
}

/// The executed plan's outputs, readable by field resolvers.
///
/// Inserted into the request data as `Arc<PlanResults>` by
/// [`PlanExtension::prepare_request`]; published once by
/// [`PlanExtension::execute`] after the plan runs. A resolver for a planned
/// field calls [`field_output`](Self::field_output) with its response key.
#[derive(Default)]
pub struct PlanResults {
    inner: OnceLock<Planned>,
}

struct Planned {
    ctx: PlanContext,
    /// Response key (alias if present, else field name) → the field's output step.
    field_steps: HashMap<String, StepId>,
}

impl PlanResults {
    /// The executed output for the field with this response key, or `None`
    /// when the field was not planned (not registered, not a root field, or
    /// reached through a fragment). Resolvers should treat `None` for a field
    /// that *expects* planning as an error, never as empty data.
    pub fn field_output(&self, response_key: &str) -> Option<Arc<StepOutput>> {
        let planned = self.inner.get()?;
        let step_id = planned.field_steps.get(response_key)?;
        planned.ctx.get_result(*step_id)
    }

    /// True once the operation's plan has executed and results are readable.
    pub fn is_planned(&self) -> bool {
        self.inner.get().is_some()
    }
}

/// Attaches plan-based execution to a schema. Constructed by `build_schema`
/// when the [`PlanRegistry`] is non-empty.
pub struct PlanExtensionFactory {
    registry: Arc<PlanRegistry>,
}

impl PlanExtensionFactory {
    pub fn new(registry: Arc<PlanRegistry>) -> Self {
        Self { registry }
    }
}

impl ExtensionFactory for PlanExtensionFactory {
    fn create(&self) -> Arc<dyn Extension> {
        // One PlanExtension per request: the captured document and the
        // results handle are request-scoped state.
        Arc::new(PlanExtension {
            registry: Arc::clone(&self.registry),
            captured: Mutex::new(None),
            results: Arc::new(PlanResults::default()),
        })
    }
}

/// Per-request extension instance. See the module docs for the lifecycle.
pub struct PlanExtension {
    registry: Arc<PlanRegistry>,
    /// Captured by `parse_query`, consumed by `execute`.
    captured: Mutex<Option<(ExecutableDocument, Variables)>>,
    results: Arc<PlanResults>,
}

impl PlanExtension {
    /// Build and execute the plan for the captured operation. Returns
    /// resolver-visible errors; an operation with no planned fields is a
    /// successful no-op.
    async fn plan(
        &self,
        ctx: &RequestContext<'_>,
        operation_name: Option<&str>,
    ) -> Result<(), Vec<ServerError>> {
        let Some((doc, variables)) = self.captured.lock().expect("captured lock").take() else {
            return Ok(());
        };

        // Select the operation the same way the executor will. Ambiguity
        // (multiple operations, no name) is not planning's error to report —
        // skip, and let downstream produce the canonical error.
        let operation = match operation_name {
            Some(name) => doc.operations.iter().find(|(n, _)| {
                n.map(|n| n.as_str() == name).unwrap_or(false)
            }),
            None => {
                let mut iter = doc.operations.iter();
                let first = iter.next();
                if iter.next().is_some() { None } else { first }
            }
        };
        let Some((_, op)) = operation else {
            return Ok(());
        };
        if op.node.ty != OperationType::Query {
            return Ok(());
        }

        // Gather the planned root fields: (response key, plan fn, resolved args).
        let mut planned_fields: Vec<(String, PlanFn, FieldArgs)> = Vec::new();
        for sel in &op.node.selection_set.node.items {
            let Selection::Field(field) = &sel.node else {
                // Fragment spread / inline fragment at the root: not planned
                // in the first pass (module docs). The field's resolver will
                // find no output and must error loudly.
                continue;
            };
            let name = field.node.name.node.as_str();
            let Some(plan_fn) = self.registry.get("Query", name) else {
                continue;
            };

            let mut args = FieldArgs::new();
            for (arg_name, arg_value) in &field.node.arguments {
                let const_value = arg_value
                    .node
                    .clone()
                    .into_const_with(|var_name| {
                        variables.get(&var_name).cloned().ok_or_else(|| {
                            ServerError::new(
                                format!("unknown variable ${var_name}"),
                                Some(arg_value.pos),
                            )
                        })
                    })
                    .map_err(|e| vec![e])?;
                let json = const_value.into_json().map_err(|e| {
                    vec![ServerError::new(
                        format!("argument {arg_name} is not JSON-representable: {e}"),
                        Some(arg_value.pos),
                    )]
                })?;
                args.insert(arg_name.node.to_string(), json);
            }

            let response_key = field
                .node
                .alias
                .as_ref()
                .map(|a| a.node.to_string())
                .unwrap_or_else(|| name.to_string());
            planned_fields.push((response_key, Arc::clone(plan_fn), args));
        }

        if planned_fields.is_empty() {
            return Ok(());
        }

        // One ExecutionContext and ONE plan for the whole operation — this is
        // what makes cross-field dedup structural rather than per-field.
        let jwt_claims = ctx.data_opt::<JwtClaims>().cloned().map(Arc::new);
        let variables_json = serde_json::Value::Object(
            variables
                .iter()
                .map(|(name, value)| {
                    (
                        name.to_string(),
                        value.clone().into_json().unwrap_or(serde_json::Value::Null),
                    )
                })
                .collect(),
        );
        let exec_ctx = Arc::new(ExecutionContext {
            request_id: uuid::Uuid::new_v4(),
            jwt_claims,
            variables: Arc::new(variables_json),
        });

        let plan_ctx = PlanContext::new(exec_ctx, PLAN_BATCH_SIZE);
        let next_id = AtomicU32::new(1);
        let scope = PlanScope {
            ctx: &plan_ctx,
            next_id: &next_id,
        };

        let mut field_steps = HashMap::new();
        for (response_key, plan_fn, args) in planned_fields {
            let step_id = plan_fn(&scope, &args).map_err(|e| {
                vec![ServerError::new(
                    format!("planning failed for field '{response_key}': {e}"),
                    None,
                )]
            })?;
            field_steps.insert(response_key, step_id);
        }

        plan_ctx
            .execute()
            .await
            .map_err(|e| vec![ServerError::new(format!("plan execution failed: {e}"), None)])?;

        // OnceLock: publish exactly once. A second publish would mean two
        // execute() hooks ran for one request instance — a bug upstream.
        let _ = self.results.inner.set(Planned {
            ctx: plan_ctx,
            field_steps,
        });
        Ok(())
    }
}

#[async_trait::async_trait]
impl Extension for PlanExtension {
    async fn prepare_request(
        &self,
        ctx: &RequestContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        // Make the (not-yet-populated) results handle visible to resolvers.
        let request = request.data(Arc::clone(&self.results));
        next.run(ctx, request).await
    }

    async fn parse_query(
        &self,
        ctx: &RequestContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let doc = next.run(ctx, query, variables).await?;
        if !self.registry.is_empty() {
            *self.captured.lock().expect("captured lock") = Some((doc.clone(), variables.clone()));
        }
        Ok(doc)
    }

    async fn execute(
        &self,
        ctx: &RequestContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        if let Err(errors) = self.plan(ctx, operation_name).await {
            return Response::from_errors(errors);
        }
        next.run(ctx, operation_name).await
    }
}
