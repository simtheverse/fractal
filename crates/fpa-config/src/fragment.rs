use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// A composition fragment at any scope in the fractal structure.
/// Can represent layer 0 (system), layer 1 (partition), or deeper scopes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionFragment {
    /// Optional path to a base fragment this one extends.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,

    /// System-level parameters (timestep, transport mode, etc.)
    #[serde(default)]
    pub system: BTreeMap<String, toml::Value>,

    /// Partition selections — maps partition name to its config.
    /// Uses BTreeMap for deterministic iteration order: partition
    /// creation and stepping order is the same across runs.
    #[serde(default)]
    pub partitions: BTreeMap<String, PartitionConfig>,

    /// System-level events
    #[serde(default)]
    pub events: Vec<EventConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionConfig {
    /// The implementation to use for this partition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implementation: Option<String>,

    /// Partition-scoped events
    #[serde(default)]
    pub events: Vec<EventConfig>,

    /// Partition-specific parameters
    #[serde(default, flatten)]
    pub params: HashMap<String, toml::Value>,
}

/// Configuration for a single event definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConfig {
    /// Unique identifier for this event.
    pub id: String,
    /// Trigger specification.
    pub trigger: TriggerConfig,
    /// Action identifier to invoke when fired.
    pub action: String,
    /// Optional scope for the action.
    #[serde(default)]
    pub scope: Option<String>,
    /// Parameters passed to the action.
    #[serde(default)]
    pub parameters: HashMap<String, toml::Value>,
}

/// Trigger configuration, tagged by type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TriggerConfig {
    /// Fire at (or after) the specified simulation time.
    #[serde(rename = "time")]
    Time { at: f64 },
    /// Fire when a named signal satisfies a predicate.
    #[serde(rename = "condition")]
    Condition {
        signal: String,
        predicate: String,
        value: f64,
    },
}
