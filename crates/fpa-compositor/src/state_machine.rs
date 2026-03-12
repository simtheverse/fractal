//! Shared state machine for compositor execution state (FPA-006).
//!
//! The state machine owns the authoritative execution state and enforces
//! valid transitions. It is owned by the compositor and observable as
//! read-only by all partitions.

use std::fmt;

/// Execution states for the compositor lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone)]
pub struct TransitionRequest {
    pub requested_by: String,
    pub target_state: ExecutionState,
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
pub struct StateMachine {
    state: ExecutionState,
}

impl StateMachine {
    /// Create a new state machine in the Uninitialized state.
    pub fn new() -> Self {
        Self {
            state: ExecutionState::Uninitialized,
        }
    }

    /// Get the current execution state.
    pub fn state(&self) -> ExecutionState {
        self.state
    }

    /// Check whether a transition from the current state to the target is valid.
    pub fn is_valid_transition(&self, target: ExecutionState) -> bool {
        Self::valid_transitions(self.state).contains(&target)
    }

    /// Request a state transition. Returns Ok if the transition is valid,
    /// or Err with a TransitionError if not.
    pub fn request_transition(
        &mut self,
        request: TransitionRequest,
    ) -> Result<ExecutionState, TransitionError> {
        if self.is_valid_transition(request.target_state) {
            self.state = request.target_state;
            Ok(self.state)
        } else {
            Err(TransitionError {
                from: self.state,
                to: request.target_state,
                reason: format!(
                    "transition from {} to {} is not allowed (requested by '{}')",
                    self.state, request.target_state, request.requested_by
                ),
            })
        }
    }

    /// Force-set the state (used internally by compositor for error handling).
    pub(crate) fn force_state(&mut self, state: ExecutionState) {
        self.state = state;
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
