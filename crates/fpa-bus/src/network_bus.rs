//! Network bus stub implementation.
//!
//! This is a stub that proves the Bus trait abstraction works across different
//! transport modes. Internally it uses the same clone-based delivery as
//! InProcessBus. A real implementation would register per-type codecs for
//! serialization and send messages over TCP/gRPC.
//!
//! The codec registration pattern (register_codec<M>) allows network
//! transport without adding serde bounds to the base Message trait.

use crate::bus::{Bus, CloneableMessage, ErasedReader, Transport};
use fpa_contract::message::DeliverySemantic;
use std::any::{Any, TypeId};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};

/// Channel state for a single message type.
struct ChannelState {
    /// Active subscribers: Weak refs enable automatic cleanup when readers drop.
    subscribers: Vec<Weak<Mutex<SubscriberState>>>,
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

    fn ensure_channel(
        channels: &mut HashMap<TypeId, ChannelState>,
        type_id: TypeId,
        _semantic: DeliverySemantic,
    ) {
        channels.entry(type_id).or_insert_with(|| ChannelState {
            subscribers: Vec::new(),
        });
    }
}

impl Bus for NetworkBus {
    fn publish_erased(
        &self,
        type_id: TypeId,
        semantic: DeliverySemantic,
        msg: Box<dyn CloneableMessage>,
    ) {
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel(&mut channels, type_id, semantic);

        let channel = channels.get_mut(&type_id).unwrap();

        // Prune dead subscribers
        channel.subscribers.retain(|w| w.strong_count() > 0);

        for weak_sub in &channel.subscribers {
            if let Some(sub) = weak_sub.upgrade() {
                let mut sub_state = sub.lock().unwrap();
                let cloned: Box<dyn Any + Send> = msg.clone_box().into_any();
                match sub_state.semantic {
                    DeliverySemantic::LatestValue => {
                        sub_state.latest = Some(cloned);
                    }
                    DeliverySemantic::Queued => {
                        sub_state.queue.push_back(cloned);
                    }
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
        Self::ensure_channel(&mut channels, type_id, semantic);

        let channel = channels.get_mut(&type_id).unwrap();

        let sub_state = Arc::new(Mutex::new(SubscriberState {
            latest: None,
            queue: VecDeque::new(),
            semantic,
        }));

        channel.subscribers.push(Arc::downgrade(&sub_state));

        Box::new(NetworkReader { state: sub_state })
    }

    fn transport(&self) -> Transport {
        Transport::Network
    }

    fn id(&self) -> &str {
        &self.id
    }
}

struct NetworkReader {
    state: Arc<Mutex<SubscriberState>>,
}

impl ErasedReader for NetworkReader {
    fn read_erased(&mut self) -> Option<Box<dyn Any + Send>> {
        let mut state = self.state.lock().unwrap();
        match state.semantic {
            DeliverySemantic::LatestValue => state.latest.take(),
            DeliverySemantic::Queued => state.queue.pop_front(),
        }
    }

    fn read_all_erased(&mut self) -> Vec<Box<dyn Any + Send>> {
        let mut state = self.state.lock().unwrap();
        match state.semantic {
            DeliverySemantic::LatestValue => state.latest.take().into_iter().collect(),
            DeliverySemantic::Queued => state.queue.drain(..).collect(),
        }
    }
}
