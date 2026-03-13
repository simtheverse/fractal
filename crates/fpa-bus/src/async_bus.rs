//! Async bus implementation using tokio channels.
//!
//! Uses `tokio::sync::mpsc::unbounded_channel` for message delivery.
//! For LatestValue, the reader drains the channel and returns only the last value.
//! For Queued, messages are delivered in order.
//!
//! The external API remains synchronous (FPA-004); internally, the tokio
//! unbounded channel's `send()` and `try_recv()` are both non-async.

use crate::bus::{Bus, CloneableMessage, ErasedReader, Transport};
use fpa_contract::message::DeliverySemantic;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Holds the per-message-type sender list (type-erased).
struct ChannelState {
    /// Senders for all subscribers. Dead senders are pruned during publish.
    senders: Vec<mpsc::UnboundedSender<Box<dyn Any + Send>>>,
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
        _semantic: DeliverySemantic,
    ) {
        channels.entry(type_id).or_insert_with(|| ChannelState {
            senders: Vec::new(),
        });
    }
}

impl Bus for AsyncBus {
    fn publish_erased(
        &self,
        type_id: TypeId,
        semantic: DeliverySemantic,
        msg: Box<dyn CloneableMessage>,
    ) {
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel(&mut channels, type_id, semantic);

        let channel = channels.get_mut(&type_id).unwrap();

        // Prune closed senders (subscriber dropped their receiver)
        channel.senders.retain(|tx| !tx.is_closed());

        // Send a clone to each subscriber
        for tx in &channel.senders {
            let cloned: Box<dyn Any + Send> = msg.clone_box().into_any();
            let _ = tx.send(cloned);
        }
    }

    fn subscribe_erased(
        &self,
        type_id: TypeId,
        semantic: DeliverySemantic,
    ) -> Box<dyn ErasedReader> {
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel(&mut channels, type_id, semantic);

        let channel = channels.get_mut(&type_id).unwrap();

        let (tx, rx) = mpsc::unbounded_channel();
        channel.senders.push(tx);

        Box::new(AsyncReader { rx, semantic })
    }

    fn transport(&self) -> Transport {
        Transport::Async
    }

    fn id(&self) -> &str {
        &self.id
    }
}

/// Reader for both delivery semantics using tokio::sync::mpsc.
struct AsyncReader {
    rx: mpsc::UnboundedReceiver<Box<dyn Any + Send>>,
    semantic: DeliverySemantic,
}

impl ErasedReader for AsyncReader {
    fn read_erased(&mut self) -> Option<Box<dyn Any + Send>> {
        match self.semantic {
            DeliverySemantic::LatestValue => {
                // Drain channel, keep only the last value.
                let mut latest = None;
                while let Ok(msg) = self.rx.try_recv() {
                    latest = Some(msg);
                }
                latest
            }
            DeliverySemantic::Queued => self.rx.try_recv().ok(),
        }
    }

    fn read_all_erased(&mut self) -> Vec<Box<dyn Any + Send>> {
        match self.semantic {
            DeliverySemantic::LatestValue => {
                // Drain channel, return only the last value.
                let mut latest = None;
                while let Ok(msg) = self.rx.try_recv() {
                    latest = Some(msg);
                }
                latest.into_iter().collect()
            }
            DeliverySemantic::Queued => {
                let mut results = Vec::new();
                while let Ok(msg) = self.rx.try_recv() {
                    results.push(msg);
                }
                results
            }
        }
    }
}
