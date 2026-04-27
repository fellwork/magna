//! Optimizer -- post-planning transformations.
//!
//! Runs immediately after the Planner produces an ExecutionPlan:
//! 1. Topologically sorts the DAG for execution ordering.
//! 2. Deduplicates steps with matching fingerprints.
//! 3. Hoists unary steps so they execute as early as possible.

use crate::fingerprint::StepFingerprint;
use crate::planner::ExecutionPlan;
use magna_types::StepId;
use std::collections::HashMap;

/// Result of the optimization pass: an ordered list of step IDs
/// to execute, plus the (possibly reduced) plan.
pub struct OptimizedPlan {
    /// The execution plan (may have fewer steps after deduplication).
    pub plan: ExecutionPlan,

    /// Step IDs in topological execution order. Steps earlier in this
    /// list have no unresolved dependencies when it is their turn.
    pub execution_order: Vec<StepId>,
}

/// Run all optimization passes on an ExecutionPlan.
///
/// The passes are:
/// 1. Topological sort (required for dedup ordering)
/// 2. Fingerprint-based deduplication
/// 3. Re-sort after dedup (graph may have changed)
/// 4. Unary step hoisting
pub fn optimize(mut plan: ExecutionPlan) -> OptimizedPlan {
    // First topo sort to get a valid processing order for dedup
    let order = topological_sort(&plan);

    // Deduplicate steps with matching fingerprints
    deduplicate(&mut plan, &order);

    // Re-sort after dedup since the graph may have changed
    let mut execution_order = topological_sort(&plan);

    // Hoist unary steps to the front of each "tier"
    hoist_unary_steps(&plan, &mut execution_order);

    OptimizedPlan {
        plan,
        execution_order,
    }
}

/// Produce a topological ordering of step IDs in the plan.
///
/// Steps with no dependencies come first. If the graph has a cycle,
/// this panics -- the Planner should have caught that.
pub fn topological_sort(plan: &ExecutionPlan) -> Vec<StepId> {
    let sorted = petgraph::algo::toposort(&plan.graph, None)
        .expect("ExecutionPlan should be acyclic (validated by Planner)");

    sorted.iter().map(|&idx| plan.graph[idx]).collect()
}

/// Deduplicate steps with identical fingerprints.
///
/// Processes steps in topological order so that dependency IDs are
/// already remapped by the time we compute a step's fingerprint.
fn deduplicate(plan: &mut ExecutionPlan, topo_order: &[StepId]) {
    let mut seen: HashMap<StepFingerprint, StepId> = HashMap::new();
    let mut remap: HashMap<StepId, StepId> = HashMap::new();
    let mut to_remove: Vec<StepId> = Vec::new();

    for &step_id in topo_order {
        let Some(step) = plan.steps.get(&step_id) else {
            continue;
        };

        // Remap dependency IDs using the accumulated remap table
        let remapped_deps: Vec<StepId> = step
            .dependencies()
            .iter()
            .map(|id| *remap.get(id).unwrap_or(id))
            .collect();

        // Build the fingerprint with remapped deps
        let fp = StepFingerprint {
            type_id: step.fingerprint().type_id,
            dep_ids: remapped_deps,
            config_hash: step.fingerprint().config_hash,
        };

        if let Some(&canonical_id) = seen.get(&fp) {
            // This step is a duplicate -- remap it to the canonical step
            remap.insert(step_id, canonical_id);
            to_remove.push(step_id);
        } else {
            seen.insert(fp, step_id);
        }
    }

    // Remove duplicate steps from the plan
    for id in to_remove {
        plan.remove_step(id);
    }

    // Remap dependency references in remaining steps.
    // We need to update steps that depended on removed steps to
    // instead depend on the canonical step. Since ExecutableStep
    // owns its deps, we rebuild edges in the graph.
    if !remap.is_empty() {
        rebuild_edges_after_remap(plan, &remap);
    }
}

/// After deduplication, rebuild the graph edges so that steps which
/// previously depended on a now-removed step instead point to the
/// canonical replacement.
fn rebuild_edges_after_remap(plan: &mut ExecutionPlan, remap: &HashMap<StepId, StepId>) {
    // Clear all edges and re-add them using remapped deps
    plan.graph.clear_edges();

    let step_ids: Vec<StepId> = plan.steps.keys().copied().collect();
    for step_id in step_ids {
        let step = &plan.steps[&step_id];
        let step_idx = plan.node_map[&step_id];

        for &dep_id in step.dependencies() {
            let remapped_dep = *remap.get(&dep_id).unwrap_or(&dep_id);
            if let Some(&dep_idx) = plan.node_map.get(&remapped_dep) {
                plan.graph.add_edge(dep_idx, step_idx, ());
            }
        }
    }
}

/// Hoist unary steps earlier in the execution order.
///
/// Unary steps execute once regardless of batch size, so they should
/// run as early as possible to unblock dependents. This pass moves
/// unary steps to the earliest position in the topological order that
/// respects their dependencies.
fn hoist_unary_steps(plan: &ExecutionPlan, order: &mut Vec<StepId>) {
    // Partition into unary and non-unary, preserving relative order
    let mut unary_ids: Vec<StepId> = Vec::new();
    let mut non_unary_ids: Vec<StepId> = Vec::new();

    for &id in order.iter() {
        if let Some(step) = plan.steps.get(&id) {
            if step.is_unary() {
                unary_ids.push(id);
            } else {
                non_unary_ids.push(id);
            }
        }
    }

    // Rebuild: unary first (they have no batch deps), then non-unary.
    // Both sub-lists are already in valid topological order since we
    // preserved relative ordering within each partition.
    //
    // This is safe because unary steps' dependencies are either:
    // - Other unary steps (which are also hoisted)
    // - No dependencies (root steps)
    // A unary step should never depend on a non-unary step (that would
    // make it batch-dependent). We verify this at placement time.
    let mut result = Vec::with_capacity(order.len());

    // Insert unary steps, respecting their internal dependency order
    result.extend(unary_ids.iter().copied());

    // Insert non-unary steps after all unary steps
    result.extend(non_unary_ids.iter().copied());

    *order = result;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::Planner;
    use crate::step::{ExecutableStep, ExecutionContext, StepInputs, StepOutput};
    use std::any::TypeId;
    use std::sync::Arc;

    /// Mock step for optimizer tests.
    struct MockStep {
        id: StepId,
        deps: Vec<StepId>,
        unary: bool,
        config: u64,
    }

    impl MockStep {
        fn new(id: StepId, deps: Vec<StepId>) -> Arc<dyn ExecutableStep> {
            Arc::new(Self {
                id,
                deps,
                unary: false,
                config: id as u64,
            })
        }

        fn new_unary(id: StepId, deps: Vec<StepId>) -> Arc<dyn ExecutableStep> {
            Arc::new(Self {
                id,
                deps,
                unary: true,
                config: id as u64,
            })
        }

        /// Create a step that will have the same fingerprint as another
        /// step with the given config value and same deps.
        fn new_duplicate(id: StepId, deps: Vec<StepId>, config: u64) -> Arc<dyn ExecutableStep> {
            Arc::new(Self {
                id,
                deps,
                unary: false,
                config,
            })
        }
    }

    #[async_trait::async_trait]
    impl ExecutableStep for MockStep {
        fn id(&self) -> StepId {
            self.id
        }
        fn dependencies(&self) -> &[StepId] {
            &self.deps
        }
        fn is_unary(&self) -> bool {
            self.unary
        }
        fn fingerprint(&self) -> StepFingerprint {
            StepFingerprint::new(TypeId::of::<MockStep>(), self.deps.clone(), self.config)
        }
        async fn execute(
            &self,
            _ctx: &ExecutionContext,
            _inputs: StepInputs,
        ) -> Result<StepOutput, magna_types::FwGraphError> {
            Ok(StepOutput::from_values(vec![]))
        }
    }

    #[test]
    fn topological_sort_respects_deps() {
        let mut planner = Planner::new(1);
        planner.register(MockStep::new(0, vec![]));
        planner.register(MockStep::new(1, vec![0]));
        planner.register(MockStep::new(2, vec![0]));
        planner.register(MockStep::new(3, vec![1, 2]));

        let plan = planner.build().unwrap();
        let order = topological_sort(&plan);

        // Step 0 must come before 1 and 2; 1 and 2 must come before 3
        let pos = |id: StepId| order.iter().position(|&x| x == id).unwrap();
        assert!(pos(0) < pos(1));
        assert!(pos(0) < pos(2));
        assert!(pos(1) < pos(3));
        assert!(pos(2) < pos(3));
    }

    #[test]
    fn unary_steps_hoisted_to_front() {
        let mut planner = Planner::new(1);
        planner.register(MockStep::new(0, vec![]));           // non-unary root
        planner.register(MockStep::new_unary(1, vec![]));     // unary root
        planner.register(MockStep::new(2, vec![0]));          // non-unary
        planner.register(MockStep::new(3, vec![1]));          // depends on unary

        let plan = planner.build().unwrap();
        let optimized = optimize(plan);

        // Unary step 1 should come before non-unary steps in the order
        let pos = |id: StepId| optimized.execution_order.iter().position(|&x| x == id).unwrap();
        assert!(pos(1) < pos(0), "unary step 1 should come before non-unary step 0");
        assert!(pos(1) < pos(2), "unary step 1 should come before non-unary step 2");
    }

    #[test]
    fn deduplication_removes_identical_steps() {
        let mut planner = Planner::new(1);
        planner.register(MockStep::new(0, vec![]));
        // Steps 1 and 2 have the same type, same deps, and same config
        planner.register(MockStep::new_duplicate(1, vec![0], 42));
        planner.register(MockStep::new_duplicate(2, vec![0], 42));

        let plan = planner.build().unwrap();
        assert_eq!(plan.step_count(), 3);

        let optimized = optimize(plan);
        // One of the duplicates should have been removed
        assert_eq!(optimized.plan.step_count(), 2);
    }
}
