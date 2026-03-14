//! A trivial partition that accumulates dt values. Alternative implementation pattern.

use crate::error::PartitionError;
use crate::partition::Partition;

/// Accumulates the sum of all dt values passed to step(). Demonstrates an
/// alternative partition implementation conforming to the same contract.
pub struct Accumulator {
    id: String,
    total: f64,
    initialized: bool,
}

impl Accumulator {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            total: 0.0,
            initialized: false,
        }
    }

    pub fn total(&self) -> f64 {
        self.total
    }
}

impl Partition for Accumulator {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        self.initialized = true;
        Ok(())
    }

    fn step(&mut self, dt: f64) -> Result<(), PartitionError> {
        if !self.initialized {
            return Err(PartitionError::new(&self.id, "step", "not initialized"));
        }
        self.total += dt;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        self.initialized = false;
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        let mut table = toml::map::Map::new();
        table.insert("total".to_string(), toml::Value::Float(self.total));
        Ok(toml::Value::Table(table))
    }

    fn load_state(&mut self, state: toml::Value) -> Result<(), PartitionError> {
        if let Some(table) = state.as_table() {
            if let Some(total) = table.get("total").and_then(|v| v.as_float()) {
                self.total = total;
                return Ok(());
            }
        }
        Err(PartitionError::new(&self.id, "load_state", "invalid state format"))
    }
}
