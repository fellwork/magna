//! Step fingerprinting for plan-time deduplication.
//!
//! Two steps are structurally identical if and only if their fingerprints are equal.
//! The fingerprint captures the step's concrete type, its dependency graph, and a
//! hash of its plan-time configuration. Runtime values (JWT claims, variables,
//! request IDs) must NEVER be included in the config hash.

use magna_types::StepId;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// A stable hash that identifies a step's structure at plan-time.
/// Two steps with equal fingerprints can share one execution -- their
/// output is computed once and distributed to all dependents.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StepFingerprint {
    /// TypeId of the concrete step implementation.
    /// Ensures different step types never merge even with the same deps.
    pub type_id: std::any::TypeId,

    /// IDs of dependency steps, in dependencies() order.
    /// Two steps with different dep graphs are never identical.
    /// Note: these are STEP IDs, not values -- plan-time only.
    pub dep_ids: Vec<StepId>,

    /// A hash of plan-time configuration parameters.
    /// Each step type is responsible for hashing its own config.
    /// Example: PgSelectStep hashes (schema, table, selected_columns[]).
    /// MUST NOT include runtime values (variables, JWT claims).
    /// If a step has no plan-time config, use 0u64.
    pub config_hash: u64,
}

impl StepFingerprint {
    /// Create a new fingerprint from a type ID, dependency list, and hashable config.
    pub fn new(type_id: std::any::TypeId, dep_ids: Vec<StepId>, config: impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        config.hash(&mut hasher);
        Self {
            type_id,
            dep_ids,
            config_hash: hasher.finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    struct FakeStepA;
    struct FakeStepB;

    #[test]
    fn same_type_same_deps_same_config_are_equal() {
        let fp1 = StepFingerprint::new(TypeId::of::<FakeStepA>(), vec![1, 2], "table_users");
        let fp2 = StepFingerprint::new(TypeId::of::<FakeStepA>(), vec![1, 2], "table_users");
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn different_types_are_not_equal() {
        let fp1 = StepFingerprint::new(TypeId::of::<FakeStepA>(), vec![1], 0u64);
        let fp2 = StepFingerprint::new(TypeId::of::<FakeStepB>(), vec![1], 0u64);
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn different_deps_are_not_equal() {
        let fp1 = StepFingerprint::new(TypeId::of::<FakeStepA>(), vec![1, 2], 0u64);
        let fp2 = StepFingerprint::new(TypeId::of::<FakeStepA>(), vec![1, 3], 0u64);
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn different_config_are_not_equal() {
        let fp1 = StepFingerprint::new(TypeId::of::<FakeStepA>(), vec![1], "config_a");
        let fp2 = StepFingerprint::new(TypeId::of::<FakeStepA>(), vec![1], "config_b");
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn fingerprint_is_hashable() {
        use std::collections::HashMap;
        let fp = StepFingerprint::new(TypeId::of::<FakeStepA>(), vec![1], 0u64);
        let mut map = HashMap::new();
        map.insert(fp.clone(), 42u32);
        assert_eq!(map.get(&fp), Some(&42));
    }
}
