//! SharedContext: aggregated partition state published by the compositor each tick.
//!
//! SharedContext is a framework-level message type. It lives in the contract
//! crate (not the compositor) so that partitions can subscribe to it without
//! depending on the compositor crate — consistent with FPA-003 and FPA-005.

use crate::message::{DeliverySemantic, Message};
use crate::state_machine::ExecutionState;

/// Aggregated partition state published on the bus each tick.
///
/// The compositor collects `contribute_state()` results from all partitions
/// and publishes them as a single `SharedContext` message on the layer bus.
/// Partitions subscribe to this type to observe their peers' state and the
/// compositor's execution state (FPA-009).
#[derive(Debug, Clone)]
pub struct SharedContext {
    /// Aggregated partition states keyed by partition ID.
    pub state: toml::Value,
    /// The tick number when this context was produced.
    pub tick: u64,
    /// The compositor's execution state at the time of publication (FPA-009).
    pub execution_state: ExecutionState,
}

impl Message for SharedContext {
    const NAME: &'static str = "SharedContext";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}
