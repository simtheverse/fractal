//! Tests for FPA-006: Shared State Machine.

use fpa_compositor::state_machine::{ExecutionState, StateMachine, TransitionRequest};

/// State machine starts in Uninitialized.
#[test]
fn starts_in_uninitialized() {
    let sm = StateMachine::new();
    assert_eq!(sm.state(), ExecutionState::Uninitialized);
}

/// Valid transition: Uninitialized -> Initializing succeeds.
#[test]
fn valid_transition_uninitialized_to_initializing() {
    let sm = StateMachine::new();
    let result = sm.request_transition(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Initializing,
    });
    assert!(result.is_ok());
    assert_eq!(sm.state(), ExecutionState::Initializing);
}

/// Invalid transition: Uninitialized -> Running (skipping Initializing) is rejected.
#[test]
fn invalid_transition_uninitialized_to_running_rejected() {
    let sm = StateMachine::new();
    let result = sm.request_transition(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Running,
    });
    assert!(result.is_err());
    // State should remain Uninitialized after rejected transition.
    assert_eq!(sm.state(), ExecutionState::Uninitialized);
}

/// State is observable: can read the current state at any time.
#[test]
fn state_is_observable() {
    let sm = StateMachine::new();
    assert_eq!(sm.state(), ExecutionState::Uninitialized);

    sm.request_transition(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Initializing,
    })
    .unwrap();
    assert_eq!(sm.state(), ExecutionState::Initializing);

    sm.request_transition(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Running,
    })
    .unwrap();
    assert_eq!(sm.state(), ExecutionState::Running);
}

/// Full lifecycle traversal: Uninitialized -> Initializing -> Running ->
/// ShuttingDown -> Terminated.
#[test]
fn full_lifecycle_transitions() {
    let sm = StateMachine::new();

    let states = [
        ExecutionState::Initializing,
        ExecutionState::Running,
        ExecutionState::ShuttingDown,
        ExecutionState::Terminated,
    ];

    for target in &states {
        sm.request_transition(TransitionRequest {
            requested_by: "test".to_string(),
            target_state: *target,
        })
        .unwrap();
    }

    assert_eq!(sm.state(), ExecutionState::Terminated);
}

/// Terminated is a terminal state: no transitions are valid from it.
#[test]
fn terminated_is_terminal() {
    let sm = StateMachine::new();
    // Go to Terminated
    for target in &[
        ExecutionState::Initializing,
        ExecutionState::Running,
        ExecutionState::ShuttingDown,
        ExecutionState::Terminated,
    ] {
        sm.request_transition(TransitionRequest {
            requested_by: "test".to_string(),
            target_state: *target,
        })
        .unwrap();
    }

    // Try every state from Terminated - all should fail
    for target in &[
        ExecutionState::Uninitialized,
        ExecutionState::Initializing,
        ExecutionState::Running,
        ExecutionState::Paused,
        ExecutionState::ShuttingDown,
        ExecutionState::Error,
    ] {
        let result = sm.request_transition(TransitionRequest {
            requested_by: "test".to_string(),
            target_state: *target,
        });
        assert!(result.is_err(), "transition from Terminated to {:?} should fail", target);
    }
}

/// Running can pause and resume.
#[test]
fn running_can_pause_and_resume() {
    let sm = StateMachine::new();
    sm.request_transition(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Initializing,
    })
    .unwrap();
    sm.request_transition(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Running,
    })
    .unwrap();

    // Pause
    sm.request_transition(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Paused,
    })
    .unwrap();
    assert_eq!(sm.state(), ExecutionState::Paused);

    // Resume
    sm.request_transition(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Running,
    })
    .unwrap();
    assert_eq!(sm.state(), ExecutionState::Running);
}

/// Error state can transition to ShuttingDown for recovery.
#[test]
fn error_can_transition_to_shutting_down() {
    let sm = StateMachine::new();
    sm.request_transition(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Initializing,
    })
    .unwrap();
    sm.request_transition(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Error,
    })
    .unwrap();
    assert_eq!(sm.state(), ExecutionState::Error);

    sm.request_transition(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::ShuttingDown,
    })
    .unwrap();
    assert_eq!(sm.state(), ExecutionState::ShuttingDown);
}
