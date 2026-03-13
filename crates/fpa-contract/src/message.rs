use std::any::Any;

/// Delivery semantic for a message type, declared as part of the message's contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliverySemantic {
    /// Each subscriber receives only the most recent value published after subscription.
    /// Suitable for continuous state.
    LatestValue,
    /// Each subscriber receives all instances published after subscription, in order.
    /// Suitable for requests/commands.
    Queued,
}

/// Trait for all inter-partition messages. Declared in the contract crate.
///
/// Messages are named, versioned types with a declared delivery semantic.
pub trait Message: Clone + Send + 'static + Any {
    /// The name of this message type for identification.
    const NAME: &'static str;

    /// Version of this message type's contract.
    const VERSION: u32;

    /// How the bus should deliver this message type.
    const DELIVERY: DeliverySemantic;
}
