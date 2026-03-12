//! In-process bus implementation using channels.

use crate::bus::{Bus, BusReader, Transport};
use fpa_contract::message::{DeliverySemantic, Message};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Channel state for a single message type.
struct ChannelState {
    /// For LatestValue: stores the most recent value.
    latest: Option<Box<dyn Any + Send>>,
    /// For Queued: stores all published values in order.
    queue: Vec<Box<dyn Any + Send>>,
    /// The delivery semantic for this channel.
    semantic: DeliverySemantic,
    /// Subscriber notification: each subscriber gets its own copy.
    subscribers: Vec<Arc<Mutex<SubscriberState>>>,
}

struct SubscriberState {
    /// For LatestValue: the most recent value.
    latest: Option<Box<dyn Any + Send>>,
    /// For Queued: pending messages.
    queue: Vec<Box<dyn Any + Send>>,
    /// The delivery semantic.
    semantic: DeliverySemantic,
}

/// In-process bus using channels. Supports both delivery semantics (FPA-007).
pub struct InProcessBus {
    id: String,
    channels: Arc<Mutex<HashMap<TypeId, ChannelState>>>,
}

impl InProcessBus {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            channels: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn ensure_channel<M: Message>(channels: &mut HashMap<TypeId, ChannelState>) {
        let type_id = TypeId::of::<M>();
        channels.entry(type_id).or_insert_with(|| ChannelState {
            latest: None,
            queue: Vec::new(),
            semantic: M::DELIVERY,
            subscribers: Vec::new(),
        });
    }
}

impl Bus for InProcessBus {
    fn publish<M: Message>(&self, msg: M) {
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel::<M>(&mut channels);

        let type_id = TypeId::of::<M>();
        let channel = channels.get_mut(&type_id).unwrap();

        // Deliver to all subscribers according to the delivery semantic
        for sub in &channel.subscribers {
            let mut sub_state = sub.lock().unwrap();
            match sub_state.semantic {
                DeliverySemantic::LatestValue => {
                    sub_state.latest = Some(Box::new(msg.clone()));
                }
                DeliverySemantic::Queued => {
                    sub_state.queue.push(Box::new(msg.clone()));
                }
            }
        }

        // Also store in channel state
        match channel.semantic {
            DeliverySemantic::LatestValue => {
                channel.latest = Some(Box::new(msg));
            }
            DeliverySemantic::Queued => {
                channel.queue.push(Box::new(msg));
            }
        }
    }

    fn subscribe<M: Message>(&self) -> Box<dyn BusReader<M>> {
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel::<M>(&mut channels);

        let type_id = TypeId::of::<M>();
        let channel = channels.get_mut(&type_id).unwrap();

        let sub_state = Arc::new(Mutex::new(SubscriberState {
            latest: None,
            queue: Vec::new(),
            semantic: M::DELIVERY,
        }));

        channel.subscribers.push(sub_state.clone());

        Box::new(InProcessReader::<M> {
            state: sub_state,
            _marker: std::marker::PhantomData,
        })
    }

    fn transport(&self) -> Transport {
        Transport::InProcess
    }

    fn id(&self) -> &str {
        &self.id
    }
}

struct InProcessReader<M> {
    state: Arc<Mutex<SubscriberState>>,
    _marker: std::marker::PhantomData<M>,
}

impl<M: Message> BusReader<M> for InProcessReader<M> {
    fn read(&mut self) -> Option<M> {
        let mut state = self.state.lock().unwrap();
        match state.semantic {
            DeliverySemantic::LatestValue => {
                state.latest.take().and_then(|v| v.downcast::<M>().ok()).map(|v| *v)
            }
            DeliverySemantic::Queued => {
                if state.queue.is_empty() {
                    None
                } else {
                    state.queue.remove(0).downcast::<M>().ok().map(|v| *v)
                }
            }
        }
    }

    fn read_all(&mut self) -> Vec<M> {
        let mut state = self.state.lock().unwrap();
        match state.semantic {
            DeliverySemantic::LatestValue => {
                state.latest.take()
                    .and_then(|v| v.downcast::<M>().ok())
                    .map(|v| vec![*v])
                    .unwrap_or_default()
            }
            DeliverySemantic::Queued => {
                state.queue.drain(..)
                    .filter_map(|v| v.downcast::<M>().ok().map(|v| *v))
                    .collect()
            }
        }
    }
}

// InProcessBus needs to be Send since Bus: Send
unsafe impl Send for InProcessBus {}
