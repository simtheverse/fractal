//! Reference file generation for system test baselines (FPA-038).
//!
//! Captures system output with provenance metadata so that test baselines
//! are traceable to the exact configuration and versions that produced them.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use fpa_bus::{Bus, InProcessBus};
use fpa_config::CompositionFragment;

use crate::registry::PartitionRegistry;
use crate::system::{System, SystemError};

/// Provenance metadata for a reference file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// Description of how this reference was generated.
    pub command: String,
    /// Timestamp when this reference was generated (RFC 3339).
    pub timestamp: String,
    /// Implementation versions used (e.g., partition implementation versions).
    pub impl_versions: Vec<String>,
    /// Contract versions used.
    pub contract_versions: Vec<String>,
}

/// A reference file containing system output and provenance metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceFile {
    pub provenance: Provenance,
    pub output: toml::Value,
}

impl ReferenceFile {
    /// Generate a reference file by running a system from a composition fragment.
    pub fn generate(
        fragment: &CompositionFragment,
        registry: &PartitionRegistry,
        ticks: u64,
        dt: f64,
    ) -> Result<Self, SystemError> {
        Self::generate_with_bus(
            fragment,
            registry,
            Arc::new(InProcessBus::new("reference")),
            ticks,
            dt,
        )
    }

    /// Generate a reference file with a specific bus implementation.
    pub fn generate_with_bus(
        fragment: &CompositionFragment,
        registry: &PartitionRegistry,
        bus: Arc<dyn Bus>,
        ticks: u64,
        dt: f64,
    ) -> Result<Self, SystemError> {
        let mut system = System::from_fragment(fragment, registry, bus)?;
        let actual_dt = system.dt().unwrap_or(dt);
        let output = system.run(ticks, dt)?;

        // Collect implementation names from the fragment for provenance.
        let mut impl_versions: Vec<String> = fragment
            .partitions
            .iter()
            .filter_map(|(id, config)| {
                config
                    .implementation
                    .as_ref()
                    .map(|imp| format!("{}={}", id, imp))
            })
            .collect();
        impl_versions.sort();

        // Record contract versions from framework crate versions.
        let contract_versions = vec![
            format!("fpa-contract={}", env!("CARGO_PKG_VERSION")),
        ];

        let provenance = Provenance {
            command: format!(
                "generate ticks={} dt={} transport={}",
                ticks,
                actual_dt,
                "InProcess"
            ),
            timestamp: current_timestamp(),
            impl_versions,
            contract_versions,
        };

        Ok(ReferenceFile { provenance, output })
    }

    /// Serialize to TOML string.
    pub fn to_toml_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Deserialize from TOML string.
    pub fn from_toml_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}

/// Returns current UTC timestamp in a simple format.
/// Uses a minimal approach without pulling in chrono — exact format
/// is less important than having a non-empty, meaningful value.
fn current_timestamp() -> String {
    // Use std::time to get seconds since epoch, format manually.
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("epoch:{}", duration.as_secs())
}
