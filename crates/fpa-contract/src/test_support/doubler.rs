//! A trivial partition that doubles a value each step. Targets contract version 2.
//!
//! Used to demonstrate contract version isolation (FPA-039): an impl targeting
//! version 2 is unaffected by version 1 reference data and vice versa.

use crate::error::PartitionError;
use crate::partition::Partition;

/// Doubles an internal value each step. Starts at 1.0 and doubles on each step.
/// Targets contract version 2 to demonstrate version isolation.
pub struct Doubler {
    id: String,
    value: f64,
    initialized: bool,
}

impl Doubler {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            value: 1.0,
            initialized: false,
        }
    }

    pub fn value(&self) -> f64 {
        self.value
    }
}

impl Partition for Doubler {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        self.initialized = true;
        Ok(())
    }

    fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
        if !self.initialized {
            return Err(PartitionError::new(&self.id, "step", "not initialized"));
        }
        self.value *= 2.0;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        self.initialized = false;
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        let mut table = toml::map::Map::new();
        table.insert("value".to_string(), toml::Value::Float(self.value));
        Ok(toml::Value::Table(table))
    }

    fn load_state(&mut self, state: toml::Value) -> Result<(), PartitionError> {
        if let Some(table) = state.as_table() {
            if let Some(value) = table.get("value").and_then(|v| v.as_float()) {
                self.value = value;
                return Ok(());
            }
        }
        Err(PartitionError::new(&self.id, "load_state", "invalid state format"))
    }
}
