//! Re-exports from fpa-contract for backward compatibility.
//!
//! The state machine types are defined in fpa-contract (FPA-006) so that
//! partitions can observe execution state without depending on the compositor.

pub use fpa_contract::state_machine::{ExecutionState, StateMachine, TransitionError, TransitionRequest};
