//! Composition fragment parsing, inheritance, and named presets.

pub mod convert;
pub mod error;
pub mod fragment;
pub mod loader;
pub mod merge;
pub mod registry;

pub use error::ConfigError;
pub use fragment::{CompositionFragment, EventConfig, PartitionConfig, TriggerConfig};
pub use loader::{load_from_str, resolve_extends};
pub use merge::deep_merge;
pub use registry::FragmentRegistry;
