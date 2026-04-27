//! Step types — the building blocks of the execution plan DAG.

use crate::error::StepError;

/// Unique identifier for a step within a plan.
/// Assigned by the Planner in registration order.
pub type StepId = u32;

/// The result of executing one step against one input item.
/// Flags propagate downstream — dependents are skipped when Null or Inhibited.
#[derive(Debug, Clone)]
pub enum StepResult<T> {
  /// A real value — dependents execute normally.
  Value(T),

  /// A propagating null — dependents that are not null-safe are skipped
  /// and return Null themselves without executing.
  Null,

  /// An error — propagates like Null, surfaces in the GraphQL error array.
  Error(StepError),

  /// An inhibitor flag — used for conditional fields (e.g. check-node results).
  /// Dependents receive this and choose how to handle it.
  Inhibited,
}

/// Flags carried alongside step output batches.
/// Stored as a bitfield per output slot for cache-friendly iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepFlags(pub u8);

impl StepFlags {
  pub const NONE:      StepFlags = StepFlags(0b0000);
  pub const NULL:      StepFlags = StepFlags(0b0001);
  pub const ERROR:     StepFlags = StepFlags(0b0010);
  pub const INHIBITED: StepFlags = StepFlags(0b0100);

  pub fn is_null(self)      -> bool { self.0 & 0b0001 != 0 }
  pub fn is_error(self)     -> bool { self.0 & 0b0010 != 0 }
  pub fn is_inhibited(self) -> bool { self.0 & 0b0100 != 0 }
  pub fn is_value(self)     -> bool { self.0 == 0 }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn step_flags_bitfield() {
    assert!(StepFlags::NONE.is_value());
    assert!(!StepFlags::NONE.is_null());

    assert!(StepFlags::NULL.is_null());
    assert!(!StepFlags::NULL.is_value());

    assert!(StepFlags::ERROR.is_error());
    assert!(StepFlags::INHIBITED.is_inhibited());

    // Combined flags
    let combined = StepFlags(StepFlags::NULL.0 | StepFlags::ERROR.0);
    assert!(combined.is_null());
    assert!(combined.is_error());
    assert!(!combined.is_value());
  }
}
