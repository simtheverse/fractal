//! Async bus implementation using tokio channels.
//!
//! For LatestValue, each subscriber has a shared slot (`Arc<Mutex<Option>>`)
//! that the publisher overwrites in place — no unbounded queue growth.
//! For Queued, messages are delivered via `tokio::sync::mpsc::unbounded_channel`.
//!
//! The external API remains synchronous (FPA-004); internally, the tokio
//! unbounded channel's `send()` and `try_recv()` are both non-async.

use crate::bus::{Bus, CloneableMessage, ErasedReader, Transport};
use fpa_contract::message::DeliverySemantic;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::mpsc;

/// A subscriber entry, either a shared slot (LatestValue) or a channel sender (Queued).
enum Subscriber {
    LatestValue(Weak<Mutex<Option<Box<dyn Any + Send>>>>),
    Queued(mpsc::UnboundedSender<Box<dyn Any + Send>>),
}

/// Holds the per-message-type subscriber list (type-erased).
struct ChannelState {
    subscribers: Vec<Subscriber>,
}

/// Async bus using tokio channels. Supports both delivery semantics (FPA-007).
pub struct AsyncBus {
    id: String,
    channels: Arc<Mutex<HashMap<TypeId, ChannelState>>>,
}

impl AsyncBus {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            channels: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn ensure_channel(
        channels: &mut HashMap<TypeId, ChannelState>,
        type_id: TypeId,
    ) {
        channels.entry(type_id).or_insert_with(|| ChannelState {
            subscribers: Vec::new(),
        });
    }
}

impl Bus for AsyncBus {
    fn publish_erased(
        &self,
        type_id: TypeId,
        _semantic: DeliverySemantic,
        msg: Box<dyn CloneableMessage>,
    ) {
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel(&mut channels, type_id);

        let channel = channels.get_mut(&type_id).unwrap();

        // Prune dead subscribers
        channel.subscribers.retain(|sub| match sub {
            Subscriber::LatestValue(weak) => weak.strong_count() > 0,
            Subscriber::Queued(tx) => !tx.is_closed(),
        });

        // Deliver to each subscriber
        for sub in &channel.subscribers {
            match sub {
                Subscriber::LatestValue(weak) => {
                    if let Some(slot) = weak.upgrade() {
                        let cloned: Box<dyn Any + Send> = msg.clone_box().into_any();
                        *slot.lock().unwrap() = Some(cloned);
                    }
                }
                Subscriber::Queued(tx) => {
                    let cloned: Box<dyn Any + Send> = msg.clone_box().into_any();
                    let _ = tx.send(cloned);
                }
            }
        }
    }

    fn subscribe_erased(
        &self,
        type_id: TypeId,
        semantic: DeliverySemantic,
    ) -> Box<dyn ErasedReader> {
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel(&mut channels, type_id);

        let channel = channels.get_mut(&type_id).unwrap();

        match semantic {
            DeliverySemantic::LatestValue => {
                let slot = Arc::new(Mutex::new(None));
                channel.subscribers.push(Subscriber::LatestValue(Arc::downgrade(&slot)));
                Box::new(LatestValueReader { slot })
            }
            DeliverySemantic::Queued => {
                let (tx, rx) = mpsc::unbounded_channel();
                channel.subscribers.push(Subscriber::Queued(tx));
                Box::new(QueuedReader { rx })
            }
        }
    }

    fn transport(&self) -> Transport {
        Transport::Async
    }

    fn id(&self) -> &str {
        &self.id
    }
}

/// Reader for LatestValue: single shared slot, overwritten on each publish.
struct LatestValueReader {
    slot: Arc<Mutex<Option<Box<dyn Any + Send>>>>,
}

impl ErasedReader for LatestValueReader {
    fn read_erased(&mut self) -> Option<Box<dyn Any + Send>> {
        self.slot.lock().unwrap().take()
    }

    fn read_all_erased(&mut self) -> Vec<Box<dyn Any + Send>> {
        self.slot.lock().unwrap().take().into_iter().collect()
    }
}

/// Reader for Queued: messages delivered in order via channel.
struct QueuedReader {
    rx: mpsc::UnboundedReceiver<Box<dyn Any + Send>>,
}

impl ErasedReader for QueuedReader {
    fn read_erased(&mut self) -> Option<Box<dyn Any + Send>> {
        self.rx.try_recv().ok()
    }

    fn read_all_erased(&mut self) -> Vec<Box<dyn Any + Send>> {
        let mut results = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            results.push(msg);
        }
        results
    }
}
