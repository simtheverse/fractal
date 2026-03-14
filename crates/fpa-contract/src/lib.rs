//! Contract traits, message types, and delivery semantics for the Fractal Partition Architecture.

pub mod error;
pub mod message;
pub mod partition;
pub mod shared_context;
pub mod state_contribution;
pub mod state_machine;
pub mod test_support;

pub use error::PartitionError;
pub use message::{DeliverySemantic, Message};
pub use partition::Partition;
pub use shared_context::SharedContext;
pub use state_contribution::StateContribution;
pub use state_machine::{ExecutionState, StateMachine, TransitionError, TransitionRequest};
