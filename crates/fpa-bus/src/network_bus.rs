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
use crate::network_message::MessageCodec;
use fpa_contract::message::DeliverySemantic;
use std::any::{Any, TypeId};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};

/// A delivered item: either serialized bytes (when codec is available)
/// or a cloned object (fallback when no codec is registered).
/// Serialized bytes are shared via Arc to avoid per-subscriber copies.
enum DeliveredItem {
    Serialized(Arc<[u8]>),
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

/// Shared codec registry, referenced by both the bus and all readers.
type CodecRegistry = Arc<Mutex<HashMap<TypeId, Arc<dyn MessageCodec>>>>;

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
    codecs: CodecRegistry,
}

impl NetworkBus {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            channels: Arc::new(Mutex::new(HashMap::new())),
            codecs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a custom codec for a message type identified by TypeId.
    ///
    /// Once registered, messages of this type are serialized to bytes on
    /// publish and deserialized on read. Without registration, the bus
    /// falls back to clone-based delivery for this type.
    ///
    /// Codecs can be registered at any time — both existing and future
    /// subscribers will use the codec for deserialization.
    ///
    /// This method is always available regardless of feature flags.
    /// Use it to register codecs for custom serialization formats.
    pub fn register_custom_codec(&self, type_id: TypeId, codec: Arc<dyn MessageCodec>) {
        let mut codecs = self.codecs.lock().unwrap();
        codecs.insert(type_id, codec);
    }

    /// Register a JSON codec for a `NetworkMessage` type.
    ///
    /// Convenience method that creates a `JsonCodec<M>` and registers it.
    /// Requires the `json-codec` feature.
    #[cfg(feature = "json-codec")]
    pub fn register_codec<M: crate::network_message::NetworkMessage>(&self) {
        self.register_custom_codec(
            TypeId::of::<M>(),
            Arc::new(crate::network_message::JsonCodec::<M>::new()),
        );
    }

    /// Builder: pre-register JSON codecs for all framework message types.
    ///
    /// Registers SharedContext, TransitionRequest, DumpRequest, and
    /// LoadRequest. Domain applications should call this and then
    /// register their own message codecs.
    ///
    /// Requires the `json-codec` feature.
    #[cfg(feature = "json-codec")]
    pub fn with_framework_codecs(self) -> Self {
        use fpa_contract::{DumpRequest, LoadRequest, SharedContext, TransitionRequest};
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
        // Serialize before acquiring the channels lock to minimize lock scope.
        // Bytes are wrapped in Arc so all subscribers share the same buffer.
        let codec = self.get_codec(type_id);
        let serialized: Option<Arc<[u8]>> = codec.as_ref().map(|c| {
            let any_ref: &dyn Any = &*msg;
            Arc::from(c.serialize(any_ref))
        });

        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel(&mut channels, type_id);

        let channel = channels.get_mut(&type_id).unwrap();

        // Prune dead subscribers
        channel.subscribers.retain(|w| w.strong_count() > 0);

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
        let mut channels = self.channels.lock().unwrap();
        Self::ensure_channel(&mut channels, type_id);

        let channel = channels.get_mut(&type_id).unwrap();

        let sub_state = Arc::new(Mutex::new(SubscriberState {
            latest: None,
            queue: VecDeque::new(),
            semantic,
        }));

        channel.subscribers.push(Arc::downgrade(&sub_state));

        // Share the codec registry with the reader so it can look up
        // codecs dynamically at read time. This means codecs registered
        // after subscription are still available for deserialization.
        Box::new(NetworkReader {
            state: sub_state,
            codecs: Arc::clone(&self.codecs),
            type_id,
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
    codecs: CodecRegistry,
    type_id: TypeId,
}

impl NetworkReader {
    fn get_codec(&self) -> Option<Arc<dyn MessageCodec>> {
        let codecs = self.codecs.lock().unwrap();
        codecs.get(&self.type_id).cloned()
    }

    fn resolve_item(&self, item: DeliveredItem) -> Box<dyn Any + Send> {
        match item {
            DeliveredItem::Serialized(ref bytes) => {
                self.get_codec()
                    .expect("received serialized item but no codec is registered")
                    .deserialize(bytes)
            }
            DeliveredItem::Cloned(any) => any,
        }
    }
}

impl ErasedReader for NetworkReader {
    fn read_erased(&mut self) -> Option<Box<dyn Any + Send>> {
        // Take the item under the lock, then deserialize outside it.
        let item = {
            let mut state = self.state.lock().unwrap();
            match state.semantic {
                DeliverySemantic::LatestValue => state.latest.take(),
                DeliverySemantic::Queued => state.queue.pop_front(),
            }
        }?;
        Some(self.resolve_item(item))
    }

    fn read_all_erased(&mut self) -> Vec<Box<dyn Any + Send>> {
        // Drain all items under the lock, then deserialize outside it.
        let items: Vec<DeliveredItem> = {
            let mut state = self.state.lock().unwrap();
            match state.semantic {
                DeliverySemantic::LatestValue => state.latest.take().into_iter().collect(),
                DeliverySemantic::Queued => state.queue.drain(..).collect(),
            }
        };
        items
            .into_iter()
            .map(|item| self.resolve_item(item))
            .collect()
    }
}
