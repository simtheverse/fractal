//! Network bus stub implementation.
//!
//! This is a stub that proves the Bus trait abstraction works across different
//! transport modes. Internally it uses the same clone-based delivery as
//! InProcessBus. A real implementation would serialize messages to bytes and
//! send them over TCP/gRPC, requiring `Serialize + Deserialize` bounds on
//! messages.
//!
//! TODO: Replace clone-based delivery with actual network serialization
//! (e.g., serde + toml/bincode over TCP) once Message gains Serialize bounds.

use crate::bus::{Bus, BusReader, Transport};
use fpa_contract::message::{DeliverySemantic, Message};
use std::any::{Any, TypeId};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// Channel state for a single message type.
struct ChannelState {
    latest: Option<Box<dyn Any + Send>>,
    queue: VecDeque<Box<dyn Any + Send>>,
    semantic: DeliverySemantic,
    subscribers: Vec<Arc<Mutex<SubscriberState>>>,
}

struct SubscriberState {
    latest: Option<Box<dyn Any + Send>>,
    queue: VecDeque<Box<dyn Any + Send>>,
    semantic: DeliverySemantic,
}

/// Network bus stub. Reports `Transport::Network` but uses in-process channels
/// internally. See module docs for rationale.
pub struct NetworkBus {
    id: String,
    channels: Arc<Mutex<HashMap<TypeId, ChannelState>>>,
}

impl NetworkBus {
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
            queue: VecDeque::new(),
            semantic: M::DELIVERY,
            subscribers: Vec::new(),
        });
    }
}

impl Bus for NetworkBus {
    fn publish<M: Message>(&self, msg: M) {
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel::<M>(&mut channels);

        let type_id = TypeId::of::<M>();
        let channel = channels.get_mut(&type_id).unwrap();

        for sub in &channel.subscribers {
            let mut sub_state = sub.lock().unwrap();
            match sub_state.semantic {
                DeliverySemantic::LatestValue => {
                    sub_state.latest = Some(Box::new(msg.clone()));
                }
                DeliverySemantic::Queued => {
                    sub_state.queue.push_back(Box::new(msg.clone()));
                }
            }
        }

        match channel.semantic {
            DeliverySemantic::LatestValue => {
                channel.latest = Some(Box::new(msg));
            }
            DeliverySemantic::Queued => {
                channel.queue.push_back(Box::new(msg));
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
            queue: VecDeque::new(),
            semantic: M::DELIVERY,
        }));

        channel.subscribers.push(sub_state.clone());

        Box::new(NetworkReader::<M> {
            state: sub_state,
            _marker: std::marker::PhantomData,
        })
    }

    fn transport(&self) -> Transport {
        Transport::Network
    }

    fn id(&self) -> &str {
        &self.id
    }
}

struct NetworkReader<M> {
    state: Arc<Mutex<SubscriberState>>,
    _marker: std::marker::PhantomData<M>,
}

impl<M: Message> BusReader<M> for NetworkReader<M> {
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
                    state.queue.pop_front().and_then(|v| v.downcast::<M>().ok()).map(|v| *v)
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
