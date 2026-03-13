//! Async bus implementation using tokio channels.
//!
//! Uses `tokio::sync::mpsc::unbounded_channel` for both delivery semantics.
//! For LatestValue, the reader drains the channel and returns only the last value.
//! For Queued, messages are delivered in order.
//!
//! The external API remains synchronous (FPA-004); internally, the tokio
//! unbounded channel's `send()` and `try_recv()` are both non-async.

use crate::bus::{Bus, BusReader, Transport};
use fpa_contract::message::{DeliverySemantic, Message};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Holds the per-message-type sender list.
struct ChannelState {
    /// Type-erased list of senders. Concrete type:
    /// `Arc<Mutex<Vec<mpsc::UnboundedSender<M>>>>`
    senders: Box<dyn Any + Send>,
    /// The delivery semantic for this channel.
    semantic: DeliverySemantic,
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

    fn ensure_channel<M: Message>(channels: &mut HashMap<TypeId, ChannelState>) {
        let type_id = TypeId::of::<M>();
        channels.entry(type_id).or_insert_with(|| {
            let senders: Arc<Mutex<Vec<mpsc::UnboundedSender<M>>>> =
                Arc::new(Mutex::new(Vec::new()));
            ChannelState {
                senders: Box::new(senders),
                semantic: M::DELIVERY,
            }
        });
    }
}

impl Bus for AsyncBus {
    fn publish<M: Message>(&self, msg: M) {
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel::<M>(&mut channels);

        let type_id = TypeId::of::<M>();
        let channel = channels.get_mut(&type_id).unwrap();

        let senders = channel
            .senders
            .downcast_ref::<Arc<Mutex<Vec<mpsc::UnboundedSender<M>>>>>()
            .expect("type mismatch in channel senders");
        let senders = senders.lock().unwrap();
        for tx in senders.iter() {
            let _ = tx.send(msg.clone());
        }
    }

    fn subscribe<M: Message>(&self) -> Box<dyn BusReader<M>> {
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel::<M>(&mut channels);

        let type_id = TypeId::of::<M>();
        let channel = channels.get_mut(&type_id).unwrap();

        let senders = channel
            .senders
            .downcast_ref::<Arc<Mutex<Vec<mpsc::UnboundedSender<M>>>>>()
            .expect("type mismatch in channel senders");

        let (tx, rx) = mpsc::unbounded_channel();
        senders.lock().unwrap().push(tx);

        Box::new(AsyncReader {
            rx,
            semantic: channel.semantic,
        })
    }

    fn transport(&self) -> Transport {
        Transport::Async
    }

    fn id(&self) -> &str {
        &self.id
    }
}

/// Reader for both delivery semantics using tokio::sync::mpsc.
struct AsyncReader<M: Message> {
    rx: mpsc::UnboundedReceiver<M>,
    semantic: DeliverySemantic,
}

impl<M: Message> BusReader<M> for AsyncReader<M> {
    fn read(&mut self) -> Option<M> {
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

    fn read_all(&mut self) -> Vec<M> {
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
