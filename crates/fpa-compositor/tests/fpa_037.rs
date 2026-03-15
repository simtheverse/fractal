//! FPA-037: Compositor Tests Assert Compositional Properties
//!
//! These tests assert compositional properties — delivery, conservation, ordering —
//! that hold regardless of which partition implementation is used. Swapping
//! Counter for Accumulator or Doubler must not cause these tests to fail.

use std::sync::Arc;

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_compositor::state_machine::ExecutionState;
use fpa_contract::Partition;
use fpa_contract::test_support::{Accumulator, Counter, Doubler};

// ---------------------------------------------------------------------------
// Helper: run a compositor lifecycle with any set of partitions
// ---------------------------------------------------------------------------

fn make_compositor(partitions: Vec<Box<dyn Partition>>) -> Compositor {
    let bus = InProcessBus::new("test-bus");
    Compositor::new(partitions, Arc::new(bus))
}

// ---------------------------------------------------------------------------
// Compositional property: DELIVERY
//
// After init + N ticks, every partition's state appears in the write buffer.
// This holds regardless of partition implementation.
// ---------------------------------------------------------------------------

fn assert_delivery_property(partitions: Vec<Box<dyn Partition>>, tick_count: usize) {
    let ids: Vec<String> = partitions.iter().map(|p| p.id().to_string()).collect();
    let mut comp = make_compositor(partitions);
    comp.init().unwrap();

    for _ in 0..tick_count {
        comp.run_tick(1.0 / 60.0).unwrap();
    }

    // Every partition's state is delivered to the write buffer
    let write_buf = comp.buffer().write_all();
    for id in &ids {
        assert!(
            write_buf.contains_key(id),
            "partition '{}' state should be delivered to the write buffer",
            id
        );
    }

    // Every delivered state is a valid, non-empty TOML table
    for id in &ids {
        let state = &write_buf[id];
        assert!(state.is_table(), "partition '{}' state should be a TOML table", id);
        assert!(
            !state.as_table().unwrap().is_empty(),
            "partition '{}' state table should be non-empty",
            id
        );
    }

    comp.shutdown().unwrap();
}

/// Delivery property holds with Counter partitions.
#[test]
fn delivery_with_counters() {
    assert_delivery_property(
        vec![
            Box::new(Counter::new("a")),
            Box::new(Counter::new("b")),
        ],
        5,
    );
}

/// Delivery property holds with Accumulator partitions.
#[test]
fn delivery_with_accumulators() {
    assert_delivery_property(
        vec![
            Box::new(Accumulator::new("a")),
            Box::new(Accumulator::new("b")),
        ],
        5,
    );
}

/// Delivery property holds with Doubler partitions.
#[test]
fn delivery_with_doublers() {
    assert_delivery_property(
        vec![
            Box::new(Doubler::new("a")),
            Box::new(Doubler::new("b")),
        ],
        5,
    );
}

/// Delivery property holds with mixed implementations.
#[test]
fn delivery_with_mixed_impls() {
    assert_delivery_property(
        vec![
            Box::new(Counter::new("counter")),
            Box::new(Accumulator::new("accum")),
            Box::new(Doubler::new("doubler")),
        ],
        5,
    );
}

// ---------------------------------------------------------------------------
// Compositional property: CONSERVATION
//
// The number of partitions is conserved across the lifecycle. The compositor
// does not add or remove partitions during normal operation.
// ---------------------------------------------------------------------------

fn assert_conservation_property(partitions: Vec<Box<dyn Partition>>, tick_count: usize) {
    let initial_count = partitions.len();
    let mut comp = make_compositor(partitions);
    comp.init().unwrap();

    for _ in 0..tick_count {
        comp.run_tick(1.0 / 60.0).unwrap();
        assert_eq!(
            comp.partitions().len(),
            initial_count,
            "partition count should be conserved across ticks"
        );
    }

    // After multiple ticks, write buffer has exactly as many entries as partitions
    let write_buf = comp.buffer().write_all();
    assert_eq!(
        write_buf.len(),
        initial_count,
        "write buffer should have one entry per partition"
    );

    comp.shutdown().unwrap();
}

/// Conservation property holds with Counters.
#[test]
fn conservation_with_counters() {
    assert_conservation_property(
        vec![
            Box::new(Counter::new("a")),
            Box::new(Counter::new("b")),
            Box::new(Counter::new("c")),
        ],
        5,
    );
}

/// Conservation property holds with mixed impls.
#[test]
fn conservation_with_mixed_impls() {
    assert_conservation_property(
        vec![
            Box::new(Counter::new("counter")),
            Box::new(Accumulator::new("accum")),
            Box::new(Doubler::new("doubler")),
        ],
        5,
    );
}

// ---------------------------------------------------------------------------
// Compositional property: ORDERING
//
// The compositor processes partitions in a deterministic order (insertion order).
// The tick count monotonically increases. State machine transitions follow
// the defined lifecycle ordering.
// ---------------------------------------------------------------------------

fn assert_ordering_property(partitions: Vec<Box<dyn Partition>>, tick_count: usize) {
    let ids: Vec<String> = partitions.iter().map(|p| p.id().to_string()).collect();
    let mut comp = make_compositor(partitions);

    // Lifecycle ordering: Uninitialized -> Running
    assert_eq!(comp.state(), ExecutionState::Uninitialized);
    comp.init().unwrap();
    assert_eq!(comp.state(), ExecutionState::Running);

    // Tick count monotonically increases
    let mut prev_tick = 0;
    for i in 0..tick_count {
        comp.run_tick(1.0 / 60.0).unwrap();
        assert_eq!(
            comp.tick_count(),
            (i + 1) as u64,
            "tick count should monotonically increase"
        );
        assert!(
            comp.tick_count() > prev_tick || i == 0,
            "tick count should strictly increase"
        );
        prev_tick = comp.tick_count();
    }

    // Partition order preserved in partitions() list
    let current_ids: Vec<&str> = comp.partitions().iter().map(|p| p.id()).collect();
    assert_eq!(
        current_ids,
        ids.iter().map(|s| s.as_str()).collect::<Vec<&str>>(),
        "partition order should be preserved"
    );

    // Lifecycle ordering: Running -> Terminated
    comp.shutdown().unwrap();
    assert_eq!(comp.state(), ExecutionState::Terminated);
}

/// Ordering property holds with Counters.
#[test]
fn ordering_with_counters() {
    assert_ordering_property(
        vec![
            Box::new(Counter::new("first")),
            Box::new(Counter::new("second")),
        ],
        5,
    );
}

/// Ordering property holds with mixed impls.
#[test]
fn ordering_with_mixed_impls() {
    assert_ordering_property(
        vec![
            Box::new(Doubler::new("first")),
            Box::new(Counter::new("second")),
            Box::new(Accumulator::new("third")),
        ],
        5,
    );
}

// ---------------------------------------------------------------------------
// Compositional property: IMPL SWAP STABILITY
//
// The same compositional test function works when the partition implementation
// is swapped. This is the key FPA-037 requirement.
// ---------------------------------------------------------------------------

/// Run the full compositional property suite with any set of partitions.
/// This function does NOT change when implementations change.
fn full_compositional_suite(partitions: Vec<Box<dyn Partition>>) {
    let ids: Vec<String> = partitions.iter().map(|p| p.id().to_string()).collect();
    let initial_count = partitions.len();
    let mut comp = make_compositor(partitions);

    // Lifecycle ordering
    assert_eq!(comp.state(), ExecutionState::Uninitialized);
    comp.init().unwrap();
    assert_eq!(comp.state(), ExecutionState::Running);

    let tick_count = 10;
    for i in 0..tick_count {
        comp.run_tick(1.0 / 60.0).unwrap();

        // Conservation: partition count stable
        assert_eq!(comp.partitions().len(), initial_count);

        // Ordering: tick count monotonic
        assert_eq!(comp.tick_count(), (i + 1) as u64);
    }

    // Delivery: all partition states in write buffer
    let write_buf = comp.buffer().write_all();
    for id in &ids {
        assert!(write_buf.contains_key(id));
        let state = &write_buf[id];
        assert!(state.is_table());
        assert!(!state.as_table().unwrap().is_empty());
    }

    // Conservation: buffer has exactly N entries
    assert_eq!(write_buf.len(), initial_count);

    comp.shutdown().unwrap();
    assert_eq!(comp.state(), ExecutionState::Terminated);
}

/// Full suite with all-Counter partitions.
#[test]
fn full_suite_all_counters() {
    full_compositional_suite(vec![
        Box::new(Counter::new("a")),
        Box::new(Counter::new("b")),
    ]);
}

/// Full suite with all-Accumulator partitions — same test, no modification.
#[test]
fn full_suite_all_accumulators() {
    full_compositional_suite(vec![
        Box::new(Accumulator::new("a")),
        Box::new(Accumulator::new("b")),
    ]);
}

/// Full suite with all-Doubler partitions — same test, no modification.
#[test]
fn full_suite_all_doublers() {
    full_compositional_suite(vec![
        Box::new(Doubler::new("a")),
        Box::new(Doubler::new("b")),
    ]);
}

/// Full suite with mixed implementations — same test, no modification.
#[test]
fn full_suite_mixed_impls() {
    full_compositional_suite(vec![
        Box::new(Counter::new("counter")),
        Box::new(Accumulator::new("accum")),
        Box::new(Doubler::new("doubler")),
    ]);
}

// ---------------------------------------------------------------------------
// Compositional property: DUMP/LOAD ROUNDTRIP
//
// Compositor dump/load preserves the compositional structure regardless of
// which partition implementations are inside.
// ---------------------------------------------------------------------------

fn assert_dump_load_roundtrip(partitions: Vec<Box<dyn Partition>>) {
    let mut comp = make_compositor(partitions);
    comp.init().unwrap();

    for _ in 0..5 {
        comp.run_tick(1.0 / 60.0).unwrap();
    }

    // Dump state
    let snapshot = comp.dump().unwrap();

    // Snapshot is a valid TOML table with "partitions" and "system" sections
    let root = snapshot.as_table().unwrap();
    assert!(root.contains_key("partitions"), "dump should contain 'partitions' section");
    assert!(root.contains_key("system"), "dump should contain 'system' section");

    // System section has tick_count and elapsed_time
    let system = root["system"].as_table().unwrap();
    assert!(system.contains_key("tick_count"));
    assert!(system.contains_key("elapsed_time"));

    // Load the snapshot back (must pause first per FPA-023)
    comp.pause().unwrap();
    comp.load(snapshot.clone()).unwrap();
    comp.resume().unwrap();

    // After load, dump again and verify structural equality
    let reloaded = comp.dump().unwrap();
    assert_eq!(snapshot, reloaded, "dump/load roundtrip should preserve state");

    comp.shutdown().unwrap();
}

/// Dump/load roundtrip with Counters.
#[test]
fn dump_load_roundtrip_counters() {
    assert_dump_load_roundtrip(vec![
        Box::new(Counter::new("a")),
        Box::new(Counter::new("b")),
    ]);
}

/// Dump/load roundtrip with mixed impls.
#[test]
fn dump_load_roundtrip_mixed() {
    assert_dump_load_roundtrip(vec![
        Box::new(Counter::new("counter")),
        Box::new(Accumulator::new("accum")),
        Box::new(Doubler::new("doubler")),
    ]);
}
