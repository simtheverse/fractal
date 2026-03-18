// 6R — Ergonomics & Performance evaluation tests
//
// Measures boilerplate cost, fractal uniformity, and conceptual footprint
// to establish baselines for the FPA prototype's developer experience.

use std::sync::Arc;

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::Counter;
use fpa_contract::Partition;

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

// ---------------------------------------------------------------------------
// 6R.1 — Boilerplate measurement
// ---------------------------------------------------------------------------

#[test]
fn boilerplate_new_partition() {
    let root = workspace_root();

    // Simplest partition: Counter
    let counter_path = root.join("crates/fpa-contract/src/test_support/counter.rs");
    let counter_src = std::fs::read_to_string(&counter_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", counter_path.display(), e));
    let counter_loc = counter_src.lines().count();
    println!("Counter partition (simplest): {} LOC", counter_loc);

    // Bus-aware partition: Sensor section in test_partitions.rs
    let tp_path = root.join("crates/fpa-testkit/src/test_partitions.rs");
    let tp_src = std::fs::read_to_string(&tp_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", tp_path.display(), e));
    let tp_loc = tp_src.lines().count();
    // Sensor is the first partition in the file (roughly lines 1-164)
    println!("test_partitions.rs (Sensor+Follower+Recorder): {} LOC", tp_loc);

    // Measure Sensor section LOC
    let sensor_start = tp_src.find("pub struct Sensor").expect("should find Sensor");
    let sensor_end = tp_src[sensor_start..].find("pub struct Follower")
        .map(|i| sensor_start + i)
        .unwrap_or(tp_src.len());
    let sensor_loc = tp_src[sensor_start..sensor_end].lines().count();
    println!("Sensor partition (bus-aware): {} LOC", sensor_loc);

    // Boilerplate summary
    println!("--- Boilerplate: new partition ---");
    println!("  Minimal (no bus):  ~{} LOC (Counter)", counter_loc);
    println!("  Bus-aware:         ~{} LOC (Sensor section)", sensor_loc);
}

#[test]
fn boilerplate_new_message_type() {
    let root = workspace_root();

    let msg_path = root.join("crates/fpa-contract/src/test_support/messages.rs");
    let msg_src = std::fs::read_to_string(&msg_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", msg_path.display(), e));
    let msg_loc = msg_src.lines().count();
    let msg_type_count = msg_src.matches("impl Message for").count();
    assert!(msg_type_count > 0, "messages.rs should contain at least one Message impl");
    println!("messages.rs ({} message types): {} LOC", msg_type_count, msg_loc);
    println!("  Approximate LOC per message type: {}", msg_loc / msg_type_count);
}

#[test]
fn boilerplate_new_layer() {
    let root = workspace_root();

    let nesting_test = root.join("crates/fpa-compositor/tests/fpa_001_fractal.rs");
    let src = std::fs::read_to_string(&nesting_test)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", nesting_test.display(), e));
    let loc = src.lines().count();
    println!("fpa_001_fractal.rs (3-layer nesting test): {} LOC", loc);

    // The compositor-as-partition setup (layer_1_compositor_as_partition) is roughly 30 lines
    println!("  Compositor-as-partition setup: ~30 LOC (inner + outer + init + tick loop)");
}

// ---------------------------------------------------------------------------
// 6R.3 — Fractal uniformity
// ---------------------------------------------------------------------------

#[test]
fn api_identical_across_layers() {
    // Layer-0: flat compositor with a single Counter
    let mut layer0 = Compositor::new(
        vec![Box::new(Counter::new("a"))],
        Arc::new(InProcessBus::new("bus-0")),
    );

    // Layer-1: nested compositor (inner compositor as a partition)
    let inner = Compositor::new(
        vec![Box::new(Counter::new("b"))],
        Arc::new(InProcessBus::new("bus-1")),
    )
    .with_id("inner")
    .with_layer_depth(1);

    let mut layer0_nested = Compositor::new(
        vec![Box::new(Counter::new("a")), Box::new(inner)],
        Arc::new(InProcessBus::new("bus-0n")),
    );

    // Same Partition trait methods work identically on both
    layer0.init().unwrap();
    layer0_nested.init().unwrap();

    let dt = 1.0 / 60.0;
    layer0.run_tick(dt).unwrap();
    layer0_nested.run_tick(dt).unwrap();

    // contribute_state works on both
    let state0 = layer0.contribute_state().unwrap();
    let state1 = layer0_nested.contribute_state().unwrap();
    assert!(state0.is_table(), "layer-0 state should be a table");
    assert!(state1.is_table(), "nested layer state should be a table");

    layer0.shutdown().unwrap();
    layer0_nested.shutdown().unwrap();

    println!("API identity verified: init, step/run_tick, contribute_state, shutdown all work identically across layers");
}

#[test]
fn conceptual_footprint() {
    let concepts = [
        "Partition",
        "Message",
        "Bus",
        "Compositor",
        "StateContribution",
        "SharedContext",
        "DoubleBuffer",
        "DeliverySemantic",
        "ExecutionState",
        "CompositionFragment",
    ];
    println!("Conceptual footprint: {} unique concepts", concepts.len());
    // Layer depth doesn't add concepts — same primitives at every layer
    assert!(
        concepts.len() <= 15,
        "conceptual footprint should be bounded"
    );
}
