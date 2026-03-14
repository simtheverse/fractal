//! FPA-039: Contract Version Reference Data Isolation
//!
//! Verifies that:
//! - Each contract version has its own reference data (canonical inputs, tolerances)
//! - An impl targeting version N is unaffected by version N+1
//! - Version-scoped canonical inputs and output properties are independent
//! - Message types declare their contract version

use fpa_contract::Partition;
use fpa_contract::test_support::{
    Accumulator, CanonicalInputs, ContractTolerances, ContractVersion, Counter, Doubler,
    OutputProperties,
};
use fpa_contract::message::Message;
use fpa_contract::test_support::{AccumulatorOutput, CounterOutput, DoublerOutput};

// ---------------------------------------------------------------------------
// Contract versions are distinct and well-defined
// ---------------------------------------------------------------------------

/// Contract versions V1 and V2 are distinct.
#[test]
fn contract_versions_are_distinct() {
    assert_ne!(ContractVersion::V1, ContractVersion::V2);
}

// ---------------------------------------------------------------------------
// Message types declare their contract version
// ---------------------------------------------------------------------------

/// CounterOutput targets contract version 1.
#[test]
fn counter_output_targets_v1() {
    assert_eq!(CounterOutput::VERSION, 1);
}

/// AccumulatorOutput targets contract version 1.
#[test]
fn accumulator_output_targets_v1() {
    assert_eq!(AccumulatorOutput::VERSION, 1);
}

/// DoublerOutput targets contract version 2.
#[test]
fn doubler_output_targets_v2() {
    assert_eq!(DoublerOutput::VERSION, 2);
}

// ---------------------------------------------------------------------------
// Version-scoped canonical inputs are independent
// ---------------------------------------------------------------------------

/// V1 canonical inputs exist and are valid.
#[test]
fn v1_canonical_inputs_valid() {
    let dt = CanonicalInputs::standard_dt_for_version(ContractVersion::V1);
    assert!(dt > 0.0);
    assert!(dt.is_finite());

    let seq = CanonicalInputs::timestep_sequence_for_version(ContractVersion::V1, 5);
    assert_eq!(seq.len(), 5);
    assert!(seq.iter().all(|&d| d > 0.0 && d.is_finite()));
}

/// V2 canonical inputs exist and are valid.
#[test]
fn v2_canonical_inputs_valid() {
    let dt = CanonicalInputs::standard_dt_for_version(ContractVersion::V2);
    assert!(dt > 0.0);
    assert!(dt.is_finite());

    let seq = CanonicalInputs::timestep_sequence_for_version(ContractVersion::V2, 5);
    assert_eq!(seq.len(), 5);
    assert!(seq.iter().all(|&d| d > 0.0 && d.is_finite()));
}

// ---------------------------------------------------------------------------
// Version-scoped tolerances are independent
// ---------------------------------------------------------------------------

/// V1 tolerance is defined and positive.
#[test]
fn v1_tolerance_defined() {
    let tol = ContractTolerances::float_tolerance_for_version(ContractVersion::V1);
    assert!(tol > 0.0);
    assert!(tol.is_finite());
}

/// V2 tolerance is defined and positive.
#[test]
fn v2_tolerance_defined() {
    let tol = ContractTolerances::float_tolerance_for_version(ContractVersion::V2);
    assert!(tol > 0.0);
    assert!(tol.is_finite());
}

// ---------------------------------------------------------------------------
// V1 impl unaffected by V2 reference data
// ---------------------------------------------------------------------------

/// V1 impl (Counter) passes contract tests using V1 canonical inputs.
/// This test is completely independent of V2 — adding or changing V2
/// canonical inputs, tolerances, or implementations does not affect it.
#[test]
fn v1_counter_passes_with_v1_inputs() {
    let mut counter = Counter::new("c");
    let dts = CanonicalInputs::timestep_sequence_for_version(ContractVersion::V1, 10);
    let tol = ContractTolerances::float_tolerance_for_version(ContractVersion::V1);

    counter.init().unwrap();
    for dt in &dts {
        counter.step(*dt).unwrap();
    }

    let state = counter.contribute_state().unwrap();
    OutputProperties::assert_valid_state_table(&state);
    OutputProperties::assert_non_negative_numeric_fields(&state);

    // V1-specific property: counter's "count" field is an integer equal to step count
    let count = state.as_table().unwrap()["count"].as_integer().unwrap();
    assert_eq!(count, 10, "counter should have counted 10 steps");

    counter.shutdown().unwrap();
    let _ = tol; // tolerance is available but not needed for integer comparison
}

/// V1 impl (Accumulator) passes contract tests using V1 canonical inputs.
#[test]
fn v1_accumulator_passes_with_v1_inputs() {
    let mut acc = Accumulator::new("a");
    let dts = CanonicalInputs::timestep_sequence_for_version(ContractVersion::V1, 10);
    let tol = ContractTolerances::float_tolerance_for_version(ContractVersion::V1);

    acc.init().unwrap();
    for dt in &dts {
        acc.step(*dt).unwrap();
    }

    let state = acc.contribute_state().unwrap();
    OutputProperties::assert_valid_state_table(&state);
    OutputProperties::assert_non_negative_numeric_fields(&state);

    // V1-specific property: accumulator total within tolerance of sum of inputs
    let total = state.as_table().unwrap()["total"].as_float().unwrap();
    let expected = dts.iter().sum::<f64>();
    assert!(
        (total - expected).abs() < tol,
        "accumulator total should be within V1 tolerance of expected sum"
    );

    acc.shutdown().unwrap();
}

// ---------------------------------------------------------------------------
// V2 impl uses V2 reference data independently
// ---------------------------------------------------------------------------

/// V2 impl (Doubler) passes contract tests using V2 canonical inputs.
/// Uses V2-scoped tolerance. Independent of V1.
#[test]
fn v2_doubler_passes_with_v2_inputs() {
    let mut doubler = Doubler::new("d");
    let dts = CanonicalInputs::timestep_sequence_for_version(ContractVersion::V2, 5);
    let tol = ContractTolerances::float_tolerance_for_version(ContractVersion::V2);

    doubler.init().unwrap();
    for dt in &dts {
        doubler.step(*dt).unwrap();
    }

    let state = doubler.contribute_state().unwrap();
    OutputProperties::assert_valid_state_table(&state);
    OutputProperties::assert_non_negative_numeric_fields(&state);

    // V2-specific property: doubler value is 2^N (starts at 1.0, doubles each step)
    let value = state.as_table().unwrap()["value"].as_float().unwrap();
    let expected = 2.0_f64.powi(dts.len() as i32);
    assert!(
        (value - expected).abs() < tol,
        "doubler value should be 2^{} within V2 tolerance (got {}, expected {})",
        dts.len(),
        value,
        expected
    );

    doubler.shutdown().unwrap();
}

// ---------------------------------------------------------------------------
// Cross-version isolation: changing V2 does not break V1
// ---------------------------------------------------------------------------

/// Demonstrates version isolation: V1 test suite uses only V1 reference data.
/// If V2 canonical inputs or tolerances were to change, this test would be
/// completely unaffected because it only references V1 data.
#[test]
fn v1_tests_isolated_from_v2_changes() {
    // This test explicitly uses only V1 versions of everything
    let dt = CanonicalInputs::standard_dt_for_version(ContractVersion::V1);
    let tol = ContractTolerances::float_tolerance_for_version(ContractVersion::V1);

    // Run V1 impls with V1 data
    let mut counter = Counter::new("c");
    counter.init().unwrap();
    counter.step(dt).unwrap();
    let state = counter.contribute_state().unwrap();
    OutputProperties::assert_valid_state_table(&state);
    counter.shutdown().unwrap();

    let mut acc = Accumulator::new("a");
    acc.init().unwrap();
    acc.step(dt).unwrap();
    let state = acc.contribute_state().unwrap();
    OutputProperties::assert_valid_state_table(&state);

    // Use V1 tolerance for float comparison
    let total = state.as_table().unwrap()["total"].as_float().unwrap();
    assert!((total - dt).abs() < tol);
    acc.shutdown().unwrap();

    // V2 data is not referenced anywhere in this test — proof of isolation
}

/// V2 tests are isolated from V1: uses only V2 reference data.
#[test]
fn v2_tests_isolated_from_v1_changes() {
    let dt = CanonicalInputs::standard_dt_for_version(ContractVersion::V2);
    let tol = ContractTolerances::float_tolerance_for_version(ContractVersion::V2);

    let mut doubler = Doubler::new("d");
    doubler.init().unwrap();
    doubler.step(dt).unwrap();
    let state = doubler.contribute_state().unwrap();
    OutputProperties::assert_valid_state_table(&state);

    let value = state.as_table().unwrap()["value"].as_float().unwrap();
    // After 1 step, value should be 2.0
    assert!((value - 2.0).abs() < tol);
    doubler.shutdown().unwrap();

    // V1 data is not referenced anywhere in this test — proof of isolation
}
