use crate::error::PartitionError;

/// Core trait for all partitions at every layer of the fractal structure.
///
/// Implementations provide domain-specific behavior. The compositor invokes
/// these methods according to the active execution strategy.
pub trait Partition: Send {
    /// Unique identifier for this partition instance.
    fn id(&self) -> &str;

    /// Initialize the partition. Called once before any stepping occurs.
    fn init(&mut self) -> Result<(), PartitionError>;

    /// Execute one processing step with the given time delta.
    fn step(&mut self, dt: f64) -> Result<(), PartitionError>;

    /// Shut down the partition, releasing resources.
    fn shutdown(&mut self) -> Result<(), PartitionError>;

    /// Contribute this partition's current state as a TOML value.
    fn contribute_state(&self) -> Result<toml::Value, PartitionError>;

    /// Load state from a TOML value, replacing current state.
    fn load_state(&mut self, state: toml::Value) -> Result<(), PartitionError>;

    /// Downcast support for nested composition.
    ///
    /// Compositors override this to enable inner signal collection and
    /// other cross-layer interactions. Default returns `None`.
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
}
