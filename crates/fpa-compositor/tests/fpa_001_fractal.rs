//! Tests for FPA-001: Fractal nesting — 3-layer depth verification.
//!
//! Verifies that compositors can be nested 3 levels deep, that state
//! contribution is properly nested, and that tick propagation works
//! at all layers.

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::Counter;

/// Build a 3-layer compositor hierarchy:
///
/// Layer 0 (orchestrator):
///   - Partition A: Counter
///   - Partition B: Compositor (layer 1)
///       - Partition B1: Counter
///       - Partition B2: Compositor (layer 2)
///           - Partition B2a: Counter
#[test]
fn three_layer_nesting_state_and_ticks() {
    // Layer 2: innermost compositor containing B2a
    let b2_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("B2a")),
    ];
    let b2_bus = InProcessBus::new("layer-2-bus");
    let b2 = Compositor::new(b2_partitions, b2_bus)
        .with_id("B2")
        .with_layer_depth(2);

    // Layer 1: middle compositor containing B1 and B2
    let b_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("B1")),
        Box::new(b2),
    ];
    let b_bus = InProcessBus::new("layer-1-bus");
    let b = Compositor::new(b_partitions, b_bus)
        .with_id("B")
        .with_layer_depth(1);

    // Layer 0: orchestrator containing A and B
    let outer_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(b),
    ];
    let outer_bus = InProcessBus::new("layer-0-bus");
    let mut orchestrator = Compositor::new(outer_partitions, outer_bus)
        .with_id("orchestrator");

    orchestrator.init().unwrap();

    // Run 3 ticks
    for _ in 0..3 {
        orchestrator.run_tick(1.0).unwrap();
    }

    // Verify state contribution is properly nested 3 levels
    let state = orchestrator.dump().unwrap();
    let root = state.as_table().unwrap();
    let partitions = root["partitions"].as_table().unwrap();

    // Layer 0: partition A should have count = 3
    let a_state = partitions["A"].as_table().unwrap();
    assert_eq!(
        a_state["count"].as_integer().unwrap(),
        3,
        "Layer 0 Counter A should have count 3 after 3 ticks"
    );

    // Layer 0: partition B is a compositor, its state is a dump with partitions + system
    let b_state = partitions["B"].as_table().unwrap();
    let b_partitions_state = b_state["partitions"].as_table().unwrap();
    let b_system = b_state["system"].as_table().unwrap();
    assert_eq!(
        b_system["tick_count"].as_integer().unwrap(),
        3,
        "Layer 1 compositor B should have tick_count 3"
    );

    // Layer 1: partition B1 (counter) should have count = 3
    let b1_state = b_partitions_state["B1"].as_table().unwrap();
    assert_eq!(
        b1_state["count"].as_integer().unwrap(),
        3,
        "Layer 1 Counter B1 should have count 3 after 3 ticks"
    );

    // Layer 1: partition B2 is itself a compositor
    let b2_state = b_partitions_state["B2"].as_table().unwrap();
    let b2_partitions_state = b2_state["partitions"].as_table().unwrap();
    let b2_system = b2_state["system"].as_table().unwrap();
    assert_eq!(
        b2_system["tick_count"].as_integer().unwrap(),
        3,
        "Layer 2 compositor B2 should have tick_count 3"
    );

    // Layer 2: partition B2a (counter) should have count = 3
    let b2a_state = b2_partitions_state["B2a"].as_table().unwrap();
    assert_eq!(
        b2a_state["count"].as_integer().unwrap(),
        3,
        "Layer 2 Counter B2a should have count 3 after 3 ticks"
    );
}
