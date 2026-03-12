//! Bus trait abstraction for inter-partition communication.

use fpa_contract::message::Message;

/// Transport mode for the bus. Selectable at runtime without recompilation (FPA-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// In-process synchronous channels.
    InProcess,
    /// Asynchronous message-passing across threads or processes.
    Async,
    /// Network-based publish-subscribe.
    Network,
}

/// A reader handle for consuming messages from a bus subscription.
pub trait BusReader<M: Message>: Send {
    /// Read the next value(s) according to the message's delivery semantic.
    ///
    /// For LatestValue: returns the most recent published value, or None.
    /// For Queued: returns the next queued message, or None if queue is empty.
    fn read(&mut self) -> Option<M>;

    /// Read all available values. For LatestValue this returns at most one.
    /// For Queued this drains the queue.
    fn read_all(&mut self) -> Vec<M>;
}

/// Bus abstraction for inter-partition communication (FPA-004, FPA-007, FPA-008).
///
/// Each compositor owns a bus instance for its layer. Bus instances at different
/// layers are independent.
pub trait Bus: Send {
    /// Publish a message on the bus.
    fn publish<M: Message>(&self, msg: M);

    /// Subscribe to a message type. Returns a reader handle.
    fn subscribe<M: Message>(&self) -> Box<dyn BusReader<M>>;

    /// The transport mode this bus uses.
    fn transport(&self) -> Transport;

    /// A unique identifier for this bus instance (for layer scoping).
    fn id(&self) -> &str;
}
