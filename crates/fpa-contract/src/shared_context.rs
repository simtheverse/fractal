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

/// Request to dump the compositor's current state (FPA-023).
///
/// A partition publishes this on the bus to request a state dump.
/// The compositor drains these during Phase 1 and produces a dump result.
#[derive(Debug, Clone)]
pub struct DumpRequest;

impl Message for DumpRequest {
    const NAME: &'static str = "DumpRequest";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::Queued;
}

/// Request to load a state fragment into the compositor (FPA-023).
///
/// A partition publishes this on the bus to request a state load.
/// The compositor drains these during Phase 1 and applies the fragment.
#[derive(Debug, Clone)]
pub struct LoadRequest {
    /// The TOML composition fragment to load.
    pub fragment: toml::Value,
}

impl Message for LoadRequest {
    const NAME: &'static str = "LoadRequest";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::Queued;
}
