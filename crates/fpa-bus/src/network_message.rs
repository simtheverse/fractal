//! Network message subtrait and codec infrastructure.
//!
//! Messages that cross network boundaries must be serializable. The
//! `NetworkMessage` subtrait adds `Serialize + DeserializeOwned` bounds
//! without affecting the base `Message` trait. Codecs handle the actual
//! serialization format — `JsonCodec` is provided with the `json-codec`
//! feature; domain applications can implement `MessageCodec` for custom
//! formats regardless of feature flags.

use std::any::Any;

use fpa_contract::Message;
use serde::{de::DeserializeOwned, Serialize};

/// A message that can be serialized for network transport.
///
/// This is a subtrait of `Message` that adds serde bounds. The base `Message`
/// trait stays clean — only messages that cross network boundaries need this.
pub trait NetworkMessage: Message + Serialize + DeserializeOwned {}

/// Blanket impl: any `Message` that is also `Serialize + DeserializeOwned`
/// automatically implements `NetworkMessage`.
impl<T: Message + Serialize + DeserializeOwned> NetworkMessage for T {}

/// Object-safe codec for serializing and deserializing messages.
///
/// Each `NetworkMessage` type gets a codec registered with the `NetworkBus`.
/// The codec handles the conversion between typed messages and byte buffers.
/// Implement this trait for custom serialization formats (bincode, protobuf, etc.).
pub trait MessageCodec: Send + Sync {
    /// Serialize a type-erased message to bytes.
    ///
    /// The `Any` reference is guaranteed to be the correct concrete type
    /// for this codec (enforced by the registration pattern).
    fn serialize(&self, msg: &dyn Any) -> Vec<u8>;

    /// Deserialize bytes back into a type-erased message.
    fn deserialize(&self, bytes: &[u8]) -> Box<dyn Any + Send>;
}

/// JSON codec for a specific `NetworkMessage` type.
///
/// Uses `serde_json` for serialization. Domain applications can implement
/// `MessageCodec` directly for custom formats without this feature.
#[cfg(feature = "json-codec")]
pub struct JsonCodec<M> {
    // PhantomData<fn() -> M> makes JsonCodec<M> Send+Sync regardless of M,
    // since it doesn't actually own or reference an M. This avoids requiring
    // M: Sync on the codec when M is already Send (as Message requires).
    _marker: std::marker::PhantomData<fn() -> M>,
}

#[cfg(feature = "json-codec")]
impl<M> JsonCodec<M> {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "json-codec")]
impl<M> Default for JsonCodec<M> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "json-codec")]
impl<M: NetworkMessage> MessageCodec for JsonCodec<M> {
    fn serialize(&self, msg: &dyn Any) -> Vec<u8> {
        let typed = msg
            .downcast_ref::<M>()
            .expect("JsonCodec::serialize called with wrong type");
        serde_json::to_vec(typed).expect("failed to serialize message to JSON")
    }

    fn deserialize(&self, bytes: &[u8]) -> Box<dyn Any + Send> {
        let typed: M =
            serde_json::from_slice(bytes).expect("failed to deserialize message from JSON");
        Box::new(typed)
    }
}
