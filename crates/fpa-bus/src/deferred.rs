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
/// enqueues in one critical section, and `end_deferred` atomically
/// disables and drains in one critical section. This prevents a TOCTOU
/// race where a concurrent publisher could slip between flag change
/// and queue drain, stranding or bypassing messages.
struct DeferredState {
    deferred: bool,
    pending: Vec<PendingMessage>,
}

/// Bus wrapper that queues messages during deferred mode and flushes them
/// after the tick barrier.
///
/// When `deferred` is false (the default), all operations delegate directly
/// to the inner bus. When `deferred` is true, `publish_erased` queues
/// messages instead of delivering them. Subscriptions delegate to the
/// inner bus — subscribers see flushed messages on the next read after
/// `end_deferred()`.
///
/// The deferred flag and pending queue share a single mutex to satisfy
/// the `Send + Sync` contract of the `Bus` trait under concurrent use.
/// The primary API is `begin_deferred()` / `end_deferred()`, which
/// provide correct-by-construction lifecycle management. `end_deferred()`
/// atomically disables deferral and drains the queue in a single critical
/// section, eliminating any window where a concurrent publisher could
/// bypass the queue or strand messages. Note: the actual delivery to the
/// inner bus happens after the lock is released, so a concurrent publisher
/// could interleave with the flushed batch — the guarantee is that no
/// message is lost or bypassed, not batch-contiguous delivery.
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

    /// Enter deferred mode: subsequent publishes are queued instead of
    /// delivered immediately.
    ///
    /// Pair with `end_deferred()` to flush the queued batch and return
    /// to passthrough mode.
    pub fn begin_deferred(&self) {
        self.state.lock().unwrap().deferred = true;
    }

    /// Exit deferred mode and flush all pending messages to the inner bus.
    ///
    /// Atomically disables deferred mode and drains the pending queue in a
    /// single critical section. Messages are then delivered to the inner bus
    /// in publish order, preserving the Queued delivery semantic's ordering
    /// guarantee (FPA-007).
    ///
    /// This combined operation eliminates the TOCTOU window that would exist
    /// if disable and flush were separate calls: no concurrent publisher can
    /// observe deferred=false while messages remain in the queue.
    ///
    /// Safe to call when not in deferred mode (no-op).
    pub fn end_deferred(&self) {
        let messages = {
            let mut state = self.state.lock().unwrap();
            state.deferred = false;
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
