//! Tests for FPA-022: State Snapshot as Composition Fragment.

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::Counter;

/// Dump produces valid TOML (parseable as a TOML string).
#[test]
fn dump_produces_valid_toml() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
        Box::new(Counter::new("b")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, bus);

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
    let mut compositor = Compositor::new(partitions, bus);

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
    let mut compositor = Compositor::new(partitions, bus);

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

/// A fragment extending a snapshot can override a partition's state.
#[test]
fn snapshot_with_extends_override() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("counter")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();
    for _ in 0..5 {
        compositor.run_tick(1.0).unwrap();
    }

    // Take a snapshot (base)
    let base_snapshot = compositor.dump().unwrap();

    // Create an "extending" fragment that overrides the counter's state
    let override_toml: toml::Value = toml::from_str(
        r#"
        [partitions.counter]
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
    let mut compositor2 = Compositor::new(partitions2, bus2);
    compositor2.init().unwrap();
    compositor2.load(merged).unwrap();

    // The override should have won: counter at 42, not 5
    let state2 = compositor2.dump().unwrap();
    let count = state2
        .get("partitions")
        .unwrap()
        .get("counter")
        .unwrap()
        .get("count")
        .unwrap()
        .as_integer()
        .unwrap();
    assert_eq!(count, 42);
}
