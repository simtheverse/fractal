//! FPA-032: Contract Test Reusability
//!
//! Verifies that the same contract test suite runs against multiple partition
//! implementations without modification. The generic test harness is parameterized
//! over `impl Partition` — no implementation-specific logic leaks into the tests.

use fpa_contract::Partition;
use fpa_contract::test_support::{
    Accumulator, CanonicalInputs, Counter, Doubler, OutputProperties,
};

/// Generic contract test suite parameterized over `impl Partition`.
///
/// This single function exercises the full partition contract: init, step,
/// contribute_state, load_state, and shutdown. It asserts only output
/// properties (valid table, non-empty, round-trip identity) — never
/// implementation-specific values like "count == 10" or "total == 0.166...".
fn contract_test_suite<P: Partition>(mut partition: P) {
    let dts = CanonicalInputs::timestep_sequence(10);

    // Init succeeds
    partition.init().unwrap();

    // Stepping succeeds for all canonical timesteps
    for dt in &dts {
        partition.step(*dt).unwrap();
    }

    // Output property: valid, non-empty TOML table
    let state = partition.contribute_state().unwrap();
    OutputProperties::assert_valid_state_table(&state);

    // Output property: all numeric fields are non-negative after stepping
    OutputProperties::assert_non_negative_numeric_fields(&state);

    // Output property: state round-trip is identity
    OutputProperties::assert_state_roundtrip(&mut partition, &state);

    // Shutdown succeeds
    partition.shutdown().unwrap();
}

/// Same test suite runs against Counter — no modification.
#[test]
fn contract_suite_runs_against_counter() {
    contract_test_suite(Counter::new("counter"));
}

/// Same test suite runs against Accumulator — no modification.
#[test]
fn contract_suite_runs_against_accumulator() {
    contract_test_suite(Accumulator::new("accum"));
}

/// Same test suite runs against Doubler — no modification.
#[test]
fn contract_suite_runs_against_doubler() {
    contract_test_suite(Doubler::new("doubler"));
}

/// Generic contract test for the step-before-init error case.
/// All implementations must reject step() before init().
fn contract_test_step_before_init<P: Partition>(mut partition: P) {
    let result = partition.step(CanonicalInputs::standard_dt());
    assert!(result.is_err(), "step before init should fail");
}

#[test]
fn step_before_init_counter() {
    contract_test_step_before_init(Counter::new("c"));
}

#[test]
fn step_before_init_accumulator() {
    contract_test_step_before_init(Accumulator::new("a"));
}

#[test]
fn step_before_init_doubler() {
    contract_test_step_before_init(Doubler::new("d"));
}

/// Generic: id() returns non-empty string for all implementations.
fn contract_test_id_non_empty<P: Partition>(partition: P) {
    assert!(!partition.id().is_empty(), "partition id should be non-empty");
}

#[test]
fn id_non_empty_counter() {
    contract_test_id_non_empty(Counter::new("c"));
}

#[test]
fn id_non_empty_accumulator() {
    contract_test_id_non_empty(Accumulator::new("a"));
}

#[test]
fn id_non_empty_doubler() {
    contract_test_id_non_empty(Doubler::new("d"));
}
