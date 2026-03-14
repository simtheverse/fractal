//! A trivial partition that counts steps. Used to verify contract test patterns.

use crate::error::PartitionError;
use crate::partition::Partition;

/// Counts the number of steps taken. Simplest possible Partition implementation.
pub struct Counter {
    id: String,
    count: u64,
    initialized: bool,
}

impl Counter {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            count: 0,
            initialized: false,
        }
    }

    pub fn count(&self) -> u64 {
        self.count
    }
}

impl Partition for Counter {
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
        self.count += 1;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        self.initialized = false;
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        let count = i64::try_from(self.count).map_err(|_| {
            PartitionError::new(&self.id, "contribute_state", "count exceeds i64::MAX")
        })?;
        let mut table = toml::map::Map::new();
        table.insert("count".to_string(), toml::Value::Integer(count));
        Ok(toml::Value::Table(table))
    }

    fn load_state(&mut self, state: toml::Value) -> Result<(), PartitionError> {
        if let Some(table) = state.as_table() {
            if let Some(count) = table.get("count").and_then(|v| v.as_integer()) {
                self.count = u64::try_from(count).map_err(|_| {
                    PartitionError::new(&self.id, "load_state", "count is negative")
                })?;
                return Ok(());
            }
        }
        Err(PartitionError::new(&self.id, "load_state", "invalid state format"))
    }
}
