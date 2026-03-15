//! Deferred bus wrapper for intra-tick message isolation (FPA-014).
//!
//! During Phase 2 of the compositor tick, partitions step and may publish
//! bus messages. Without isolation, messages published by partition A during
//! its step are immediately visible to partition B if B steps later — creating
//! a stepping-order dependence that violates FPA-014's guarantee of identical
//! results regardless of step order.
//!
//! `DeferredBus` wraps any `Bus` implementation and queues messages published
//! while deferred mode is active. Messages are flushed to the inner bus after
//! all partitions have stepped (the tick barrier), giving bus messages the
//! same one-tick-delay as SharedContext via the double buffer.

use crate::bus::{Bus, CloneableMessage, ErasedReader, Transport};
use fpa_contract::message::DeliverySemantic;
use std::any::TypeId;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A pending message captured during deferred mode.
struct PendingMessage {
    type_id: TypeId,
    semantic: DeliverySemantic,
    msg: Box<dyn CloneableMessage>,
}

/// Bus wrapper that queues messages during deferred mode and flushes them
/// after the tick barrier.
///
/// When `deferred` is false (the default), all operations delegate directly
/// to the inner bus with minimal overhead (atomic load on the fast path).
/// When `deferred` is true, `publish_erased` queues messages instead of
/// delivering them. Subscriptions always go to the inner bus — subscribers
/// see flushed messages on the next read after `flush()`.
pub struct DeferredBus {
    inner: Arc<dyn Bus>,
    deferred: AtomicBool,
    pending: Mutex<Vec<PendingMessage>>,
}

impl DeferredBus {
    /// Create a new `DeferredBus` wrapping the given bus.
    ///
    /// Starts in non-deferred mode (passthrough).
    pub fn new(inner: Arc<dyn Bus>) -> Self {
        Self {
            inner,
            deferred: AtomicBool::new(false),
            pending: Mutex::new(Vec::new()),
        }
    }

    /// Enable or disable deferred mode.
    ///
    /// When enabled, published messages are queued instead of delivered.
    /// When disabled, published messages go directly to the inner bus.
    pub fn set_deferred(&self, deferred: bool) {
        self.deferred.store(deferred, Ordering::SeqCst);
    }

    /// Flush all pending messages to the inner bus.
    ///
    /// Messages are delivered in the order they were published, preserving
    /// the Queued delivery semantic's ordering guarantee (FPA-007).
    pub fn flush(&self) {
        let messages: Vec<PendingMessage> = {
            let mut pending = self.pending.lock().unwrap();
            std::mem::take(&mut *pending)
        };
        for msg in messages {
            self.inner.publish_erased(msg.type_id, msg.semantic, msg.msg);
        }
    }

    /// Get a reference to the inner bus.
    pub fn inner(&self) -> &Arc<dyn Bus> {
        &self.inner
    }
}

impl Bus for DeferredBus {
    fn publish_erased(
        &self,
        type_id: TypeId,
        semantic: DeliverySemantic,
        msg: Box<dyn CloneableMessage>,
    ) {
        if self.deferred.load(Ordering::SeqCst) {
            let mut pending = self.pending.lock().unwrap();
            pending.push(PendingMessage {
                type_id,
                semantic,
                msg,
            });
        } else {
            self.inner.publish_erased(type_id, semantic, msg);
        }
    }

    fn subscribe_erased(
        &self,
        type_id: TypeId,
        semantic: DeliverySemantic,
    ) -> Box<dyn ErasedReader> {
        self.inner.subscribe_erased(type_id, semantic)
    }

    fn transport(&self) -> Transport {
        self.inner.transport()
    }

    fn id(&self) -> &str {
        self.inner.id()
    }
}
