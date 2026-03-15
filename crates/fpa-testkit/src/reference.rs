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
    /// Timestamp when this reference was generated (RFC 3339 UTC).
    pub timestamp: String,
    /// Partition implementations used (e.g., "counter=Counter").
    pub implementations: Vec<String>,
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
        let transport = bus.transport();
        let mut system = System::from_fragment(fragment, registry, bus)?;
        let actual_dt = system.dt().unwrap_or(dt);
        let output = system.run(ticks, dt)?;

        // Collect implementation names from the fragment for provenance.
        let mut implementations: Vec<String> = fragment
            .partitions
            .iter()
            .filter_map(|(id, config)| {
                config
                    .implementation
                    .as_ref()
                    .map(|imp| format!("{}={}", id, imp))
            })
            .collect();
        implementations.sort();

        // Record contract versions. In a workspace where all crates share
        // a version, this is the workspace version. When versions diverge,
        // each crate's version should be recorded individually.
        let contract_versions = vec![
            format!("fpa-contract={}", fpa_contract::VERSION),
        ];

        let provenance = Provenance {
            command: format!(
                "generate ticks={} dt={} transport={:?}",
                ticks,
                actual_dt,
                transport
            ),
            timestamp: current_timestamp(),
            implementations,
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

/// Returns current UTC timestamp in RFC 3339 format.
fn current_timestamp() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Manual UTC breakdown — avoids chrono/time dependency.
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Days since epoch to Y-M-D (civil calendar from days).
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
/// Algorithm from Howard Hinnant's chrono-compatible date library.
fn days_to_ymd(days: u64) -> (i64, u64, u64) {
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
