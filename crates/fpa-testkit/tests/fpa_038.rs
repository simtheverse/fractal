// FPA-038 — Reference File Tests
//
// Verifies that reference files capture provenance metadata and can be
// serialized/deserialized for persistence. Tests follow bottom-up ordering:
// contract -> compositor -> system.

use fpa_config::CompositionFragment;
use fpa_contract::StateContribution;
use fpa_testkit::reference::ReferenceFile;
use fpa_testkit::registry::PartitionRegistry;

fn basic_fragment() -> CompositionFragment {
    let toml_str = include_str!("../test-configs/basic.toml");
    fpa_config::load_from_str(toml_str).unwrap()
}

/// Reference file records provenance metadata (FPA-038).
#[test]
fn reference_file_records_provenance() {
    let fragment = basic_fragment();
    let registry = PartitionRegistry::with_test_partitions();

    let reference = ReferenceFile::generate(&fragment, &registry, 5, 1.0).unwrap();

    // Command records generation parameters
    assert!(
        reference.provenance.command.contains("ticks=5"),
        "provenance should record tick count"
    );

    // Timestamp is non-empty
    assert!(
        !reference.provenance.timestamp.is_empty(),
        "provenance should record timestamp"
    );

    // Implementation versions are recorded
    assert!(
        !reference.provenance.impl_versions.is_empty(),
        "provenance should record impl versions"
    );
    // Should contain entries for each partition
    assert!(
        reference.provenance.impl_versions.iter().any(|v| v.contains("counter")),
        "impl versions should include counter partition"
    );

    // Contract versions are recorded
    assert!(
        !reference.provenance.contract_versions.is_empty(),
        "provenance should record contract versions"
    );
    assert!(
        reference.provenance.contract_versions.iter().any(|v| v.contains("fpa-contract")),
        "contract versions should include fpa-contract"
    );
}

/// Reference file round-trips through TOML serialization.
#[test]
fn reference_file_toml_round_trip() {
    let fragment = basic_fragment();
    let registry = PartitionRegistry::with_test_partitions();

    let original = ReferenceFile::generate(&fragment, &registry, 3, 1.0).unwrap();
    let toml_str = original.to_toml_string().unwrap();
    let restored = ReferenceFile::from_toml_str(&toml_str).unwrap();

    assert_eq!(original.output, restored.output);
    assert_eq!(original.provenance.command, restored.provenance.command);
    assert_eq!(
        original.provenance.impl_versions,
        restored.provenance.impl_versions
    );
    assert_eq!(
        original.provenance.contract_versions,
        restored.provenance.contract_versions
    );
}

/// Regeneration produces updated references after config change.
#[test]
fn regeneration_after_config_change() {
    let registry = PartitionRegistry::with_test_partitions();

    // Generate with basic config (counter + accumulator)
    let fragment1 = basic_fragment();
    let ref1 = ReferenceFile::generate(&fragment1, &registry, 5, 1.0).unwrap();

    // Generate with different tick count
    let ref2 = ReferenceFile::generate(&fragment1, &registry, 10, 1.0).unwrap();

    // Outputs should differ (more ticks = higher count)
    let p1 = ref1.output.as_table().unwrap()["partitions"].as_table().unwrap();
    let p2 = ref2.output.as_table().unwrap()["partitions"].as_table().unwrap();

    let count1 = StateContribution::from_toml(&p1["counter"])
        .unwrap()
        .state
        .as_table()
        .unwrap()["count"]
        .as_integer()
        .unwrap();
    let count2 = StateContribution::from_toml(&p2["counter"])
        .unwrap()
        .state
        .as_table()
        .unwrap()["count"]
        .as_integer()
        .unwrap();

    assert_eq!(count1, 5);
    assert_eq!(count2, 10);
    assert_ne!(ref1.output, ref2.output);
}

/// Bottom-up ordering: contract tests (partition behavior) are independent
/// of compositor and system tests. This test verifies that the reference
/// file infrastructure works at each level.
#[test]
fn bottom_up_contract_to_system() {
    // Contract level: partition creates valid state
    let mut counter = fpa_contract::test_support::Counter::new("c");
    counter.init().unwrap();
    counter.step(1.0).unwrap();
    let state = counter.contribute_state().unwrap();
    assert_eq!(
        state.as_table().unwrap()["count"].as_integer().unwrap(),
        1
    );

    // Compositor level: compositions produce valid state
    use std::sync::Arc;
    use fpa_bus::InProcessBus;
    use fpa_compositor::compositor::Compositor;
    use fpa_contract::Partition;

    let parts: Vec<Box<dyn Partition>> = vec![
        Box::new(fpa_contract::test_support::Counter::new("c")),
    ];
    let mut comp = Compositor::new(parts, Arc::new(InProcessBus::new("b")));
    comp.init().unwrap();
    comp.run_tick(1.0).unwrap();
    let comp_state = comp.dump().unwrap();
    let comp_count = StateContribution::from_toml(
        &comp_state.as_table().unwrap()["partitions"]
            .as_table()
            .unwrap()["c"],
    )
    .unwrap()
    .state
    .as_table()
    .unwrap()["count"]
    .as_integer()
    .unwrap();
    assert_eq!(comp_count, 1);
    comp.shutdown().unwrap();

    // System level: reference file captures the same result
    let fragment = basic_fragment();
    let registry = fpa_testkit::registry::PartitionRegistry::with_test_partitions();
    let reference = ReferenceFile::generate(&fragment, &registry, 1, 1.0).unwrap();
    let ref_parts = reference.output.as_table().unwrap()["partitions"]
        .as_table()
        .unwrap();
    let ref_count = StateContribution::from_toml(&ref_parts["counter"])
        .unwrap()
        .state
        .as_table()
        .unwrap()["count"]
        .as_integer()
        .unwrap();
    assert_eq!(ref_count, 1);
}
