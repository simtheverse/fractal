//! Bus trait abstraction for inter-partition communication.
//!
//! The bus is split into two layers:
//! - `Bus`: an object-safe core trait that supports `dyn Bus` for runtime
//!   transport selection (FPA-004).
//! - `BusExt`: a typed extension trait with generic methods, blanket-implemented
//!   for all `Bus` types (including `dyn Bus`). Partitions use this API.
//!
//! This design preserves full compile-time type safety at the partition API
//! while enabling runtime transport selection without recompilation.

use fpa_contract::message::{DeliverySemantic, Message};
use std::any::{Any, TypeId};

/// Transport mode for the bus (FPA-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// In-process synchronous channels.
    InProcess,
    /// Asynchronous message-passing across threads or processes.
    Async,
    /// Network-based publish-subscribe.
    Network,
}

/// Object-safe wrapper for cloneable, type-erased messages.
///
/// All `Message` types satisfy the bounds (`Clone + Send + 'static`) via the
/// blanket impl. This trait enables the bus to clone messages for multi-subscriber
/// delivery without knowing the concrete type.
pub trait CloneableMessage: Any + Send {
    /// Clone this message into a new box.
    fn clone_box(&self) -> Box<dyn CloneableMessage>;

    /// Convert into a `Box<dyn Any + Send>` for downcasting.
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send>;
}

impl<T: Clone + Send + 'static> CloneableMessage for T {
    fn clone_box(&self) -> Box<dyn CloneableMessage> {
        Box::new(self.clone())
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any + Send> {
        self
    }
}

/// Object-safe reader for type-erased messages.
///
/// Bus implementations return this from `subscribe_erased`. The `BusExt`
/// layer wraps it in a `TypedReader<M>` to restore compile-time types.
pub trait ErasedReader: Send {
    /// Read the next value as a type-erased `Box<dyn Any>`.
    fn read_erased(&mut self) -> Option<Box<dyn Any + Send>>;

    /// Read all available values as type-erased boxes.
    fn read_all_erased(&mut self) -> Vec<Box<dyn Any + Send>>;
}

/// Object-safe bus trait for inter-partition communication (FPA-004, FPA-007, FPA-008).
///
/// Each compositor owns a bus instance for its layer. Bus instances at different
/// layers are independent. This trait is object-safe (`dyn Bus`) to support
/// runtime transport selection.
pub trait Bus: Send + Sync {
    /// Publish a type-erased message on the bus.
    ///
    /// `type_id` identifies the message type. `semantic` determines delivery
    /// behavior (latest-value or queued). Callers should use `BusExt::publish`
    /// for the typed API.
    fn publish_erased(
        &self,
        type_id: TypeId,
        semantic: DeliverySemantic,
        msg: Box<dyn CloneableMessage>,
    );

    /// Subscribe to a message type by TypeId. Returns an erased reader.
    ///
    /// `semantic` determines delivery behavior for this subscription.
    /// Callers should use `BusExt::subscribe` for the typed API.
    fn subscribe_erased(
        &self,
        type_id: TypeId,
        semantic: DeliverySemantic,
    ) -> Box<dyn ErasedReader>;

    /// The transport mode this bus uses.
    fn transport(&self) -> Transport;

    /// A unique identifier for this bus instance (for layer scoping).
    fn id(&self) -> &str;
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

/// Typed reader wrapping an `ErasedReader`, restoring compile-time types.
pub struct TypedReader<M: Message> {
    inner: Box<dyn ErasedReader>,
    _marker: std::marker::PhantomData<M>,
}

impl<M: Message> BusReader<M> for TypedReader<M> {
    fn read(&mut self) -> Option<M> {
        self.inner
            .read_erased()
            .and_then(|v| v.downcast::<M>().ok())
            .map(|v| *v)
    }

    fn read_all(&mut self) -> Vec<M> {
        self.inner
            .read_all_erased()
            .into_iter()
            .filter_map(|v| v.downcast::<M>().ok().map(|v| *v))
            .collect()
    }
}

/// Typed extension trait for `Bus`. Provides compile-time-safe `publish`
/// and `subscribe` methods.
///
/// Blanket-implemented for all `T: Bus + ?Sized`, so it works on both
/// concrete bus types and `dyn Bus`.
pub trait BusExt: Bus {
    /// Publish a typed message on the bus.
    fn publish<M: Message>(&self, msg: M) {
        self.publish_erased(TypeId::of::<M>(), M::DELIVERY, Box::new(msg));
    }

    /// Subscribe to a typed message. Returns a reader that yields `M` values.
    fn subscribe<M: Message>(&self) -> TypedReader<M> {
        TypedReader {
            inner: self.subscribe_erased(TypeId::of::<M>(), M::DELIVERY),
            _marker: std::marker::PhantomData,
        }
    }
}

/// Blanket implementation: any `Bus` (including `dyn Bus`) gets typed methods.
impl<T: Bus + ?Sized> BusExt for T {}
