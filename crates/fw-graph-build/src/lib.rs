pub mod error;
pub mod gather;
pub mod ir;
pub mod naming;
pub mod plan_resolver;
pub mod smart_tags;
pub mod type_map;
pub mod union_step;

pub use error::BuildError;
pub use gather::gather;
pub use ir::{
    BehaviorSet, GatherOutput, ResolvedColumn, ResolvedEnum, ResolvedRelation, ResolvedResource,
    ResourceKind,
};
pub use plan_resolver::PlanContext;
pub use union_step::{PgUnionStep, TaggedRow};
