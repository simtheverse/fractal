//! Config-driven composition: fragment + registry + bus → Compositor.
//!
//! This is the canonical operator entry point for FPA applications. Given a
//! composition fragment, a partition registry, and a bus, it creates partition
//! instances and assembles a ready-to-use compositor.

use std::collections::HashMap;
use std::sync::Arc;

use fpa_bus::Bus;
use fpa_config::{CompositionFragment, EventConfig};
use fpa_contract::{Partition, PartitionError};
use fpa_events::{EventDefinition, EventEngine};

use crate::compositor::Compositor;

/// Factory function that creates a partition from its ID, config, and bus.
///
/// The bus is the compositor's layer bus — partitions that need to publish
/// or subscribe to messages use this to participate in inter-partition
/// communication. Partitions that don't need the bus simply ignore it.
pub type PartitionFactory = Box<
    dyn Fn(&str, &toml::Value, &Arc<dyn Bus>) -> Result<Box<dyn Partition>, PartitionError>
        + Send,
>;

/// Registry mapping implementation name strings to partition factory functions.
///
/// Domain applications register their own partition types here. The registry
/// is the bridge between TOML config (which names implementations as strings)
/// and Rust code (which constructs typed partition instances).
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
    pub fn register(&mut self, name: impl Into<String>, factory: PartitionFactory) {
        self.factories.insert(name.into(), factory);
    }

    /// Convenience: register a simple factory that only needs the partition ID.
    ///
    /// The config and bus parameters are ignored. Use `register` for partitions
    /// that need config or bus access.
    pub fn register_simple<F>(&mut self, name: impl Into<String>, factory: F)
    where
        F: Fn(&str) -> Box<dyn Partition> + Send + 'static,
    {
        self.factories.insert(
            name.into(),
            Box::new(move |id, _config, _bus| Ok(factory(id))),
        );
    }

    /// Create a partition instance from its implementation name, ID, config, and bus.
    pub fn create(
        &self,
        impl_name: &str,
        partition_id: &str,
        config: &toml::Value,
        bus: &Arc<dyn Bus>,
    ) -> Result<Box<dyn Partition>, PartitionError> {
        let factory = self.factories.get(impl_name).ok_or_else(|| {
            PartitionError::new(
                partition_id,
                "create",
                format!("unknown implementation '{}'", impl_name),
            )
        })?;
        factory(partition_id, config, bus)
    }

    /// Pre-load Counter, Accumulator, and Doubler factories for testing.
    pub fn with_test_partitions() -> Self {
        use fpa_contract::test_support::{Accumulator, Counter, Doubler};
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

/// Error from the composition process.
#[derive(Debug)]
pub enum ComposeError {
    /// A partition could not be created.
    Partition(PartitionError),
    /// The composition fragment is invalid.
    Config(String),
}

impl std::fmt::Display for ComposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComposeError::Partition(e) => write!(f, "{}", e),
            ComposeError::Config(msg) => write!(f, "config error: {}", msg),
        }
    }
}

impl std::error::Error for ComposeError {}

impl From<PartitionError> for ComposeError {
    fn from(e: PartitionError) -> Self {
        ComposeError::Partition(e)
    }
}

/// Compose a compositor from a configuration fragment.
///
/// This is the canonical operator entry point. It creates partition instances
/// from the fragment's partition entries using the registry, wires events
/// from the fragment, and returns a ready-to-use compositor.
///
/// The bus is passed to each partition factory so that partitions needing
/// inter-partition communication are guaranteed to use the compositor's bus.
///
/// Partition iteration follows `BTreeMap` ordering (alphabetical by ID),
/// which determines stepping order within a tick.
pub fn compose(
    fragment: &CompositionFragment,
    registry: &PartitionRegistry,
    bus: Arc<dyn Bus>,
) -> Result<Compositor, ComposeError> {
    let mut partitions: Vec<Box<dyn Partition>> = Vec::new();

    for (id, config) in &fragment.partitions {
        let impl_name = config.implementation.as_deref().ok_or_else(|| {
            ComposeError::Config(format!(
                "partition '{}' has no implementation specified",
                id
            ))
        })?;

        let config_value = toml::Value::try_from(config).map_err(|e| {
            ComposeError::Config(format!(
                "failed to serialize config for partition '{}': {}",
                id, e
            ))
        })?;

        let partition = registry
            .create(impl_name, id, &config_value, &bus)
            .map_err(ComposeError::Partition)?;
        partitions.push(partition);
    }

    let mut compositor = Compositor::new(partitions, bus);

    // Wire system-level events from the fragment.
    let event_defs = convert_events(&fragment.events)?;
    if !event_defs.is_empty() {
        compositor.set_event_engine(EventEngine::new(event_defs));
    }

    Ok(compositor)
}

/// Convert event configs to event definitions.
fn convert_events(events: &[EventConfig]) -> Result<Vec<EventDefinition>, ComposeError> {
    events
        .iter()
        .map(|config| {
            EventDefinition::try_from(config).map_err(|e| {
                ComposeError::Config(format!("invalid event '{}': {}", config.id, e))
            })
        })
        .collect()
}
