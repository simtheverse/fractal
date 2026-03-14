//! Tests for FPA-012: Recursive state contribution — nested TOML fragment,
//! outer layer sees one contribution per partition.

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::Counter;

/// A compositor-as-partition contributes a nested TOML fragment.
#[test]
fn compositor_partition_contributes_nested_state() {
    // Inner compositor with two counters
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("b1")),
        Box::new(Counter::new("b2")),
    ];
    let inner_bus = InProcessBus::new("inner-bus");
    let mut inner = Compositor::new(inner_partitions, inner_bus)
        .with_id("B")
        .with_layer_depth(1);
    inner.init().unwrap();
    inner.run_tick(1.0).unwrap();

    // Inner compositor produces a nested fragment with partitions + system
    let state = inner.dump().unwrap();
    let table = state.as_table().unwrap();
    assert!(table.contains_key("partitions"), "dump should contain partitions key");
    assert!(table.contains_key("system"), "dump should contain system key");

    let partitions = table["partitions"].as_table().unwrap();
    assert!(partitions.contains_key("b1"), "should have partition b1");
    assert!(partitions.contains_key("b2"), "should have partition b2");
}

/// Outer compositor sees one contribution per inner compositor (not its sub-partitions).
#[test]
fn outer_sees_one_contribution_per_inner_compositor() {
    // Create inner compositor as a partition
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("b1")),
        Box::new(Counter::new("b2")),
    ];
    let inner_bus = InProcessBus::new("inner-bus");
    let inner = Compositor::new(inner_partitions, inner_bus)
        .with_id("B")
        .with_layer_depth(1);

    // Outer compositor with Counter A and Compositor B
    let outer_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(inner),
    ];
    let outer_bus = InProcessBus::new("outer-bus");
    let mut outer = Compositor::new(outer_partitions, outer_bus)
        .with_id("orchestrator");

    outer.init().unwrap();
    outer.run_tick(1.0).unwrap();

    // Outer dump should have exactly two top-level partition entries: A and B
    let outer_state = outer.dump().unwrap();
    let outer_table = outer_state.as_table().unwrap();
    let outer_partitions = outer_table["partitions"].as_table().unwrap();

    assert_eq!(outer_partitions.len(), 2, "outer should see exactly 2 partitions");
    assert!(outer_partitions.contains_key("A"), "should have partition A");
    assert!(outer_partitions.contains_key("B"), "should have partition B");

    // B's contribution should itself be a nested table with partitions/system
    let b_state = outer_partitions["B"].as_table().unwrap();
    assert!(b_state.contains_key("partitions"), "B should have nested partitions");
}

/// Round-trip: dump then load preserves nested state.
#[test]
fn recursive_state_round_trip() {
    // Build two-layer structure
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("b1")),
    ];
    let inner_bus = InProcessBus::new("inner-bus");
    let inner = Compositor::new(inner_partitions, inner_bus)
        .with_id("B")
        .with_layer_depth(1);

    let outer_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(inner),
    ];
    let outer_bus = InProcessBus::new("outer-bus");
    let mut outer = Compositor::new(outer_partitions, outer_bus)
        .with_id("orchestrator");

    outer.init().unwrap();
    // Run 3 ticks to build up state
    for _ in 0..3 {
        outer.run_tick(1.0).unwrap();
    }

    // Dump state
    let snapshot = outer.dump().unwrap();

    // Create a fresh identical structure
    let inner2_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("b1")),
    ];
    let inner2_bus = InProcessBus::new("inner-bus-2");
    let inner2 = Compositor::new(inner2_partitions, inner2_bus)
        .with_id("B")
        .with_layer_depth(1);

    let outer2_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(inner2),
    ];
    let outer2_bus = InProcessBus::new("outer-bus-2");
    let mut outer2 = Compositor::new(outer2_partitions, outer2_bus)
        .with_id("orchestrator");

    // Load the snapshot
    outer2.load(snapshot.clone()).unwrap();

    // Verify round-trip identity
    let snapshot2 = outer2.dump().unwrap();
    assert_eq!(snapshot, snapshot2, "dump/load round-trip should preserve state");
}
