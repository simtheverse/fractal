//! Test support: canonical inputs, trivial partition implementations, and test utilities.
//!
//! This module provides the canonical inputs and reference implementations
//! required by FPA-036. Contract tests use these to verify output properties.
//!
//! Contract versioning (FPA-039): canonical inputs and output property
//! definitions are scoped by contract version. Each version has its own
//! reference data so that an impl targeting version N is unaffected by
//! changes introduced in version N+1.

mod counter;
mod accumulator;
mod doubler;
mod messages;

pub use counter::Counter;
pub use accumulator::Accumulator;
pub use doubler::Doubler;
pub use messages::{CounterOutput, AccumulatorOutput, DoublerOutput};

/// Contract version identifier (FPA-039).
///
/// Each contract version defines its own canonical inputs, output property
/// assertions, and tolerances. Implementations target a specific version.
/// This is an enum rather than an open struct so that invalid versions are
/// unrepresentable and match arms are exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContractVersion {
    /// Version 1: original contract (Counter, Accumulator).
    V1,
    /// Version 2: extended contract (adds Doubler).
    V2,
}

/// Canonical input builder for contract tests (FPA-036).
///
/// Inputs are version-scoped (FPA-039): each contract version defines its
/// own canonical inputs so that adding v2 inputs does not affect v1 tests.
pub struct CanonicalInputs;

impl CanonicalInputs {
    /// Standard timestep for contract tests.
    pub fn standard_dt() -> f64 {
        1.0 / 60.0
    }

    /// A sequence of timesteps for multi-step contract tests.
    pub fn timestep_sequence(count: usize) -> Vec<f64> {
        vec![Self::standard_dt(); count]
    }

    /// Version-scoped canonical timestep (FPA-039).
    ///
    /// Each contract version may define a different standard timestep.
    /// v1 and v2 currently share the same value, but this structure
    /// ensures version isolation — changing v2's dt won't affect v1 tests.
    pub fn standard_dt_for_version(version: ContractVersion) -> f64 {
        match version {
            ContractVersion::V1 => 1.0 / 60.0,
            ContractVersion::V2 => 1.0 / 60.0,
        }
    }

    /// Version-scoped timestep sequence (FPA-039).
    pub fn timestep_sequence_for_version(version: ContractVersion, count: usize) -> Vec<f64> {
        vec![Self::standard_dt_for_version(version); count]
    }
}

/// Tolerance declarations for contract output properties (FPA-036).
///
/// Stated in the contract so that tests assert properties within declared
/// tolerances rather than exact values.
pub struct ContractTolerances;

impl ContractTolerances {
    /// Floating-point comparison tolerance for state values.
    pub const FLOAT_TOLERANCE: f64 = 1e-12;

    /// Version-scoped tolerance (FPA-039).
    pub fn float_tolerance_for_version(version: ContractVersion) -> f64 {
        match version {
            ContractVersion::V1 => 1e-12,
            ContractVersion::V2 => 1e-10,
        }
    }
}

/// Output property assertions for contract tests (FPA-036).
///
/// These assert structural and semantic properties of partition output,
/// not exact values. Version-scoped (FPA-039).
pub struct OutputProperties;

impl OutputProperties {
    /// Assert that contribute_state returns a valid, non-empty TOML table.
    pub fn assert_valid_state_table(state: &toml::Value) {
        assert!(state.is_table(), "contribute_state must return a TOML table");
        let table = state.as_table().unwrap();
        assert!(!table.is_empty(), "state table should contain at least one field");
    }

    /// Assert state round-trip: load then contribute should produce the same value.
    pub fn assert_state_roundtrip(
        partition: &mut dyn crate::partition::Partition,
        state: &toml::Value,
    ) {
        partition.load_state(state.clone()).unwrap();
        let reloaded = partition.contribute_state().unwrap();
        assert_eq!(
            state, &reloaded,
            "state round-trip should be identity"
        );
    }

    /// Assert monotonic progress: after N steps, numeric state fields are non-negative.
    /// This is an output property that holds for all well-behaved partitions
    /// regardless of implementation.
    pub fn assert_non_negative_numeric_fields(state: &toml::Value) {
        if let Some(table) = state.as_table() {
            for (key, val) in table {
                if let Some(f) = val.as_float() {
                    assert!(
                        f >= 0.0,
                        "field '{}' should be non-negative, got {}",
                        key,
                        f
                    );
                }
                if let Some(i) = val.as_integer() {
                    assert!(
                        i >= 0,
                        "field '{}' should be non-negative, got {}",
                        key,
                        i
                    );
                }
            }
        }
    }
}
