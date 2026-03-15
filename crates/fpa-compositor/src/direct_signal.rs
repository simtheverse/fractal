//! Direct signals: bypass relay chain within contract crate scope (FPA-013).
//!
//! A direct signal reaches the declaring crate's orchestrator without passing
//! through the relay chain. Signals are scoped to the declaring crate and
//! cannot propagate beyond the system boundary. Every emission is logged with
//! identity and depth.

use std::collections::HashSet;

/// A direct signal that bypasses the relay chain.
#[derive(Debug, Clone)]
pub struct DirectSignal {
    /// Signal identifier (unique within declaring crate).
    pub signal_id: String,
    /// Human-readable reason for emission.
    pub reason: String,
    /// Identity of the emitter (partition ID or compositor ID).
    pub emitter_identity: String,
    /// Layer depth at which the signal was emitted.
    pub layer_depth: u32,
}

impl DirectSignal {
    /// Create a new direct signal.
    pub fn new(
        signal_id: impl Into<String>,
        reason: impl Into<String>,
        emitter_identity: impl Into<String>,
        layer_depth: u32,
    ) -> Self {
        Self {
            signal_id: signal_id.into(),
            reason: reason.into(),
            emitter_identity: emitter_identity.into(),
            layer_depth,
        }
    }
}

/// Registry of allowed direct signal IDs for a crate scope.
///
/// Only signals whose IDs are registered can be emitted. This enforces
/// the "small set of signals registered in contract crate" constraint.
#[derive(Debug, Clone, Default)]
pub struct DirectSignalRegistry {
    /// Allowed signal identifiers.
    allowed: HashSet<String>,
}

impl DirectSignalRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            allowed: HashSet::new(),
        }
    }

    /// Register a signal ID as allowed.
    pub fn register(&mut self, signal_id: impl Into<String>) {
        self.allowed.insert(signal_id.into());
    }

    /// Check whether a signal ID is registered.
    pub fn is_registered(&self, signal_id: &str) -> bool {
        self.allowed.contains(signal_id)
    }

    /// Return the number of registered signal IDs.
    pub fn len(&self) -> usize {
        self.allowed.len()
    }

    /// Return whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty()
    }
}
