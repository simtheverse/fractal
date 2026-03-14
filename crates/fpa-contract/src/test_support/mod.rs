//! Test support: canonical inputs, trivial partition implementations, and test utilities.
//!
//! This module provides the canonical inputs and reference implementations
//! required by FPA-036. Contract tests use these to verify output properties.

mod counter;
mod accumulator;
mod messages;

pub use counter::Counter;
pub use accumulator::Accumulator;
pub use messages::{CounterOutput, AccumulatorOutput};

/// Canonical input builder for contract tests (FPA-036).
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
}
