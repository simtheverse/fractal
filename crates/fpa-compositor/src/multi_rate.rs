//! Multi-rate scheduling configuration (FPA-009).
//!
//! Allows partitions to run at different rates within the same compositor.
//! A rate multiplier of N means the partition steps N times per outer tick,
//! with `dt / N` for each sub-step.

use std::collections::HashMap;

/// Configuration mapping partition IDs to rate multipliers.
///
/// A rate multiplier determines how many sub-steps a partition takes per
/// outer compositor tick. The default rate is 1 (one step per tick).
#[derive(Debug, Clone)]
pub struct RateConfig {
    rates: HashMap<String, u32>,
}

impl RateConfig {
    /// Create a new empty rate configuration. All partitions default to rate 1.
    pub fn new() -> Self {
        Self {
            rates: HashMap::new(),
        }
    }

    /// Set the rate multiplier for a partition.
    ///
    /// A multiplier of 4 means the partition steps 4 times per outer tick
    /// with `dt / 4` each sub-step.
    ///
    /// # Panics
    /// Panics if `multiplier` is 0.
    pub fn set_rate(&mut self, partition_id: impl Into<String>, multiplier: u32) {
        assert!(multiplier > 0, "rate multiplier must be at least 1");
        self.rates.insert(partition_id.into(), multiplier);
    }

    /// Get the rate multiplier for a partition. Returns 1 if not configured.
    pub fn get_rate(&self, partition_id: &str) -> u32 {
        self.rates.get(partition_id).copied().unwrap_or(1)
    }
}

impl Default for RateConfig {
    fn default() -> Self {
        Self::new()
    }
}
