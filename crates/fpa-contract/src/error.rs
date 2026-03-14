use std::fmt;

/// Error type for partition operations.
#[derive(Debug)]
pub struct PartitionError {
    pub partition_id: String,
    pub operation: String,
    pub message: String,
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
    /// Optional layer depth indicating which compositor layer produced this error.
    pub layer_depth: Option<u32>,
}

impl PartitionError {
    pub fn new(partition_id: impl Into<String>, operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            partition_id: partition_id.into(),
            operation: operation.into(),
            message: message.into(),
            source: None,
            layer_depth: None,
        }
    }

    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Attach layer depth information to this error.
    pub fn with_layer_depth(mut self, depth: u32) -> Self {
        self.layer_depth = Some(depth);
        self
    }
}

impl fmt::Display for PartitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(depth) = self.layer_depth {
            write!(
                f,
                "partition '{}' (layer {}) failed during {}: {}",
                self.partition_id, depth, self.operation, self.message
            )
        } else {
            write!(
                f,
                "partition '{}' failed during {}: {}",
                self.partition_id, self.operation, self.message
            )
        }
    }
}

impl std::error::Error for PartitionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}
