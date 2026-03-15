//! Shared state machine for compositor execution state (FPA-006).
//!
//! The state machine type and transition rules are defined in the contract
//! crate so that all partitions can observe execution state and submit
//! transition requests without depending on the compositor crate.

use std::cell::Cell;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::message::{DeliverySemantic, Message};

/// Execution states for the compositor lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionState {
    Uninitialized,
    Initializing,
    Running,
    Paused,
    ShuttingDown,
    Terminated,
    Error,
}

impl fmt::Display for ExecutionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionState::Uninitialized => write!(f, "Uninitialized"),
            ExecutionState::Initializing => write!(f, "Initializing"),
            ExecutionState::Running => write!(f, "Running"),
            ExecutionState::Paused => write!(f, "Paused"),
            ExecutionState::ShuttingDown => write!(f, "ShuttingDown"),
            ExecutionState::Terminated => write!(f, "Terminated"),
            ExecutionState::Error => write!(f, "Error"),
        }
    }
}

/// A request to transition the state machine to a new state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionRequest {
    pub requested_by: String,
    pub target_state: ExecutionState,
}

impl Message for TransitionRequest {
    const NAME: &'static str = "TransitionRequest";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::Queued;
}

/// Error returned when a state transition is invalid.
#[derive(Debug)]
pub struct TransitionError {
    pub from: ExecutionState,
    pub to: ExecutionState,
    pub reason: String,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid transition from {} to {}: {}",
            self.from, self.to, self.reason
        )
    }
}

impl std::error::Error for TransitionError {}

/// The shared state machine that governs compositor execution state.
///
/// Valid transitions:
/// - Uninitialized -> Initializing
/// - Initializing -> Running
/// - Initializing -> Error
/// - Running -> Paused
/// - Running -> ShuttingDown
/// - Running -> Error
/// - Paused -> Running
/// - Paused -> ShuttingDown
/// - ShuttingDown -> Terminated
/// - ShuttingDown -> Error
/// - Error -> ShuttingDown
/// Uses `Cell` for interior mutability so that `force_state` and
/// `request_transition` can be called from `&self` contexts (e.g.,
/// `Partition::contribute_state`). This is safe because `ExecutionState`
/// is `Copy` and `StateMachine` is not `Sync` (single-threaded access).
pub struct StateMachine {
    state: Cell<ExecutionState>,
}

impl StateMachine {
    /// Create a new state machine in the Uninitialized state.
    pub fn new() -> Self {
        Self {
            state: Cell::new(ExecutionState::Uninitialized),
        }
    }

    /// Get the current execution state.
    pub fn state(&self) -> ExecutionState {
        self.state.get()
    }

    /// Check whether a transition from the current state to the target is valid.
    pub fn is_valid_transition(&self, target: ExecutionState) -> bool {
        Self::valid_transitions(self.state.get()).contains(&target)
    }

    /// Request a state transition. Returns Ok if the transition is valid,
    /// or Err with a TransitionError if not.
    pub fn request_transition(
        &self,
        request: TransitionRequest,
    ) -> Result<ExecutionState, TransitionError> {
        let current = self.state.get();
        if Self::valid_transitions(current).contains(&request.target_state) {
            self.state.set(request.target_state);
            Ok(request.target_state)
        } else {
            Err(TransitionError {
                from: current,
                to: request.target_state,
                reason: format!(
                    "transition from {} to {} is not allowed (requested by '{}')",
                    current, request.target_state, request.requested_by
                ),
            })
        }
    }

    /// Force-set the state without transition validation.
    ///
    /// This is intended for the compositor's internal error handling — when a
    /// sub-partition faults, the compositor forces the state to Error without
    /// going through the normal request path. Partitions should use
    /// `request_transition` for all normal state changes.
    pub fn force_state(&self, state: ExecutionState) {
        self.state.set(state);
    }

    /// Returns valid target states from the given state.
    fn valid_transitions(from: ExecutionState) -> &'static [ExecutionState] {
        match from {
            ExecutionState::Uninitialized => &[ExecutionState::Initializing],
            ExecutionState::Initializing => &[ExecutionState::Running, ExecutionState::Error],
            ExecutionState::Running => &[
                ExecutionState::Paused,
                ExecutionState::ShuttingDown,
                ExecutionState::Error,
            ],
            ExecutionState::Paused => &[ExecutionState::Running, ExecutionState::ShuttingDown],
            ExecutionState::ShuttingDown => &[ExecutionState::Terminated, ExecutionState::Error],
            ExecutionState::Terminated => &[],
            ExecutionState::Error => &[ExecutionState::ShuttingDown],
        }
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}
