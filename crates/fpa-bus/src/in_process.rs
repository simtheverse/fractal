//! In-process bus implementation using channels.

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
    /// For LatestValue: the most recent value.
    latest: Option<Box<dyn Any + Send>>,
    /// For Queued: pending messages.
    queue: VecDeque<Box<dyn Any + Send>>,
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

impl Bus for InProcessBus {
    fn publish_erased(
        &self,
        type_id: TypeId,
        semantic: DeliverySemantic,
        msg: Box<dyn CloneableMessage>,
    ) {
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel(&mut channels, type_id, semantic);

        let channel = channels.get_mut(&type_id).unwrap();

        // Prune dead subscribers (Weak refs whose readers have been dropped)
        channel.subscribers.retain(|w| w.strong_count() > 0);

        // Deliver to all live subscribers
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

        Box::new(InProcessReader { state: sub_state })
    }

    fn transport(&self) -> Transport {
        Transport::InProcess
    }

    fn id(&self) -> &str {
        &self.id
    }
}

struct InProcessReader {
    state: Arc<Mutex<SubscriberState>>,
}

impl ErasedReader for InProcessReader {
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
