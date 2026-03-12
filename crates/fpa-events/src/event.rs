//! Core event types: triggers, predicates, actions, and event definitions.

use std::collections::HashMap;

/// Defines when an event fires.
#[derive(Debug, Clone)]
pub enum EventTrigger {
    /// Fire at (or after) the specified simulation time.
    Time { at: f64 },
    /// Fire when a named signal satisfies a predicate.
    Condition { signal: String, predicate: Predicate },
}

/// Composable predicate for condition-triggered events.
#[derive(Debug, Clone)]
pub enum Predicate {
    LessThan(f64),
    GreaterThan(f64),
    Equal(f64),
    And(Box<Predicate>, Box<Predicate>),
}

impl Predicate {
    /// Evaluate this predicate against a concrete signal value.
    pub fn evaluate(&self, value: f64) -> bool {
        match self {
            Predicate::LessThan(threshold) => value < *threshold,
            Predicate::GreaterThan(threshold) => value > *threshold,
            Predicate::Equal(threshold) => (value - *threshold).abs() < f64::EPSILON,
            Predicate::And(a, b) => a.evaluate(value) && b.evaluate(value),
        }
    }
}

/// The action to perform when an event fires.
#[derive(Debug, Clone)]
pub struct EventAction {
    /// Unique identifier for this action type.
    pub action_id: String,
    /// The contract crate that declares this action.
    pub scope: String,
    /// Parameters passed to the action.
    pub parameters: HashMap<String, toml::Value>,
}

/// A complete event definition: trigger + action + armed state.
#[derive(Debug, Clone)]
pub struct EventDefinition {
    /// Unique identifier for this event.
    pub id: String,
    /// When the event fires.
    pub trigger: EventTrigger,
    /// What the event does.
    pub action: EventAction,
    /// Whether the event is armed (eligible to fire).
    pub armed: bool,
}
