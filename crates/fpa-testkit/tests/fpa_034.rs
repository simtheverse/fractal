// FPA-034 — System Test: Operator Entry Point
//
// Verifies that the System type provides the canonical operator entry point
// for FPA applications. Tests use System::from_fragment — never bypass
// composition. Traces to FPA-001 (fractal composition) and FPA-009 (lifecycle).

use std::sync::Arc;

use fpa_bus::InProcessBus;
use fpa_config::CompositionFragment;
use fpa_contract::StateContribution;
use fpa_testkit::registry::PartitionRegistry;
use fpa_testkit::system::System;

fn basic_fragment() -> CompositionFragment {
    let toml_str = include_str!("../test-configs/basic.toml");
    fpa_config::load_from_str(toml_str).unwrap()
}

/// System from fragment runs full lifecycle (FPA-001, FPA-009).
#[test]
fn system_from_fragment_full_lifecycle() {
    let fragment = basic_fragment();
    let registry = PartitionRegistry::with_test_partitions();
    let bus = Arc::new(InProcessBus::new("system-bus"));

    let mut system = System::from_fragment(&fragment, &registry, bus).unwrap();
    let state = system.run(10, 1.0 / 60.0).unwrap();

    let partitions = state.as_table().unwrap()["partitions"].as_table().unwrap();

    // Counter should have counted 10 steps
    let counter_sc = StateContribution::from_toml(&partitions["counter"]).unwrap();
    let count = counter_sc.state.as_table().unwrap()["count"].as_integer().unwrap();
    assert_eq!(count, 10);

    // Accumulator should have accumulated
    let acc_sc = StateContribution::from_toml(&partitions["accumulator"]).unwrap();
    let total = acc_sc.state.as_table().unwrap()["total"].as_float().unwrap();
    assert!(total > 0.0, "accumulator should have accumulated time");
}

/// System dump/load through public API (FPA-022, FPA-023).
#[test]
fn system_dump_load_round_trip() {
    let fragment = basic_fragment();
    let registry = PartitionRegistry::with_test_partitions();

    // Run system 1 for 5 ticks and capture state
    let bus1 = Arc::new(InProcessBus::new("bus-1"));
    let mut system1 = System::from_fragment(&fragment, &registry, bus1).unwrap();
    let compositor1 = system1.compositor_mut();
    compositor1.init().unwrap();
    for _ in 0..5 {
        compositor1.run_tick(1.0).unwrap();
    }
    let snapshot = compositor1.dump().unwrap();

    // Load into system 2 and verify state matches
    let bus2 = Arc::new(InProcessBus::new("bus-2"));
    let mut system2 = System::from_fragment(&fragment, &registry, bus2).unwrap();
    let compositor2 = system2.compositor_mut();
    compositor2.init().unwrap();
    compositor2.pause().unwrap();
    compositor2.load(snapshot.clone()).unwrap();
    compositor2.resume().unwrap();

    let snapshot2 = compositor2.dump().unwrap();
    assert_eq!(snapshot, snapshot2, "dump/load round-trip should preserve state");
}

/// System rejects fragments with missing implementation.
#[test]
fn system_rejects_missing_implementation() {
    let toml_str = r#"
[partitions.broken]
"#;
    let fragment = fpa_config::load_from_str(toml_str).unwrap();
    let registry = PartitionRegistry::with_test_partitions();
    let bus = Arc::new(InProcessBus::new("bus"));

    let result = System::from_fragment(&fragment, &registry, bus);
    assert!(result.is_err());
}

/// System rejects unknown implementation names.
#[test]
fn system_rejects_unknown_implementation() {
    let toml_str = r#"
[partitions.broken]
implementation = "NonexistentPartition"
"#;
    let fragment = fpa_config::load_from_str(toml_str).unwrap();
    let registry = PartitionRegistry::with_test_partitions();
    let bus = Arc::new(InProcessBus::new("bus"));

    let result = System::from_fragment(&fragment, &registry, bus);
    assert!(result.is_err());
}
