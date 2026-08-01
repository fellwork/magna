//! [`BatchStep`] — the generic N+1 eliminator.
//!
//! Grafast's central trick is that a child field does not fetch per parent; it
//! declares *what it needs per key*, and the engine collects every key in the
//! batch, issues **one** load, and hands each parent its own slice back.
//! [`PgSelectStep`] implements that for Postgres tables it builds SQL against.
//! This is the same idea with the transport removed: you supply the loader.
//!
//! That matters for embedders whose reads must go through their own seam. An
//! application with a typed store layer, an HTTP backend, or a cache cannot
//! use a step that writes its own SQL — but it can supply an async closure
//! taking `Vec<K>` and returning `Vec<V>`.
//!
//! # The shape
//!
//! ```text
//!   parent step   values: [p0, p1, p2, …]        (N batch items)
//!        │
//!        │  key_of(p) ──▶ [k0, k1, k2, …]        keys, deduplicated
//!        ▼
//!   BatchStep      load(unique keys) ──▶ ONE call, all rows
//!        │         group_by(row) ──▶ rows bucketed per key
//!        ▼
//!                  values: [Vec<V> for p0, Vec<V> for p1, …]
//! ```
//!
//! Output is positional: `values[i]` holds the rows belonging to the parent at
//! `values[i]` of the dependency. A parent with no matching rows gets an empty
//! `Vec`, never a missing slot — dropping it would shift every later parent's
//! children onto the wrong parent.
//!
//! # Keys are deduplicated before loading
//!
//! Two parents naming the same key produce **one** entry in the load call and
//! both receive the same rows. This is distinct from the optimizer's
//! step-level deduplication (which merges whole steps with equal
//! fingerprints); this is *within* a single step's batch, and neither
//! subsumes the other.
//!
//! [`PgSelectStep`]: https://docs.rs/magna-dataplan

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::Arc;

use magna_types::{FwGraphError, StepId};

use crate::fingerprint::StepFingerprint;
use crate::step::{ExecutableStep, ExecutionContext, StepInputs, StepOutput};

/// The future a [`BatchLoader`] returns. Boxed so the loader can be stored in
/// a `dyn`-safe step.
pub type LoadFuture<V> = Pin<Box<dyn Future<Output = Result<Vec<V>, FwGraphError>> + Send>>;

/// Loads every row for a batch of keys in one call.
pub type BatchLoader<K, V> = Arc<dyn Fn(Vec<K>) -> LoadFuture<V> + Send + Sync>;

/// A step that resolves children for a batch of parent keys with a single
/// load, then partitions the result back per parent.
///
/// See the [module docs](self) for the shape and the guarantees.
pub struct BatchStep<K, V> {
    id: StepId,
    deps: Vec<StepId>,
    key_of: Arc<dyn Fn(&Arc<dyn Any + Send + Sync>) -> Option<K> + Send + Sync>,
    load: BatchLoader<K, V>,
    group_by: Arc<dyn Fn(&V) -> K + Send + Sync>,
    config_hash: u64,
    label: &'static str,
}

impl<K, V> BatchStep<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    /// Build a batched child step.
    ///
    /// * `parent` — the step whose output supplies the keys. Its values are
    ///   the batch; this step emits one value per parent value, in order.
    /// * `key_of` — the key for one parent value. Returning `None` means that
    ///   parent contributes no key and receives an empty result.
    /// * `load` — issues **one** call for the deduplicated keys.
    /// * `group_by` — the key a loaded row belongs to. Rows whose key was
    ///   never requested are dropped.
    /// * `config_hash` — what makes two of these interchangeable, beyond the
    ///   parent dependency. Fold in whatever parameterizes the load (a limit,
    ///   a language, a tier). Two `BatchStep`s over the same parent with the
    ///   same `config_hash` will be merged by the optimizer, so a value that
    ///   changes the rows MUST be represented here.
    pub fn new(
        id: StepId,
        parent: StepId,
        label: &'static str,
        key_of: impl Fn(&Arc<dyn Any + Send + Sync>) -> Option<K> + Send + Sync + 'static,
        load: impl Fn(Vec<K>) -> LoadFuture<V> + Send + Sync + 'static,
        group_by: impl Fn(&V) -> K + Send + Sync + 'static,
        config_hash: u64,
    ) -> Self {
        Self {
            id,
            deps: vec![parent],
            key_of: Arc::new(key_of),
            load: Arc::new(load),
            group_by: Arc::new(group_by),
            config_hash,
            label,
        }
    }

    /// Read one parent's children out of this step's output.
    ///
    /// `index` is the parent's position in the dependency's batch. `None`
    /// means the index is out of range or the value was not produced by this
    /// step — a wiring bug, not empty data.
    pub fn read(output: &StepOutput, index: usize) -> Option<&Vec<V>> {
        output.values.get(index)?.downcast_ref::<Vec<V>>()
    }
}

#[async_trait::async_trait]
impl<K, V> ExecutableStep for BatchStep<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    fn id(&self) -> StepId {
        self.id
    }

    fn dependencies(&self) -> &[StepId] {
        &self.deps
    }

    /// Not unary: this step produces one value per parent batch item.
    fn is_unary(&self) -> bool {
        false
    }

    fn fingerprint(&self) -> StepFingerprint {
        // The parent dependency rides in `dep_ids`, so two batch steps over
        // different parents never merge. `config_hash` carries everything
        // else that changes the rows.
        StepFingerprint::new(
            TypeId::of::<BatchStep<K, V>>(),
            self.deps.clone(),
            self.config_hash,
        )
    }

    async fn execute(
        &self,
        _ctx: &ExecutionContext,
        inputs: StepInputs,
    ) -> Result<StepOutput, FwGraphError> {
        let parent = inputs.dep_outputs.first().ok_or_else(|| {
            FwGraphError::ExecutionError(format!("{}: no parent output", self.label))
        })?;

        // One key slot per parent, preserving position. `None` parents keep
        // their slot so the output stays aligned with the batch.
        let per_parent: Vec<Option<K>> = parent.values.iter().map(|v| (self.key_of)(v)).collect();

        // Deduplicate for the load. Two parents naming the same key must not
        // cause two fetches — that is the whole point.
        let mut unique: Vec<K> = Vec::new();
        for key in per_parent.iter().flatten() {
            if !unique.iter().any(|k| k == key) {
                unique.push(key.clone());
            }
        }

        // Nothing to ask for: every parent gets an empty vec, no load issued.
        if unique.is_empty() {
            return Ok(StepOutput::from_values(
                per_parent
                    .iter()
                    .map(|_| Arc::new(Vec::<V>::new()) as Arc<dyn Any + Send + Sync>)
                    .collect(),
            ));
        }

        // THE single call.
        let rows = (self.load)(unique).await?;

        // Bucket by key, then hand each parent its bucket.
        let mut buckets: HashMap<K, Vec<V>> = HashMap::new();
        for row in rows {
            buckets.entry((self.group_by)(&row)).or_default().push(row);
        }

        // Buckets are SHARED, not moved: several parents may name the same
        // key (that is the deduplication above), and every one of them must
        // receive the rows. Taking the bucket out would give the first parent
        // the data and every later one an empty vec — silently, and only for
        // the duplicate case. Sharing via `Arc` also avoids requiring
        // `V: Clone` and copies nothing.
        let by_key: HashMap<K, Arc<Vec<V>>> = buckets
            .into_iter()
            .map(|(k, rows)| (k, Arc::new(rows)))
            .collect();
        let empty: Arc<Vec<V>> = Arc::new(Vec::new());

        let values: Vec<Arc<dyn Any + Send + Sync>> = per_parent
            .into_iter()
            .map(|key| {
                let bucket = key
                    .and_then(|k| by_key.get(&k).cloned())
                    .unwrap_or_else(|| Arc::clone(&empty));
                bucket as Arc<dyn Any + Send + Sync>
            })
            .collect();

        Ok(StepOutput::from_values(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::Planner;
    use crate::{executor::Executor, optimizer::optimize};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A parent emitting a fixed batch of ids.
    struct Parents(StepId, Vec<u32>);

    #[async_trait::async_trait]
    impl ExecutableStep for Parents {
        fn id(&self) -> StepId {
            self.0
        }
        fn dependencies(&self) -> &[StepId] {
            &[]
        }
        fn fingerprint(&self) -> StepFingerprint {
            StepFingerprint::new(TypeId::of::<Parents>(), vec![], 1)
        }
        async fn execute(
            &self,
            _c: &ExecutionContext,
            _i: StepInputs,
        ) -> Result<StepOutput, FwGraphError> {
            Ok(StepOutput::from_values(
                self.1
                    .iter()
                    .map(|id| Arc::new(*id) as Arc<dyn Any + Send + Sync>)
                    .collect(),
            ))
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Row {
        parent: u32,
        text: String,
    }

    /// Run a plan containing `Parents` + a `BatchStep`, returning the batch
    /// step's output and how many times the loader was called.
    async fn run(
        parent_ids: Vec<u32>,
        rows: Vec<Row>,
    ) -> (Arc<StepOutput>, usize, Arc<std::sync::Mutex<Vec<Vec<u32>>>>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen_keys = Arc::new(std::sync::Mutex::new(Vec::new()));

        let c = Arc::clone(&calls);
        let sk = Arc::clone(&seen_keys);
        let batch: BatchStep<u32, Row> = BatchStep::new(
            2,
            1,
            "test",
            |v| v.downcast_ref::<u32>().copied(),
            move |keys| {
                c.fetch_add(1, Ordering::SeqCst);
                sk.lock().unwrap().push(keys.clone());
                let rows = rows.clone();
                Box::pin(async move {
                    Ok(rows
                        .into_iter()
                        .filter(|r| keys.contains(&r.parent))
                        .collect())
                })
            },
            |r: &Row| r.parent,
            0,
        );

        let mut p = Planner::new(parent_ids.len().max(1));
        p.register(Arc::new(Parents(1, parent_ids)));
        p.register(Arc::new(batch));
        let opt = optimize(p.build().unwrap());
        let ctx = ExecutionContext {
            request_id: uuid::Uuid::nil(),
            jwt_claims: None,
            variables: Arc::new(serde_json::Value::Null),
        };
        let out = Executor::execute(&opt, Arc::new(ctx))
            .await
            .expect("plan executes");

        let batch_out = out.get(&2).expect("batch step produced output").clone();
        (batch_out, calls.load(Ordering::SeqCst), seen_keys)
    }

    /// The headline: N parents, ONE load.
    #[tokio::test]
    async fn n_parents_produce_one_load() {
        let rows = vec![
            Row { parent: 10, text: "a".into() },
            Row { parent: 20, text: "b".into() },
            Row { parent: 30, text: "c".into() },
        ];
        let (out, calls, _) = run(vec![10, 20, 30], rows).await;

        assert_eq!(calls, 1, "three parents must cost ONE load, not three");
        assert_eq!(out.values.len(), 3, "one value per parent");
    }

    /// Each parent gets its OWN rows — the partition must not smear.
    #[tokio::test]
    async fn each_parent_receives_only_its_own_rows() {
        let rows = vec![
            Row { parent: 10, text: "a1".into() },
            Row { parent: 10, text: "a2".into() },
            Row { parent: 30, text: "c".into() },
        ];
        let (out, _, _) = run(vec![10, 20, 30], rows).await;

        let p10 = BatchStep::<u32, Row>::read(&out, 0).unwrap();
        let p20 = BatchStep::<u32, Row>::read(&out, 1).unwrap();
        let p30 = BatchStep::<u32, Row>::read(&out, 2).unwrap();

        assert_eq!(p10.len(), 2, "parent 10 owns both of its rows");
        assert!(p10.iter().all(|r| r.parent == 10));
        // A parent with no rows gets an EMPTY vec in its own slot — not a
        // missing slot, which would shift parent 30's rows onto parent 20.
        assert!(p20.is_empty(), "parent with no rows keeps an empty slot");
        assert_eq!(p30.len(), 1);
        assert_eq!(p30[0].text, "c");
    }

    /// Repeated keys are asked for once and answered for both parents.
    #[tokio::test]
    async fn duplicate_keys_are_loaded_once_and_shared() {
        let rows = vec![Row { parent: 10, text: "shared".into() }];
        let (out, calls, keys) = run(vec![10, 10, 10], rows).await;

        assert_eq!(calls, 1);
        assert_eq!(
            keys.lock().unwrap()[0],
            vec![10],
            "the key must be requested ONCE even with three parents naming it",
        );
        for i in 0..3 {
            let rows = BatchStep::<u32, Row>::read(&out, i).unwrap();
            assert_eq!(rows.len(), 1, "every parent naming the key gets the rows");
        }
    }

    /// No keys ⇒ no load at all.
    #[tokio::test]
    async fn empty_batch_issues_no_load() {
        let (out, calls, _) = run(vec![], vec![]).await;
        assert_eq!(calls, 0, "an empty batch must not hit the backend");
        assert_eq!(out.values.len(), 0);
    }

    /// Rows the loader returns for keys nobody asked about are discarded
    /// rather than attached to an arbitrary parent.
    #[tokio::test]
    async fn unrequested_rows_are_dropped() {
        let rows = vec![
            Row { parent: 10, text: "wanted".into() },
            Row { parent: 99, text: "never asked for".into() },
        ];
        let (out, _, _) = run(vec![10], rows).await;

        let p10 = BatchStep::<u32, Row>::read(&out, 0).unwrap();
        assert_eq!(p10.len(), 1);
        assert_eq!(p10[0].text, "wanted");
    }
}
