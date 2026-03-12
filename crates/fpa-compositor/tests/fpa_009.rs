//! Tests for FPA-009: Compositor Runtime.

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_compositor::state_machine::ExecutionState;
use fpa_contract::test_support::Counter;

/// Compositor controls partition lifecycle: init, step, shutdown.
#[test]
fn compositor_controls_partition_lifecycle() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
        Box::new(Counter::new("b")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, bus);

    // Init
    compositor.init().unwrap();
    assert_eq!(compositor.state(), ExecutionState::Running);

    // Step several ticks
    for _ in 0..3 {
        compositor.run_tick(1.0).unwrap();
    }
    assert_eq!(compositor.tick_count(), 3);

    // Shutdown
    compositor.shutdown().unwrap();
    assert_eq!(compositor.state(), ExecutionState::Terminated);
}

/// After init, state machine is Running.
#[test]
fn after_init_state_is_running() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, bus);

    assert_eq!(compositor.state(), ExecutionState::Uninitialized);
    compositor.init().unwrap();
    assert_eq!(compositor.state(), ExecutionState::Running);
}

/// After shutdown, state machine is Terminated.
#[test]
fn after_shutdown_state_is_terminated() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();
    compositor.shutdown().unwrap();
    assert_eq!(compositor.state(), ExecutionState::Terminated);
}

/// Compositor collects partition state into the double buffer each tick.
#[test]
fn shared_context_published_each_tick() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();

    // After tick 1, write buffer has counter output
    compositor.run_tick(1.0).unwrap();
    assert!(compositor.buffer().write_all().contains_key("counter"));

    // After tick 2, read buffer has tick-1 output (count=1)
    compositor.run_tick(1.0).unwrap();
    let read_val = compositor.buffer().read("counter").unwrap();
    let count = read_val
        .as_table()
        .unwrap()
        .get("count")
        .unwrap()
        .as_integer()
        .unwrap();
    assert_eq!(count, 1, "read buffer should have tick-1 output after tick-2 swap");
}

/// Compositor arbitrates transition requests.
#[test]
fn compositor_arbitrates_requests() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();

    // Valid request: Running -> Paused
    let result = compositor.process_transition_request(
        fpa_compositor::state_machine::TransitionRequest {
            requested_by: "test-partition".to_string(),
            target_state: ExecutionState::Paused,
        },
    );
    assert!(result.is_ok());
    assert_eq!(compositor.state(), ExecutionState::Paused);

    // Invalid request: Paused -> Terminated (not a valid transition)
    let result = compositor.process_transition_request(
        fpa_compositor::state_machine::TransitionRequest {
            requested_by: "test-partition".to_string(),
            target_state: ExecutionState::Terminated,
        },
    );
    assert!(result.is_err());
    assert_eq!(compositor.state(), ExecutionState::Paused);
}

/// Cannot run tick when not in Running state.
#[test]
fn cannot_run_tick_when_not_running() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, bus);

    // Not initialized yet - should fail
    let result = compositor.run_tick(1.0);
    assert!(result.is_err());
}

/// Tick count increments with each tick.
#[test]
fn tick_count_increments() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();
    assert_eq!(compositor.tick_count(), 0);

    compositor.run_tick(1.0).unwrap();
    assert_eq!(compositor.tick_count(), 1);

    compositor.run_tick(1.0).unwrap();
    assert_eq!(compositor.tick_count(), 2);
}
