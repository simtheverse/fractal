//! System: public entry point for composing and running FPA systems (FPA-034).
//!
//! Takes a `CompositionFragment` + `PartitionRegistry` + `Bus` and creates a
//! compositor. This is the "operator entry point" — system tests use the same
//! API available to operators and embedders.

use std::sync::Arc;

use fpa_bus::Bus;
use fpa_compositor::compositor::Compositor;
use fpa_config::CompositionFragment;
use fpa_contract::{Partition, PartitionError};

use crate::registry::PartitionRegistry;

/// Error type for system-level operations.
#[derive(Debug)]
pub enum SystemError {
    Partition(PartitionError),
    Config(String),
}

impl std::fmt::Display for SystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemError::Partition(e) => write!(f, "{}", e),
            SystemError::Config(msg) => write!(f, "config error: {}", msg),
        }
    }
}

impl std::error::Error for SystemError {}

impl From<PartitionError> for SystemError {
    fn from(e: PartitionError) -> Self {
        SystemError::Partition(e)
    }
}

/// A composed FPA system ready to run.
///
/// Created from a composition fragment, partition registry, and bus.
/// Provides the canonical operator entry point for FPA applications.
pub struct System {
    compositor: Compositor,
    /// Timestep from the fragment's system config, if specified.
    dt: Option<f64>,
}

impl System {
    /// Build a system from a composition fragment.
    ///
    /// Iterates the fragment's partition entries in sorted order (for
    /// deterministic composition), creates each via the registry, and
    /// assembles a compositor with the given bus.
    pub fn from_fragment(
        fragment: &CompositionFragment,
        registry: &PartitionRegistry,
        bus: Arc<dyn Bus>,
    ) -> Result<Self, SystemError> {
        let mut partitions: Vec<Box<dyn Partition>> = Vec::new();

        // Sort partition IDs for deterministic ordering. HashMap iteration
        // order is non-deterministic, which would cause the compositor's
        // stepping order to vary across runs.
        let mut partition_ids: Vec<&String> = fragment.partitions.keys().collect();
        partition_ids.sort();

        for id in partition_ids {
            let config = &fragment.partitions[id];
            let impl_name = config.implementation.as_deref().ok_or_else(|| {
                SystemError::Config(format!(
                    "partition '{}' has no implementation specified",
                    id
                ))
            })?;

            let config_value = toml::Value::try_from(config).map_err(|e| {
                SystemError::Config(format!(
                    "failed to serialize config for partition '{}': {}",
                    id, e
                ))
            })?;

            let partition = registry
                .create(impl_name, id, &config_value)
                .map_err(SystemError::Partition)?;
            partitions.push(partition);
        }

        // Extract timestep from system config if present.
        let dt = fragment
            .system
            .get("timestep")
            .and_then(|v| v.as_float());

        let compositor = Compositor::new(partitions, bus);
        Ok(System { compositor, dt })
    }

    /// Run the system for a given number of ticks.
    ///
    /// Uses the timestep from the fragment's system config if available,
    /// otherwise uses the provided `dt`.
    ///
    /// Performs: init -> run_tick x N -> dump -> shutdown -> return state.
    pub fn run(&mut self, ticks: u64, dt: f64) -> Result<toml::Value, SystemError> {
        let actual_dt = self.dt.unwrap_or(dt);
        self.compositor.init()?;

        for _ in 0..ticks {
            self.compositor.run_tick(actual_dt)?;
        }

        let state = self.compositor.dump()?;
        self.compositor.shutdown()?;

        Ok(state)
    }

    /// The timestep from the fragment's system config, if specified.
    pub fn dt(&self) -> Option<f64> {
        self.dt
    }

    /// Access the compositor for advanced operations.
    pub fn compositor(&self) -> &Compositor {
        &self.compositor
    }

    /// Mutably access the compositor for advanced operations.
    pub fn compositor_mut(&mut self) -> &mut Compositor {
        &mut self.compositor
    }
}
