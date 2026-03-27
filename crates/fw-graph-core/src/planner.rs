//! Planner -- the plan-phase entry point.
//!
//! The Planner accepts step registrations and their dependency edges, then
//! builds an `ExecutionPlan` (a petgraph DAG). It validates acyclicity
//! before returning the plan.

use crate::step::ExecutableStep;
use fw_graph_types::{FwGraphError, StepId};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use std::sync::Arc;

/// The execution plan produced by the Planner.
///
/// This is a directed acyclic graph where each node is an `ExecutableStep`
/// and each edge represents a data dependency (parent -> child means "child
/// depends on parent's output").
pub struct ExecutionPlan {
    /// The petgraph DAG. Nodes hold step IDs; edges encode dependencies.
    pub(crate) graph: DiGraph<StepId, ()>,

    /// Map from StepId to the node index in the graph.
    pub(crate) node_map: HashMap<StepId, NodeIndex>,

    /// Map from StepId to the step implementation.
    pub(crate) steps: HashMap<StepId, Arc<dyn ExecutableStep>>,

    /// The batch size for this execution (number of root items).
    pub batch_size: usize,
}

impl std::fmt::Debug for ExecutionPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionPlan")
            .field("step_count", &self.steps.len())
            .field("batch_size", &self.batch_size)
            .field("step_ids", &self.step_ids())
            .finish()
    }
}

impl ExecutionPlan {
    /// Get a step by its ID.
    pub fn get_step(&self, id: StepId) -> Option<&Arc<dyn ExecutableStep>> {
        self.steps.get(&id)
    }

    /// Get all step IDs in the plan.
    pub fn step_ids(&self) -> Vec<StepId> {
        self.steps.keys().copied().collect()
    }

    /// Get the number of steps in the plan.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Remove a step from the plan (used during deduplication).
    pub(crate) fn remove_step(&mut self, id: StepId) {
        if let Some(idx) = self.node_map.remove(&id) {
            self.graph.remove_node(idx);
            self.steps.remove(&id);
            // Rebuild node_map after removal since petgraph may reindex.
            // We do a full rebuild to stay correct.
            self.rebuild_node_map();
        }
    }

    /// Rebuild the node_map from the current graph state.
    fn rebuild_node_map(&mut self) {
        self.node_map.clear();
        for idx in self.graph.node_indices() {
            let step_id = self.graph[idx];
            self.node_map.insert(step_id, idx);
        }
    }

    /// Get the dependencies (parent step IDs) of a given step.
    pub fn dependencies_of(&self, id: StepId) -> Vec<StepId> {
        let Some(&idx) = self.node_map.get(&id) else {
            return Vec::new();
        };
        self.graph
            .neighbors_directed(idx, petgraph::Direction::Incoming)
            .map(|n| self.graph[n])
            .collect()
    }
}

/// The Planner builds an ExecutionPlan from step registrations.
///
/// Usage:
/// ```ignore
/// let mut planner = Planner::new(batch_size);
/// planner.register(step_a);
/// planner.register(step_b); // step_b.dependencies() returns [step_a.id()]
/// let plan = planner.build()?;
/// ```
pub struct Planner {
    steps: Vec<Arc<dyn ExecutableStep>>,
    batch_size: usize,
}

impl Planner {
    /// Create a new Planner for a given batch size.
    pub fn new(batch_size: usize) -> Self {
        Self {
            steps: Vec::new(),
            batch_size,
        }
    }

    /// Register a step with the planner.
    ///
    /// Steps must be registered in dependency order -- a step's dependencies
    /// must already be registered before it is. The step's `id()` must be unique.
    pub fn register(&mut self, step: Arc<dyn ExecutableStep>) {
        self.steps.push(step);
    }

    /// Build the ExecutionPlan from all registered steps.
    ///
    /// This constructs the DAG, validates that all dependencies reference
    /// registered steps, and checks for cycles.
    pub fn build(self) -> Result<ExecutionPlan, FwGraphError> {
        let mut graph = DiGraph::new();
        let mut node_map: HashMap<StepId, NodeIndex> = HashMap::new();
        let mut steps_map: HashMap<StepId, Arc<dyn ExecutableStep>> = HashMap::new();

        // Add all steps as nodes
        for step in &self.steps {
            let id = step.id();
            let idx = graph.add_node(id);
            node_map.insert(id, idx);
            steps_map.insert(id, Arc::clone(step));
        }

        // Add dependency edges (from dependency -> dependent)
        for step in &self.steps {
            let step_id = step.id();
            let step_idx = node_map[&step_id];

            for &dep_id in step.dependencies() {
                let dep_idx = node_map.get(&dep_id).ok_or_else(|| {
                    FwGraphError::StepNotFound(dep_id)
                })?;
                // Edge direction: dep -> step (step depends on dep)
                graph.add_edge(*dep_idx, step_idx, ());
            }
        }

        // Cycle detection: petgraph's toposort returns Err if there's a cycle
        if let Err(cycle) = petgraph::algo::toposort(&graph, None) {
            let cycle_node = cycle.node_id();
            let cycle_step_id = graph[cycle_node];
            return Err(FwGraphError::PlanCycle(vec![cycle_step_id]));
        }

        Ok(ExecutionPlan {
            graph,
            node_map,
            steps: steps_map,
            batch_size: self.batch_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fingerprint::StepFingerprint;
    use crate::step::{ExecutionContext, StepInputs, StepOutput};
    use std::any::TypeId;

    /// A minimal mock step for testing the planner.
    struct MockStep {
        id: StepId,
        deps: Vec<StepId>,
    }

    impl MockStep {
        fn new(id: StepId, deps: Vec<StepId>) -> Arc<dyn ExecutableStep> {
            Arc::new(Self { id, deps })
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
        fn fingerprint(&self) -> StepFingerprint {
            StepFingerprint::new(TypeId::of::<MockStep>(), self.deps.clone(), self.id)
        }
        async fn execute(
            &self,
            _ctx: &ExecutionContext,
            _inputs: StepInputs,
        ) -> Result<StepOutput, FwGraphError> {
            Ok(StepOutput::from_values(vec![]))
        }
    }

    #[test]
    fn build_simple_dag() {
        let mut planner = Planner::new(1);
        planner.register(MockStep::new(0, vec![]));
        planner.register(MockStep::new(1, vec![0]));
        planner.register(MockStep::new(2, vec![0]));
        planner.register(MockStep::new(3, vec![1, 2]));

        let plan = planner.build().expect("should build without error");
        assert_eq!(plan.step_count(), 4);

        // Step 3 depends on 1 and 2
        let deps = plan.dependencies_of(3);
        assert!(deps.contains(&1));
        assert!(deps.contains(&2));

        // Step 0 is a root with no dependencies
        assert!(plan.dependencies_of(0).is_empty());
    }

    #[test]
    fn detect_cycle() {
        // Create a cycle: 0 -> 1 -> 2 -> 0
        // We can't do this naturally since the Planner would need
        // all deps registered first. Instead, we create a situation
        // where the graph has a back-edge by using a step that
        // declares a forward dependency.
        //
        // Steps: 0 depends on 2, 1 depends on 0, 2 depends on 1
        let mut planner = Planner::new(1);
        planner.register(MockStep::new(0, vec![2]));
        planner.register(MockStep::new(1, vec![0]));
        planner.register(MockStep::new(2, vec![1]));

        let result = planner.build();
        assert!(result.is_err());
        match result {
            Err(FwGraphError::PlanCycle(_)) => {} // expected
            other => panic!("expected PlanCycle, got {:?}", other),
        }
    }

    #[test]
    fn missing_dependency_is_error() {
        let mut planner = Planner::new(1);
        planner.register(MockStep::new(0, vec![]));
        // Step 1 depends on step 99, which is not registered
        planner.register(MockStep::new(1, vec![99]));

        let result = planner.build();
        assert!(result.is_err());
        match result {
            Err(FwGraphError::StepNotFound(99)) => {} // expected
            other => panic!("expected StepNotFound(99), got {:?}", other),
        }
    }

    #[test]
    fn single_step_plan() {
        let mut planner = Planner::new(5);
        planner.register(MockStep::new(0, vec![]));

        let plan = planner.build().expect("should build");
        assert_eq!(plan.step_count(), 1);
        assert_eq!(plan.batch_size, 5);
    }
}
