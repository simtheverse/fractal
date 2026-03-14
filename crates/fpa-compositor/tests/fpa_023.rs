//! Tests for FPA-023: Dump/Load Operations.

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::Counter;

/// Dump invokes contribute_state() on all partitions.
#[test]
fn dump_invokes_contribute_state_on_all_partitions() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
        Box::new(Counter::new("b")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();
    compositor.run_tick(1.0).unwrap();

    let snapshot = compositor.dump().unwrap();
    let partitions_table = snapshot.get("partitions").unwrap().as_table().unwrap();

    // Both partitions should have contributed state with count = 2
    let a_count = partitions_table
        .get("a")
        .unwrap()
        .get("count")
        .unwrap()
        .as_integer()
        .unwrap();
    let b_count = partitions_table
        .get("b")
        .unwrap()
        .get("count")
        .unwrap()
        .as_integer()
        .unwrap();

    assert_eq!(a_count, 2);
    assert_eq!(b_count, 2);
}

/// Load restores state via load_state().
#[test]
fn load_restores_state_via_load_state() {
    // Build a state fragment manually
    let state: toml::Value = toml::from_str(
        r#"
        [system]
        tick_count = 10

        [partitions.counter]
        count = 7
        "#,
    )
    .unwrap();

    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.init().unwrap();
    compositor.load(state).unwrap();

    assert_eq!(compositor.tick_count(), 10);

    // Verify partition state was restored
    let snapshot = compositor.dump().unwrap();
    let count = snapshot
        .get("partitions")
        .unwrap()
        .get("counter")
        .unwrap()
        .get("count")
        .unwrap()
        .as_integer()
        .unwrap();
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
    let mut comp1 = Compositor::new(partitions, Box::new(bus));

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
    let mut comp2 = Compositor::new(partitions2, Box::new(bus2));

    comp2.init().unwrap();
    comp2.load(snapshot.clone()).unwrap();

    let snapshot2 = comp2.dump().unwrap();

    // States must match
    assert_eq!(snapshot, snapshot2);
    assert_eq!(comp2.tick_count(), 5);
}

/// Load succeeds after init (Running state).
///
/// NOTE: FPA-023 specifies "Load shall be invocable only while processing is idle;
/// loading while partitions are actively stepping shall not be supported." The current
/// prototype does not enforce the idle-only constraint — `load()` succeeds in any state,
/// including Running. This test documents the current behavior. A production implementation
/// should reject load while partitions are actively stepping.
#[test]
fn load_while_running_succeeds_in_prototype() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.init().unwrap();
    // Compositor is now in Running state
    assert_eq!(compositor.state(), fpa_compositor::state_machine::ExecutionState::Running);

    // Run a tick to confirm we're actively running
    compositor.run_tick(1.0).unwrap();
    assert_eq!(compositor.tick_count(), 1);

    // Load state while in Running state — succeeds in the prototype
    let state: toml::Value = toml::from_str(
        r#"
        [system]
        tick_count = 42

        [partitions.counter]
        count = 10
        "#,
    )
    .unwrap();

    // FPA-023 spec says this should only work while idle, but the prototype allows it
    compositor.load(state).unwrap();
    assert_eq!(compositor.tick_count(), 42);
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
    let mut comp_a = Compositor::new(partitions_a, Box::new(bus_a));

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
    let mut comp_a2 = Compositor::new(partitions_a2, Box::new(bus_a2));

    comp_a2.init().unwrap();
    comp_a2.load(snapshot).unwrap();

    for _ in 0..m {
        comp_a2.run_tick(1.0).unwrap();
    }

    // Compositor B: run N+M ticks continuously
    let partitions_b: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus_b = InProcessBus::new("bus-b");
    let mut comp_b = Compositor::new(partitions_b, Box::new(bus_b));

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
