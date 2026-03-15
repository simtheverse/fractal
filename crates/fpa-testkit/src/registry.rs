//! Partition registry: maps implementation names to factory functions.
//!
//! Each domain registers its own partition types. The prototype pre-loads
//! Counter, Accumulator, and Doubler for test use.

use std::collections::HashMap;

use fpa_contract::{Partition, PartitionError};
use fpa_contract::test_support::{Accumulator, Counter, Doubler};

/// Factory function that creates a partition from its ID and config parameters.
pub type PartitionFactory =
    Box<dyn Fn(&str, &toml::Value) -> Result<Box<dyn Partition>, PartitionError> + Send>;

/// Registry mapping implementation name strings to partition factory functions.
///
/// Domain applications register their own types here. The prototype pre-loads
/// Counter, Accumulator, and Doubler via `with_test_partitions()`.
pub struct PartitionRegistry {
    factories: HashMap<String, PartitionFactory>,
}

impl PartitionRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register a factory function for a named implementation.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        factory: PartitionFactory,
    ) {
        self.factories.insert(name.into(), factory);
    }

    /// Convenience: register a simple factory that only needs the partition ID.
    pub fn register_simple<F>(&mut self, name: impl Into<String>, factory: F)
    where
        F: Fn(&str) -> Box<dyn Partition> + Send + 'static,
    {
        self.factories.insert(
            name.into(),
            Box::new(move |id, _config| Ok(factory(id))),
        );
    }

    /// Create a partition instance from its implementation name, ID, and config.
    pub fn create(
        &self,
        impl_name: &str,
        partition_id: &str,
        config: &toml::Value,
    ) -> Result<Box<dyn Partition>, PartitionError> {
        let factory = self.factories.get(impl_name).ok_or_else(|| {
            PartitionError::new(
                partition_id,
                "create",
                format!("unknown implementation '{}'", impl_name),
            )
        })?;
        factory(partition_id, config)
    }

    /// Pre-load Counter, Accumulator, and Doubler factories for testing.
    pub fn with_test_partitions() -> Self {
        let mut reg = Self::new();
        reg.register_simple("Counter", |id| Box::new(Counter::new(id)));
        reg.register_simple("Accumulator", |id| Box::new(Accumulator::new(id)));
        reg.register_simple("Doubler", |id| Box::new(Doubler::new(id)));
        reg
    }
}

impl Default for PartitionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
