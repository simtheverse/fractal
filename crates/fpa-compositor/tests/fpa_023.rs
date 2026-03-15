//! Tests for FPA-023: Dump/Load Operations.

use std::sync::Arc;

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::Counter;
use fpa_contract::StateContribution;

/// Dump invokes contribute_state() on all partitions.
#[test]
fn dump_invokes_contribute_state_on_all_partitions() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
        Box::new(Counter::new("b")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();
    compositor.run_tick(1.0).unwrap();

    let snapshot = compositor.dump().unwrap();
    let partitions_table = snapshot.get("partitions").unwrap().as_table().unwrap();

    // Both partitions should have contributed state with count = 2 (wrapped in StateContribution)
    let a_sc = StateContribution::from_toml(partitions_table.get("a").unwrap()).unwrap();
    let a_count = a_sc.state.get("count").unwrap().as_integer().unwrap();
    let b_sc = StateContribution::from_toml(partitions_table.get("b").unwrap()).unwrap();
    let b_count = b_sc.state.get("count").unwrap().as_integer().unwrap();

    assert_eq!(a_count, 2);
    assert_eq!(b_count, 2);
}

/// Load restores state via load_state().
#[test]
fn load_restores_state_via_load_state() {
    // Build a state fragment manually (using StateContribution envelope format)
    let state: toml::Value = toml::from_str(
        r#"
        [system]
        tick_count = 10

        [partitions.counter]
        fresh = true
        age_ms = 0
        [partitions.counter.state]
        count = 7
        "#,
    )
    .unwrap();

    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.pause().unwrap();
    compositor.load(state).unwrap();
    compositor.resume().unwrap();

    assert_eq!(compositor.tick_count(), 10);

    // Verify partition state was restored (dump wraps in StateContribution)
    let snapshot = compositor.dump().unwrap();
    let counter_sc = StateContribution::from_toml(
        snapshot.get("partitions").unwrap().get("counter").unwrap()
    ).unwrap();
    let count = counter_sc.state.get("count").unwrap().as_integer().unwrap();
    assert_eq!(count, 7);
}

/// Round-trip identity: init, run 5 ticks, dump, load into fresh compositor, compare states.
#[test]
fn round_trip_identity() {
    // Compositor 1: run 5 ticks
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut comp1 = Compositor::new(partitions, Arc::new(bus));

    comp1.init().unwrap();
    for _ in 0..5 {
        comp1.run_tick(1.0).unwrap();
    }

    let snapshot = comp1.dump().unwrap();

    // Compositor 2: load snapshot
    let partitions2: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus2 = InProcessBus::new("test-bus-2");
    let mut comp2 = Compositor::new(partitions2, Arc::new(bus2));

    comp2.init().unwrap();
    comp2.pause().unwrap();
    comp2.load(snapshot.clone()).unwrap();
    comp2.resume().unwrap();

    let snapshot2 = comp2.dump().unwrap();

    // States must match
    assert_eq!(snapshot, snapshot2);
    assert_eq!(comp2.tick_count(), 5);
}

/// Load is rejected while compositor is in Running state (FPA-023).
///
/// FPA-023: "Load shall be invocable only while processing is idle — specifically,
/// when no partition lifecycle methods are in flight AND the execution state machine
/// is in a non-processing state."
#[test]
fn load_while_running_is_rejected() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    assert_eq!(compositor.state(), fpa_compositor::state_machine::ExecutionState::Running);

    compositor.run_tick(1.0).unwrap();

    let state: toml::Value = toml::from_str(
        r#"
        [system]
        tick_count = 42

        [partitions.counter]
        fresh = true
        age_ms = 0
        [partitions.counter.state]
        count = 10
        "#,
    )
    .unwrap();

    // Load while Running must be rejected
    let result = compositor.load(state);
    assert!(result.is_err(), "load should be rejected while in Running state");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("Paused"),
        "error should mention Paused state requirement: {}",
        err.message
    );

    // Tick count should be unchanged
    assert_eq!(compositor.tick_count(), 1);
}

/// More rigorous round-trip: run N ticks, dump, load, run M more, compare with continuous N+M.
#[test]
fn round_trip_with_continued_execution() {
    let n = 5u64;
    let m = 3u64;

    // Compositor A: run N ticks, dump, load into fresh, run M more
    let partitions_a: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus_a = InProcessBus::new("bus-a");
    let mut comp_a = Compositor::new(partitions_a, Arc::new(bus_a));

    comp_a.init().unwrap();
    for _ in 0..n {
        comp_a.run_tick(1.0).unwrap();
    }

    let snapshot = comp_a.dump().unwrap();

    // Load into fresh compositor and continue running
    let partitions_a2: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus_a2 = InProcessBus::new("bus-a2");
    let mut comp_a2 = Compositor::new(partitions_a2, Arc::new(bus_a2));

    comp_a2.init().unwrap();
    comp_a2.pause().unwrap();
    comp_a2.load(snapshot).unwrap();
    comp_a2.resume().unwrap();

    for _ in 0..m {
        comp_a2.run_tick(1.0).unwrap();
    }

    // Compositor B: run N+M ticks continuously
    let partitions_b: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus_b = InProcessBus::new("bus-b");
    let mut comp_b = Compositor::new(partitions_b, Arc::new(bus_b));

    comp_b.init().unwrap();
    for _ in 0..(n + m) {
        comp_b.run_tick(1.0).unwrap();
    }

    // Both should have identical state
    let state_a2 = comp_a2.dump().unwrap();
    let state_b = comp_b.dump().unwrap();

    assert_eq!(state_a2, state_b);
    assert_eq!(comp_a2.tick_count(), n + m);
    assert_eq!(comp_b.tick_count(), n + m);
}

// --- Load validation edge cases ---

/// Load rejects partition state that is not a valid StateContribution envelope.
#[test]
fn load_rejects_invalid_state_contribution_envelope() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.pause().unwrap();

    // Partition entry is a bare table without state/fresh/age_ms fields
    let state: toml::Value = toml::from_str(
        r#"
        [system]
        tick_count = 5

        [partitions.counter]
        count = 7
        "#,
    )
    .unwrap();

    let result = compositor.load(state);
    assert!(result.is_err(), "load should reject bare table without StateContribution envelope");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("StateContribution") || err.message.contains("envelope"),
        "error should mention StateContribution: {}",
        err.message
    );
}

/// Load rejects negative tick_count.
#[test]
fn load_rejects_negative_tick_count() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.pause().unwrap();

    let state: toml::Value = toml::from_str(
        r#"
        [system]
        tick_count = -5

        [partitions.counter]
        fresh = true
        age_ms = 0
        [partitions.counter.state]
        count = 7
        "#,
    )
    .unwrap();

    let result = compositor.load(state);
    assert!(result.is_err(), "load should reject negative tick_count");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("negative") || err.message.contains("tick_count"),
        "error should mention negative tick_count: {}",
        err.message
    );
}

/// Load from Uninitialized state succeeds.
#[test]
fn load_from_uninitialized_succeeds() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    assert_eq!(compositor.state(), fpa_compositor::state_machine::ExecutionState::Uninitialized);

    let state: toml::Value = toml::from_str(
        r#"
        [system]
        tick_count = 3

        [partitions.counter]
        fresh = true
        age_ms = 0
        [partitions.counter.state]
        count = 3
        "#,
    )
    .unwrap();

    let result = compositor.load(state);
    assert!(result.is_ok(), "load from Uninitialized should succeed: {:?}", result.err());
    assert_eq!(compositor.tick_count(), 3);
}
