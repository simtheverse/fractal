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
use std::sync::{Arc, Mutex};

/// A pending message captured during deferred mode.
struct PendingMessage {
    type_id: TypeId,
    semantic: DeliverySemantic,
    msg: Box<dyn CloneableMessage>,
}

/// Deferred state: the flag and pending queue are guarded by a single
/// mutex so that `publish_erased` atomically observes the flag and
/// enqueues in one critical section. This prevents a TOCTOU race where
/// a concurrent `set_deferred(false)` + `flush()` could drain the queue
/// between the flag check and the enqueue, stranding messages.
struct DeferredState {
    deferred: bool,
    pending: Vec<PendingMessage>,
}

/// Bus wrapper that queues messages during deferred mode and flushes them
/// after the tick barrier.
///
/// When `deferred` is false (the default), all operations delegate directly
/// to the inner bus. When `deferred` is true, `publish_erased` queues
/// messages instead of delivering them. Subscriptions always go to the
/// inner bus — subscribers see flushed messages on the next read after
/// `flush()`.
///
/// The deferred flag and pending queue share a single mutex to satisfy
/// the `Send + Sync` contract of the `Bus` trait under concurrent use.
pub struct DeferredBus {
    inner: Arc<dyn Bus>,
    state: Mutex<DeferredState>,
}

impl DeferredBus {
    /// Create a new `DeferredBus` wrapping the given bus.
    ///
    /// Starts in non-deferred mode (passthrough).
    pub fn new(inner: Arc<dyn Bus>) -> Self {
        Self {
            inner,
            state: Mutex::new(DeferredState {
                deferred: false,
                pending: Vec::new(),
            }),
        }
    }

    /// Enable or disable deferred mode.
    ///
    /// When enabled, published messages are queued instead of delivered.
    /// When disabled, published messages go directly to the inner bus.
    pub fn set_deferred(&self, deferred: bool) {
        self.state.lock().unwrap().deferred = deferred;
    }

    /// Flush all pending messages to the inner bus.
    ///
    /// Messages are delivered in the order they were published, preserving
    /// the Queued delivery semantic's ordering guarantee (FPA-007).
    pub fn flush(&self) {
        let messages: Vec<PendingMessage> = {
            let mut state = self.state.lock().unwrap();
            std::mem::take(&mut state.pending)
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
        {
            let mut state = self.state.lock().unwrap();
            if state.deferred {
                state.pending.push(PendingMessage {
                    type_id,
                    semantic,
                    msg,
                });
                return;
            }
        }
        self.inner.publish_erased(type_id, semantic, msg);
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
