//! Network bus with codec-based serialization.
//!
//! When a codec is registered for a message type, messages are serialized to
//! bytes on publish and deserialized on read — proving real serialization
//! round-trips. When no codec is registered, the bus falls back to clone-based
//! delivery (identical to InProcessBus), preserving Bus trait transparency.
//!
//! This design ensures that switching from InProcessBus to NetworkBus never
//! breaks existing code — codec registration is an opt-in that enables
//! serialization for types that need it.

use crate::bus::{Bus, CloneableMessage, ErasedReader, Transport};
use crate::network_message::{JsonCodec, MessageCodec, NetworkMessage};
use fpa_contract::message::DeliverySemantic;
use fpa_contract::{DumpRequest, LoadRequest, SharedContext, TransitionRequest};
use std::any::{Any, TypeId};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};

/// A delivered item: either serialized bytes (when codec is available)
/// or a cloned object (fallback when no codec is registered).
enum DeliveredItem {
    Serialized(Vec<u8>),
    Cloned(Box<dyn Any + Send>),
}

/// Channel state for a single message type.
struct ChannelState {
    /// Active subscribers: Weak refs enable automatic cleanup when readers drop.
    subscribers: Vec<Weak<Mutex<SubscriberState>>>,
}

struct SubscriberState {
    latest: Option<DeliveredItem>,
    queue: VecDeque<DeliveredItem>,
    semantic: DeliverySemantic,
}

/// Network bus with transparent codec-based serialization.
///
/// When a codec is registered for a message type (via `register_codec()`),
/// messages are serialized to `Vec<u8>` on publish and deserialized on read.
/// When no codec is registered, the bus falls back to clone-based delivery.
///
/// This preserves Bus trait transparency: switching from InProcessBus to
/// NetworkBus requires no code changes. Codec registration opts specific
/// message types into serialization.
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

    /// Register a JSON codec for a `NetworkMessage` type.
    ///
    /// Once registered, messages of this type are serialized to bytes on
    /// publish and deserialized on read. Without registration, the bus
    /// falls back to clone-based delivery for this type.
    pub fn register_codec<M: NetworkMessage + Sync>(&self) {
        let mut codecs = self.codecs.lock().unwrap();
        codecs.insert(TypeId::of::<M>(), Arc::new(JsonCodec::<M>::new()));
    }

    /// Builder: pre-register codecs for all framework message types.
    ///
    /// Registers SharedContext, TransitionRequest, DumpRequest, and
    /// LoadRequest. Domain applications should call this and then
    /// register their own message codecs.
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
        // If a codec is registered, serialize to bytes. Otherwise fall back
        // to clone-based delivery (same as InProcessBus).
        let codec = self.get_codec(type_id);

        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel(&mut channels, type_id);

        let channel = channels.get_mut(&type_id).unwrap();

        // Prune dead subscribers
        channel.subscribers.retain(|w| w.strong_count() > 0);

        // Serialize once if codec is available, then clone bytes to each subscriber.
        // Without a codec, clone the message object for each subscriber.
        let serialized = codec.as_ref().map(|c| {
            let any_ref: &dyn Any = &*msg;
            c.serialize(any_ref)
        });

        for weak_sub in &channel.subscribers {
            if let Some(sub) = weak_sub.upgrade() {
                let mut sub_state = sub.lock().unwrap();
                let item = match &serialized {
                    Some(bytes) => DeliveredItem::Serialized(bytes.clone()),
                    None => DeliveredItem::Cloned(msg.clone_box().into_any()),
                };
                match sub_state.semantic {
                    DeliverySemantic::LatestValue => {
                        sub_state.latest = Some(item);
                    }
                    DeliverySemantic::Queued => {
                        sub_state.queue.push_back(item);
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
        let codec = self.get_codec(type_id);

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
    codec: Option<Arc<dyn MessageCodec>>,
}

impl NetworkReader {
    fn resolve_item(&self, item: DeliveredItem) -> Box<dyn Any + Send> {
        match item {
            DeliveredItem::Serialized(bytes) => {
                self.codec
                    .as_ref()
                    .expect("received serialized item but reader has no codec")
                    .deserialize(&bytes)
            }
            DeliveredItem::Cloned(any) => any,
        }
    }
}

impl ErasedReader for NetworkReader {
    fn read_erased(&mut self) -> Option<Box<dyn Any + Send>> {
        let mut state = self.state.lock().unwrap();
        let item = match state.semantic {
            DeliverySemantic::LatestValue => state.latest.take(),
            DeliverySemantic::Queued => state.queue.pop_front(),
        }?;
        Some(self.resolve_item(item))
    }

    fn read_all_erased(&mut self) -> Vec<Box<dyn Any + Send>> {
        let mut state = self.state.lock().unwrap();
        match state.semantic {
            DeliverySemantic::LatestValue => state
                .latest
                .take()
                .into_iter()
                .map(|item| self.resolve_item(item))
                .collect(),
            DeliverySemantic::Queued => state
                .queue
                .drain(..)
                .map(|item| self.resolve_item(item))
                .collect(),
        }
    }
}
