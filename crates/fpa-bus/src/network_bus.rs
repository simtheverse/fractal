//! Network bus with codec-based serialization.
//!
//! Messages are serialized on publish and deserialized on read, proving
//! real serialization round-trips through the bus. Each message type must
//! have a codec registered before use — this is the pattern that domain
//! applications follow to register their own message types.

use crate::bus::{Bus, CloneableMessage, ErasedReader, Transport};
use crate::network_message::{JsonCodec, MessageCodec, NetworkMessage};
use fpa_contract::message::DeliverySemantic;
use fpa_contract::{DumpRequest, LoadRequest, SharedContext, TransitionRequest};
use std::any::TypeId;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};

/// Channel state for a single message type.
struct ChannelState {
    /// Active subscribers: Weak refs enable automatic cleanup when readers drop.
    subscribers: Vec<Weak<Mutex<SubscriberState>>>,
}

struct SubscriberState {
    /// Serialized bytes for LatestValue semantic.
    latest: Option<Vec<u8>>,
    /// Serialized bytes queue for Queued semantic.
    queue: VecDeque<Vec<u8>>,
    semantic: DeliverySemantic,
}

/// Network bus with real serialization via registered codecs.
///
/// Messages are serialized to `Vec<u8>` on publish and deserialized on read.
/// Each message type requires a codec registered via `register_codec()`.
/// Use `with_framework_codecs()` to pre-register all framework message types.
pub struct NetworkBus {
    id: String,
    channels: Arc<Mutex<HashMap<TypeId, ChannelState>>>,
    codecs: Arc<Mutex<HashMap<TypeId, Arc<dyn MessageCodec>>>>,
}

impl NetworkBus {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            channels: Arc::new(Mutex::new(HashMap::new())),
            codecs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a codec for a `NetworkMessage` type.
    ///
    /// Must be called before publishing or subscribing to this message type.
    /// Uses `JsonCodec` as the default serialization format.
    pub fn register_codec<M: NetworkMessage + Sync>(&self) {
        let mut codecs = self.codecs.lock().unwrap();
        codecs.insert(TypeId::of::<M>(), Arc::new(JsonCodec::<M>::new()));
    }

    /// Builder: pre-register codecs for all framework message types.
    ///
    /// Registers SharedContext, TransitionRequest, DumpRequest, LoadRequest,
    /// StateContribution, and ExecutionState. Domain applications should call
    /// this and then register their own message codecs.
    pub fn with_framework_codecs(self) -> Self {
        self.register_codec::<SharedContext>();
        self.register_codec::<TransitionRequest>();
        self.register_codec::<DumpRequest>();
        self.register_codec::<LoadRequest>();
        self
    }

    fn ensure_channel(channels: &mut HashMap<TypeId, ChannelState>, type_id: TypeId) {
        channels.entry(type_id).or_insert_with(|| ChannelState {
            subscribers: Vec::new(),
        });
    }

    fn get_codec(&self, type_id: TypeId) -> Option<Arc<dyn MessageCodec>> {
        let codecs = self.codecs.lock().unwrap();
        codecs.get(&type_id).cloned()
    }
}

impl Bus for NetworkBus {
    fn publish_erased(
        &self,
        type_id: TypeId,
        _semantic: DeliverySemantic,
        msg: Box<dyn CloneableMessage>,
    ) {
        let codec = self.get_codec(type_id).unwrap_or_else(|| {
            panic!(
                "NetworkBus: no codec registered for TypeId {:?}. \
                 Call register_codec::<M>() before publishing.",
                type_id
            )
        });

        // Serialize the message to bytes
        let any_ref: &dyn std::any::Any = &*msg;
        let bytes = codec.serialize(any_ref);

        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel(&mut channels, type_id);

        let channel = channels.get_mut(&type_id).unwrap();

        // Prune dead subscribers
        channel.subscribers.retain(|w| w.strong_count() > 0);

        for weak_sub in &channel.subscribers {
            if let Some(sub) = weak_sub.upgrade() {
                let mut sub_state = sub.lock().unwrap();
                match sub_state.semantic {
                    DeliverySemantic::LatestValue => {
                        sub_state.latest = Some(bytes.clone());
                    }
                    DeliverySemantic::Queued => {
                        sub_state.queue.push_back(bytes.clone());
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
        let codec = self.get_codec(type_id).unwrap_or_else(|| {
            panic!(
                "NetworkBus: no codec registered for TypeId {:?}. \
                 Call register_codec::<M>() before subscribing.",
                type_id
            )
        });

        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel(&mut channels, type_id);

        let channel = channels.get_mut(&type_id).unwrap();

        let sub_state = Arc::new(Mutex::new(SubscriberState {
            latest: None,
            queue: VecDeque::new(),
            semantic,
        }));

        channel.subscribers.push(Arc::downgrade(&sub_state));

        Box::new(NetworkReader {
            state: sub_state,
            codec,
        })
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
    codec: Arc<dyn MessageCodec>,
}

impl ErasedReader for NetworkReader {
    fn read_erased(&mut self) -> Option<Box<dyn std::any::Any + Send>> {
        let mut state = self.state.lock().unwrap();
        let bytes = match state.semantic {
            DeliverySemantic::LatestValue => state.latest.take(),
            DeliverySemantic::Queued => state.queue.pop_front(),
        }?;
        Some(self.codec.deserialize(&bytes))
    }

    fn read_all_erased(&mut self) -> Vec<Box<dyn std::any::Any + Send>> {
        let mut state = self.state.lock().unwrap();
        match state.semantic {
            DeliverySemantic::LatestValue => state
                .latest
                .take()
                .into_iter()
                .map(|bytes| self.codec.deserialize(&bytes))
                .collect(),
            DeliverySemantic::Queued => state
                .queue
                .drain(..)
                .map(|bytes| self.codec.deserialize(&bytes))
                .collect(),
        }
    }
}
