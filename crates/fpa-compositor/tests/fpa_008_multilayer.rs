//! Tests for FPA-008 multilayer: Layer 1 messages are not visible on Layer 0 bus.
//!
//! Each compositor owns its own bus. When a compositor is nested inside another,
//! the inner bus is distinct from the outer bus.

use fpa_bus::{Bus, InProcessBus};
use fpa_compositor::compositor::{Compositor, SharedContext};
use fpa_contract::test_support::Counter;

/// Layer 1 SharedContext is published on Layer 1 bus, not Layer 0 bus.
#[test]
fn inner_bus_messages_not_visible_on_outer_bus() {
    // Create inner compositor (layer 1) with its own bus
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("b1")),
    ];
    let inner_bus = InProcessBus::new("layer-1-bus");
    let inner = Compositor::new(inner_partitions, inner_bus)
        .with_id("B")
        .with_layer_depth(1);

    // Create outer compositor (layer 0) with its own bus
    let outer_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(inner),
    ];
    let outer_bus = InProcessBus::new("layer-0-bus");

    // Subscribe to SharedContext on the outer bus BEFORE creating compositor
    // (so we can observe what gets published)
    let mut outer_reader = outer_bus.subscribe::<SharedContext>();

    let mut outer = Compositor::new(outer_partitions, outer_bus)
        .with_id("orchestrator");

    outer.init().unwrap();
    outer.run_tick(1.0).unwrap();

    // The outer bus should have a SharedContext published by the outer compositor
    let outer_ctx = outer_reader.read();
    assert!(
        outer_ctx.is_some(),
        "outer bus should have a SharedContext from the outer compositor"
    );
    let ctx = outer_ctx.unwrap();

    // The outer SharedContext should contain entries for "A" and "B" (the outer partition IDs)
    let state_table = ctx.state.as_table().unwrap();
    assert!(
        state_table.contains_key("A"),
        "outer SharedContext should contain partition A"
    );
    assert!(
        state_table.contains_key("B"),
        "outer SharedContext should contain partition B"
    );

    // But the outer SharedContext should NOT contain "b1" as a top-level key
    // (b1 is an inner partition, its messages stay on the inner bus)
    assert!(
        !state_table.contains_key("b1"),
        "inner partition b1 should NOT appear at top level of outer SharedContext"
    );
}

/// Each compositor bus has a distinct ID, proving bus isolation.
#[test]
fn bus_ids_are_distinct() {
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("b1")),
    ];
    let inner_bus = InProcessBus::new("layer-1-bus");
    let inner_bus_id = inner_bus.id().to_string();
    let inner = Compositor::new(inner_partitions, inner_bus)
        .with_id("B")
        .with_layer_depth(1);

    let outer_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(inner),
    ];
    let outer_bus = InProcessBus::new("layer-0-bus");
    let outer_bus_id = outer_bus.id().to_string();

    let _outer = Compositor::new(outer_partitions, outer_bus)
        .with_id("orchestrator");

    assert_ne!(
        inner_bus_id, outer_bus_id,
        "inner and outer bus should have different IDs"
    );
}

/// Positive test: inner compositor's SharedContext is published on its own bus,
/// NOT on the outer bus. Subscribing to SharedContext on the outer bus yields
/// only the outer compositor's context. The inner bus is isolated.
#[test]
fn inner_shared_context_does_not_appear_on_outer_bus() {
    // Create inner compositor (layer 1) with its own bus
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("b1")),
    ];
    let inner_bus = InProcessBus::new("layer-1-bus");
    let inner = Compositor::new(inner_partitions, inner_bus)
        .with_id("B")
        .with_layer_depth(1);

    // Create outer compositor (layer 0) with its own bus
    let outer_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(inner),
    ];
    let outer_bus = InProcessBus::new("layer-0-bus");
    let mut outer_reader = outer_bus.subscribe::<SharedContext>();

    let mut outer = Compositor::new(outer_partitions, outer_bus)
        .with_id("orchestrator");

    outer.init().unwrap();
    outer.run_tick(1.0).unwrap();

    // Read the single SharedContext published on the outer bus
    let ctx = outer_reader.read().expect("outer bus should have SharedContext");

    // Outer SharedContext should have top-level keys "A" and "B" only
    let state_table = ctx.state.as_table().unwrap();
    assert_eq!(
        state_table.len(),
        2,
        "outer SharedContext should have exactly 2 entries (A and B)"
    );
    assert!(state_table.contains_key("A"));
    assert!(state_table.contains_key("B"));

    // The value for "B" is the compositor's dump (contains "partitions" and "system"),
    // NOT the inner SharedContext. The inner SharedContext only lives on the inner bus.
    let b_value = state_table.get("B").unwrap().as_table().unwrap();
    assert!(
        b_value.contains_key("partitions"),
        "B's state should be a compositor dump with 'partitions' key"
    );
    assert!(
        b_value.contains_key("system"),
        "B's state should be a compositor dump with 'system' key"
    );
}

/// Two-layer scenario: inner compositor tick count is independent from outer.
#[test]
fn inner_and_outer_tick_counts_independent() {
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

    // Run 3 outer ticks (each outer tick causes 1 inner tick via step delegation)
    for _ in 0..3 {
        outer.run_tick(1.0).unwrap();
    }

    assert_eq!(outer.tick_count(), 3);

    // Inner compositor (partition B) ran its own independent tick count
    // Verify through state: B's contribute_state includes tick_count
    let state = outer.dump().unwrap();
    let b_state = state
        .as_table().unwrap()
        ["partitions"]
        .as_table().unwrap()
        ["B"]
        .as_table().unwrap();

    let inner_tick = b_state["system"]
        .as_table().unwrap()
        ["tick_count"]
        .as_integer().unwrap();

    assert_eq!(inner_tick, 3, "inner compositor should also have 3 ticks (one per outer tick)");
}
