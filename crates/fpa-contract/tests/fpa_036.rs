//! FPA-036: Test Reference Data Ownership at Contract Boundaries
//!
//! Verifies that:
//! - Contract tests assert output properties (not exact values)
//! - Canonical inputs live in the contract's test support module
//! - Tolerances are stated in the contract
//! - Output property helpers are provided for reuse across tests

use fpa_contract::Partition;
use fpa_contract::test_support::{
    Accumulator, CanonicalInputs, ContractTolerances, ContractVersion, Counter, Doubler,
    OutputProperties,
};

// ---------------------------------------------------------------------------
// Property-based contract test harness (not exact-value assertions)
// ---------------------------------------------------------------------------

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

/// Same contract test harness works with Doubler — no modification needed.
#[test]
fn contract_test_doubler() {
    contract_test_lifecycle(Doubler::new("doubler"));
}

// ---------------------------------------------------------------------------
// Canonical inputs live in the contract's test support module
// ---------------------------------------------------------------------------

/// Canonical inputs are provided by the contract's test support module.
#[test]
fn canonical_inputs_in_contract() {
    let dt = CanonicalInputs::standard_dt();
    assert!(dt > 0.0, "canonical dt should be positive");

    let seq = CanonicalInputs::timestep_sequence(5);
    assert_eq!(seq.len(), 5);
    assert!(seq.iter().all(|&dt| dt > 0.0));
}

/// Version-scoped canonical inputs are available per contract version (FPA-039).
#[test]
fn canonical_inputs_version_scoped() {
    let v1_dt = CanonicalInputs::standard_dt_for_version(ContractVersion::V1);
    let v2_dt = CanonicalInputs::standard_dt_for_version(ContractVersion::V2);

    assert!(v1_dt > 0.0, "v1 canonical dt should be positive");
    assert!(v2_dt > 0.0, "v2 canonical dt should be positive");

    let v1_seq = CanonicalInputs::timestep_sequence_for_version(ContractVersion::V1, 3);
    let v2_seq = CanonicalInputs::timestep_sequence_for_version(ContractVersion::V2, 3);
    assert_eq!(v1_seq.len(), 3);
    assert_eq!(v2_seq.len(), 3);
}

// ---------------------------------------------------------------------------
// Tolerances stated in the contract
// ---------------------------------------------------------------------------

/// Tolerances are declared in the contract module, not in individual tests.
#[test]
fn tolerances_declared_in_contract() {
    // Version-scoped tolerances exist and are positive
    let v1_tol = ContractTolerances::float_tolerance_for_version(ContractVersion::V1);
    let v2_tol = ContractTolerances::float_tolerance_for_version(ContractVersion::V2);
    assert!(v1_tol > 0.0);
    assert!(v2_tol > 0.0);
}

/// Contract tests use stated tolerances for floating-point comparisons.
#[test]
fn contract_tests_use_stated_tolerances() {
    let mut acc = Accumulator::new("acc");
    acc.init().unwrap();

    let dt = CanonicalInputs::standard_dt();
    let steps = 10;
    for _ in 0..steps {
        acc.step(dt).unwrap();
    }

    let state = acc.contribute_state().unwrap();
    let total = state.as_table().unwrap()["total"].as_float().unwrap();

    // Assert PROPERTY with stated tolerance: total should be approximately N * dt.
    // We assert the property (accumulated total equals sum of inputs within tolerance),
    // not the exact value.
    let expected_total = dt * steps as f64;
    let diff = (total - expected_total).abs();
    assert!(
        diff < ContractTolerances::FLOAT_TOLERANCE,
        "accumulated total should equal sum of inputs within contract tolerance (diff={})",
        diff
    );
}

// ---------------------------------------------------------------------------
// Output property helpers provided by the contract
// ---------------------------------------------------------------------------

/// OutputProperties helper validates state table structure.
#[test]
fn output_properties_validate_state_structure() {
    let mut counter = Counter::new("c");
    counter.init().unwrap();
    counter.step(CanonicalInputs::standard_dt()).unwrap();

    let state = counter.contribute_state().unwrap();
    OutputProperties::assert_valid_state_table(&state);
    OutputProperties::assert_non_negative_numeric_fields(&state);
}

/// OutputProperties round-trip helper works across implementations.
#[test]
fn output_properties_roundtrip_works_across_impls() {
    // Counter
    let mut counter = Counter::new("c");
    counter.init().unwrap();
    counter.step(CanonicalInputs::standard_dt()).unwrap();
    let state = counter.contribute_state().unwrap();
    OutputProperties::assert_state_roundtrip(&mut counter, &state);

    // Accumulator
    let mut acc = Accumulator::new("a");
    acc.init().unwrap();
    acc.step(CanonicalInputs::standard_dt()).unwrap();
    let state = acc.contribute_state().unwrap();
    OutputProperties::assert_state_roundtrip(&mut acc, &state);

    // Doubler
    let mut dbl = Doubler::new("d");
    dbl.init().unwrap();
    dbl.step(CanonicalInputs::standard_dt()).unwrap();
    let state = dbl.contribute_state().unwrap();
    OutputProperties::assert_state_roundtrip(&mut dbl, &state);
}
