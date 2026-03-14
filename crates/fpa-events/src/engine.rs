//! Event engine: snapshot-based evaluation with no cascading side effects.

use std::collections::HashMap;

use crate::event::{EventAction, EventDefinition, EventTrigger};

/// Evaluates event definitions against a state snapshot.
///
/// CRITICAL: snapshot semantics — all conditions are evaluated against the
/// provided state. Action side effects are NOT visible to other event
/// conditions in the same evaluation pass.
pub struct EventEngine {
    events: Vec<EventDefinition>,
}

impl EventEngine {
    /// Create a new engine with the given event definitions.
    pub fn new(events: Vec<EventDefinition>) -> Self {
        Self { events }
    }

    /// Evaluate all armed events against the current state snapshot.
    ///
    /// Returns references to triggered actions in config (insertion) order.
    /// No cascading: the snapshot is immutable for the entire pass.
    pub fn evaluate(
        &self,
        current_time: f64,
        signals: &HashMap<String, f64>,
    ) -> Vec<&EventAction> {
        self.events
            .iter()
            .filter(|ev| ev.armed)
            .filter(|ev| match &ev.trigger {
                EventTrigger::Time { at } => current_time >= *at,
                EventTrigger::Condition { signal, predicate } => {
                    signals.get(signal).map_or(false, |v| predicate.evaluate(*v))
                }
            })
            .map(|ev| &ev.action)
            .collect()
    }

    /// Arm an event by its id so it becomes eligible to fire.
    pub fn arm(&mut self, event_id: &str) {
        for ev in &mut self.events {
            if ev.id == event_id {
                ev.armed = true;
            }
        }
    }

    /// Disarm an event by its id so it will not fire.
    pub fn disarm(&mut self, event_id: &str) {
        for ev in &mut self.events {
            if ev.id == event_id {
                ev.armed = false;
            }
        }
    }
}
