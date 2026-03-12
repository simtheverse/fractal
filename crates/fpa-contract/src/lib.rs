//! Contract traits, message types, and delivery semantics for the Fractal Partition Architecture.

pub mod error;
pub mod message;
pub mod partition;
pub mod test_support;

pub use error::PartitionError;
pub use message::{DeliverySemantic, Message};
pub use partition::Partition;
