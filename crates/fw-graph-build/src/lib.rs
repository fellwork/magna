pub mod error;
pub mod gather;
pub mod ir;
pub mod naming;
pub mod smart_tags;
pub mod type_map;

pub use error::BuildError;
pub use gather::gather;
pub use ir::{
    BehaviorSet, GatherOutput, ResolvedColumn, ResolvedEnum, ResolvedRelation, ResolvedResource,
    ResourceKind,
};
