//! Tests for FPA-022: State Snapshot as Composition Fragment.

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::Counter;
use fpa_contract::StateContribution;

/// Dump produces valid TOML (parseable as a TOML string).
#[test]
fn dump_produces_valid_toml() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
        Box::new(Counter::new("b")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    let snapshot = compositor.dump().unwrap();

    // Serialize to TOML string and parse back - must round-trip cleanly
    let toml_string = toml::to_string(&snapshot).unwrap();
    let parsed: toml::Value = toml::from_str(&toml_string).unwrap();
    assert_eq!(snapshot, parsed);
}

/// Dumped state contains all partition IDs.
#[test]
fn dump_contains_all_partition_ids() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("alpha")),
        Box::new(Counter::new("beta")),
        Box::new(Counter::new("gamma")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    let snapshot = compositor.dump().unwrap();
    let partitions_table = snapshot.get("partitions").unwrap().as_table().unwrap();

    assert!(partitions_table.contains_key("alpha"));
    assert!(partitions_table.contains_key("beta"));
    assert!(partitions_table.contains_key("gamma"));
    assert_eq!(partitions_table.len(), 3);
}

/// Snapshot is a valid composition fragment structure (has partitions section).
#[test]
fn snapshot_has_composition_fragment_structure() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    let snapshot = compositor.dump().unwrap();

    // Must have a top-level "partitions" table
    assert!(snapshot.get("partitions").is_some());
    assert!(snapshot.get("partitions").unwrap().is_table());

    // Must have a top-level "system" table with tick_count
    assert!(snapshot.get("system").is_some());
    let system = snapshot.get("system").unwrap().as_table().unwrap();
    assert!(system.contains_key("tick_count"));
}

/// Snapshot includes current time and execution state metadata (FPA-022).
#[test]
fn snapshot_contains_time_and_execution_state() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.init().unwrap();
    compositor.run_tick(0.5).unwrap();
    compositor.run_tick(0.5).unwrap();
    compositor.run_tick(1.0).unwrap();

    let snapshot = compositor.dump().unwrap();
    let system = snapshot.get("system").unwrap().as_table().unwrap();

    // Must include elapsed_time (cumulative dt)
    let elapsed = system.get("elapsed_time").unwrap().as_float().unwrap();
    assert!(
        (elapsed - 2.0).abs() < 1e-9,
        "elapsed_time should be 2.0 (0.5 + 0.5 + 1.0), got {}",
        elapsed
    );

    // Must include execution_state
    let exec_state = system.get("execution_state").unwrap().as_str().unwrap();
    assert_eq!(exec_state, "Running");
}

/// Loading a state fragment with bare values (no StateContribution envelope)
/// should be rejected. Partition state entries must use the StateContribution
/// format (state/fresh/age_ms) to preserve freshness metadata integrity.
///
/// This test documents the expected behavior per FPA-022: state snapshots are
/// composition fragments produced by contribute_state(), which always wraps
/// output in StateContribution envelopes. Loading bare values silently would
/// bypass freshness metadata and violate the contract boundary.
#[test]
fn load_rejects_bare_values_without_envelope() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();
    compositor.pause().unwrap();

    // Construct a fragment with bare partition state (no StateContribution envelope).
    // This is NOT a valid snapshot — snapshots always use the envelope format.
    let bare_fragment: toml::Value = toml::from_str(
        r#"
        [system]
        tick_count = 0

        [partitions.counter]
        count = 42
        "#,
    )
    .unwrap();

    let result = compositor.load(bare_fragment);
    assert!(
        result.is_err(),
        "loading bare values without StateContribution envelope should be rejected"
    );
}

/// A fragment extending a snapshot can override a partition's state.
#[test]
fn snapshot_with_extends_override() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.init().unwrap();
    for _ in 0..5 {
        compositor.run_tick(1.0).unwrap();
    }

    // Take a snapshot (base)
    let base_snapshot = compositor.dump().unwrap();

    // Create an "extending" fragment that overrides the counter's state.
    // The partition entry must use the StateContribution envelope format.
    let override_toml: toml::Value = toml::from_str(
        r#"
        [partitions.counter]
        fresh = true
        age_ms = 0
        [partitions.counter.state]
        count = 42
        "#,
    )
    .unwrap();

    // Deep-merge: override on top of base (override wins)
    let merged = fpa_config::deep_merge(base_snapshot.clone(), override_toml);

    // Create fresh compositor and load the merged state
    let partitions2: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus2 = InProcessBus::new("test-bus-2");
    let mut compositor2 = Compositor::new(partitions2, Box::new(bus2));
    compositor2.init().unwrap();
    compositor2.pause().unwrap();
    compositor2.load(merged).unwrap();
    compositor2.resume().unwrap();

    // The override should have won: counter at 42, not 5
    let state2 = compositor2.dump().unwrap();
    let counter_sc = StateContribution::from_toml(
        state2.get("partitions").unwrap().get("counter").unwrap()
    ).unwrap();
    let count = counter_sc.state
        .get("count")
        .unwrap()
        .as_integer()
        .unwrap();
    assert_eq!(count, 42);
}
