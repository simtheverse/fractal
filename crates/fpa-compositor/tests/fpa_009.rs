//! Tests for FPA-009: Compositor Runtime.

use std::sync::Arc;

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::{Compositor, LifecycleOp};
use fpa_compositor::state_machine::ExecutionState;
use fpa_contract::test_support::Counter;
use fpa_contract::StateContribution;

/// Compositor controls partition lifecycle: init, step, shutdown.
#[test]
fn compositor_controls_partition_lifecycle() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
        Box::new(Counter::new("b")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

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
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

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
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

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
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();

    // After tick 1, write buffer has counter output
    compositor.run_tick(1.0).unwrap();
    assert!(compositor.buffer().write_all().contains_key("counter"));

    // After tick 2, read buffer has tick-1 output (count=1)
    compositor.run_tick(1.0).unwrap();
    let read_val = compositor.buffer().read("counter").unwrap();
    let sc = StateContribution::from_toml(read_val).expect("should be a StateContribution");
    let count = sc.state
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
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

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
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

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
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    assert_eq!(compositor.tick_count(), 0);

    compositor.run_tick(1.0).unwrap();
    assert_eq!(compositor.tick_count(), 1);

    compositor.run_tick(1.0).unwrap();
    assert_eq!(compositor.tick_count(), 2);
}

// --- Despawn lifecycle tests ---

/// Despawn removes a partition from the compositor.
#[test]
fn despawn_removes_partition() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
        Box::new(Counter::new("b")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    compositor.request_lifecycle_op(LifecycleOp::Despawn("a".to_string()));
    compositor.run_tick(1.0).unwrap();

    let snapshot = compositor.dump().unwrap();
    let partitions_table = snapshot.get("partitions").unwrap().as_table().unwrap();
    assert!(!partitions_table.contains_key("a"), "partition 'a' should have been despawned");
    assert!(partitions_table.contains_key("b"), "partition 'b' should still exist");
}

/// Despawn of a nonexistent partition ID is silently ignored.
#[test]
fn despawn_nonexistent_is_silent() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.request_lifecycle_op(LifecycleOp::Despawn("nonexistent".to_string()));
    let result = compositor.run_tick(1.0);
    assert!(result.is_ok(), "despawning nonexistent partition should not error");
}

// --- Spawn lifecycle tests (FPA-014) ---

/// Spawn adds a partition to the compositor.
#[test]
fn spawn_adds_partition() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.request_lifecycle_op(LifecycleOp::Spawn(Box::new(Counter::new("new"))));
    compositor.run_tick(1.0).unwrap();

    let snapshot = compositor.dump().unwrap();
    let partitions_table = snapshot.get("partitions").unwrap().as_table().unwrap();
    assert!(partitions_table.contains_key("new"), "spawned partition 'new' should be in dump");
    assert!(partitions_table.contains_key("a"), "original partition 'a' should still exist");

    // The spawned partition should have stepped once (count=1)
    let new_sc = StateContribution::from_toml(&partitions_table["new"]).unwrap();
    let count = new_sc.state.as_table().unwrap().get("count").unwrap().as_integer().unwrap();
    assert_eq!(count, 1, "spawned partition should have stepped once in the same tick");
}

/// Spawn during Phase 1 means the partition steps in the same tick's Phase 2.
#[test]
fn spawn_partition_steps_in_same_tick() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.request_lifecycle_op(LifecycleOp::Spawn(Box::new(Counter::new("spawned"))));
    compositor.run_tick(1.0).unwrap();

    // After one tick, the spawned partition should have count=1
    let snapshot = compositor.dump().unwrap();
    let partitions_table = snapshot.get("partitions").unwrap().as_table().unwrap();
    let sc = StateContribution::from_toml(&partitions_table["spawned"]).unwrap();
    let count = sc.state.as_table().unwrap().get("count").unwrap().as_integer().unwrap();
    assert_eq!(count, 1, "spawn in Phase 1 means partition steps in Phase 2 of the same tick");
}

/// Spawn a partition whose init fails transitions compositor to Error state.
#[test]
fn spawn_init_failure_transitions_to_error() {
    use fpa_contract::{Partition, PartitionError};

    struct FailOnInit {
        id: String,
    }

    impl Partition for FailOnInit {
        fn id(&self) -> &str { &self.id }
        fn init(&mut self) -> Result<(), PartitionError> {
            Err(PartitionError::new(&self.id, "init", "intentional init failure"))
        }
        fn step(&mut self, _dt: f64) -> Result<(), PartitionError> { Ok(()) }
        fn shutdown(&mut self) -> Result<(), PartitionError> { Ok(()) }
        fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
            Ok(toml::Value::Table(toml::map::Map::new()))
        }
        fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> { Ok(()) }
    }

    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.request_lifecycle_op(LifecycleOp::Spawn(Box::new(FailOnInit {
        id: "bad".to_string(),
    })));

    let result = compositor.run_tick(1.0);
    assert!(result.is_err(), "spawn of a partition with failing init should error");
    assert_eq!(compositor.state(), ExecutionState::Error);
}

/// Spawn then despawn round trip: partition appears then disappears.
#[test]
fn spawn_then_despawn_round_trip() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();

    // Spawn "x"
    compositor.request_lifecycle_op(LifecycleOp::Spawn(Box::new(Counter::new("x"))));
    compositor.run_tick(1.0).unwrap();

    let snapshot = compositor.dump().unwrap();
    assert!(
        snapshot.get("partitions").unwrap().as_table().unwrap().contains_key("x"),
        "partition 'x' should exist after spawn"
    );

    // Despawn "x"
    compositor.request_lifecycle_op(LifecycleOp::Despawn("x".to_string()));
    compositor.run_tick(1.0).unwrap();

    let snapshot = compositor.dump().unwrap();
    assert!(
        !snapshot.get("partitions").unwrap().as_table().unwrap().contains_key("x"),
        "partition 'x' should be gone after despawn"
    );
    assert!(
        snapshot.get("partitions").unwrap().as_table().unwrap().contains_key("a"),
        "original partition 'a' should still exist"
    );
}

// --- State freshness tests (FPA-009) ---

/// Lock-step compositor always produces fresh=true, age_ms=0 state contributions.
#[test]
fn lock_step_state_contribution_always_fresh() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
        Box::new(Counter::new("b")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();

    // Run 3 ticks
    for _ in 0..3 {
        compositor.run_tick(1.0).unwrap();
    }

    let snapshot = compositor.dump().unwrap();
    let partitions_table = snapshot.get("partitions").unwrap().as_table().unwrap();

    for (id, value) in partitions_table {
        let sc = StateContribution::from_toml(value)
            .unwrap_or_else(|| panic!("partition '{}' should have a valid StateContribution", id));
        assert!(sc.fresh, "partition '{}' should have fresh=true in lock-step", id);
        assert_eq!(sc.age_ms, 0, "partition '{}' should have age_ms=0 in lock-step", id);
    }
}

// --- SharedContext bus subscription test (FPA-009) ---

/// SharedContext is received via standard bus subscription, not a special path.
#[test]
fn shared_context_received_via_standard_bus_subscription() {
    use fpa_bus::{BusExt, BusReader};
    use fpa_compositor::compositor::SharedContext;

    let bus = Arc::new(InProcessBus::new("test-bus"));
    let mut reader = bus.subscribe::<SharedContext>();

    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();
    compositor.run_tick(1.0).unwrap();

    // Read the latest SharedContext from the bus
    let ctx = reader.read().expect("SharedContext should be published on the bus");
    assert_eq!(ctx.tick, 2, "tick should reflect the latest tick count");
    assert_eq!(ctx.execution_state, ExecutionState::Running);

    // Verify state table contains partition data
    let table = ctx.state.as_table().expect("state should be a table");
    assert!(table.contains_key("counter"), "state should contain partition 'counter'");
}
