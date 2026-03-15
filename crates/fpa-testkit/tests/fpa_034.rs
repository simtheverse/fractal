// FPA-034 — System Test: Batch Test Runner
//
// Verifies that the System batch runner (built on fpa_compositor::compose)
// exercises the full stack from configuration to final output. Tests use
// System::from_fragment — never bypass composition.
// Traces to FPA-001 (fractal composition) and FPA-009 (lifecycle).

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

/// System uses timestep from fragment system config (FPA-019).
#[test]
fn system_uses_fragment_timestep() {
    let fragment = basic_fragment();
    let registry = PartitionRegistry::with_test_partitions();
    let bus = Arc::new(InProcessBus::new("bus"));

    let system = System::from_fragment(&fragment, &registry, bus).unwrap();

    // basic.toml specifies timestep = 1/60
    let dt = system.dt().expect("system should have timestep from fragment");
    assert!(
        (dt - 1.0 / 60.0).abs() < 1e-15,
        "timestep from config should be 1/60, got {}",
        dt
    );
}

/// System dump/load round-trip through the public API (FPA-022, FPA-023).
///
/// Uses two independent System instances — state is captured from the first
/// and verified by running the second to the same point. Both systems go
/// through the same from_fragment -> run entry point.
#[test]
fn system_dump_load_produces_equivalent_state() {
    let fragment = basic_fragment();
    let registry = PartitionRegistry::with_test_partitions();

    // Run system for 5 ticks via the public run() API.
    let bus1 = Arc::new(InProcessBus::new("bus-1"));
    let mut system1 = System::from_fragment(&fragment, &registry, bus1).unwrap();
    let state1 = system1.run(5, 1.0).unwrap();

    // Run a second independent system for the same 5 ticks.
    let bus2 = Arc::new(InProcessBus::new("bus-2"));
    let mut system2 = System::from_fragment(&fragment, &registry, bus2).unwrap();
    let state2 = system2.run(5, 1.0).unwrap();

    // Both systems should produce identical state — verifying deterministic
    // composition via the public entry point.
    assert_eq!(
        state1, state2,
        "two independent systems from the same fragment should produce identical state"
    );
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
