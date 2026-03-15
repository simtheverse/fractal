//! Bus abstraction and transport modes for inter-partition communication.

pub mod async_bus;
pub mod bus;
pub mod in_process;
pub mod network_bus;
pub mod network_message;

pub use async_bus::AsyncBus;
pub use bus::{Bus, BusExt, BusReader, CloneableMessage, ErasedReader, Transport, TypedReader};
pub use in_process::InProcessBus;
pub use network_bus::NetworkBus;
pub use network_message::{MessageCodec, NetworkMessage};
#[cfg(feature = "json-codec")]
pub use network_message::JsonCodec;
