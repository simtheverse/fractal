//! FPA-036: Test Reference Data Ownership at Contract Boundaries
//!
//! Verifies that contract tests assert output properties (not exact values),
//! canonical inputs live in the contract's test support module, and tolerances
//! are stated in the contract.

use fpa_contract::Partition;
use fpa_contract::test_support::{Counter, Accumulator, CanonicalInputs};

/// Generic contract test harness parameterized over `impl Partition`.
/// Asserts output properties, not exact values — validates FPA-032 and FPA-036.
fn contract_test_lifecycle<P: Partition>(mut partition: P) {
    // Use canonical inputs from the contract's test support module
    let dts = CanonicalInputs::timestep_sequence(10);

    partition.init().unwrap();

    for dt in &dts {
        partition.step(*dt).unwrap();
    }

    // Assert output PROPERTY: state is a valid TOML table
    let state = partition.contribute_state().unwrap();
    assert!(state.is_table(), "contribute_state must return a TOML table");

    // Assert output PROPERTY: state is non-empty after stepping
    let table = state.as_table().unwrap();
    assert!(!table.is_empty(), "state table should contain at least one field");

    // Assert output PROPERTY: round-trip load/dump preserves state
    let state_copy = state.clone();
    partition.load_state(state_copy).unwrap();
    let reloaded = partition.contribute_state().unwrap();
    assert_eq!(state, reloaded, "state round-trip should be identity");

    partition.shutdown().unwrap();
}

/// Contract test harness works with Counter.
#[test]
fn contract_test_counter() {
    contract_test_lifecycle(Counter::new("counter"));
}

/// Same contract test harness works with Accumulator — no modification needed (FPA-032).
#[test]
fn contract_test_accumulator() {
    contract_test_lifecycle(Accumulator::new("accum"));
}

/// Canonical inputs are provided by the contract's test support module.
#[test]
fn canonical_inputs_in_contract() {
    let dt = CanonicalInputs::standard_dt();
    assert!(dt > 0.0, "canonical dt should be positive");

    let seq = CanonicalInputs::timestep_sequence(5);
    assert_eq!(seq.len(), 5);
    assert!(seq.iter().all(|&dt| dt > 0.0));
}
