//! Core event types: triggers, predicates, actions, and event definitions.

use std::collections::HashMap;

/// Defines when an event fires.
#[derive(Debug, Clone)]
pub enum EventTrigger {
    /// Fire at (or after) the specified simulation time.
    Time { at: f64 },
    /// Fire when a predicate over named signals is satisfied.
    Condition { predicate: Predicate },
}

/// Composable predicate for condition-triggered events.
///
/// Each leaf predicate references a named signal and a threshold. The `And`
/// combinator enables cross-signal compound conditions (e.g.,
/// `signal_a > 1.0 && signal_b < 500.0`) as required by FPA-026.
#[derive(Debug, Clone)]
pub enum Predicate {
    LessThan { signal: String, threshold: f64 },
    GreaterThan { signal: String, threshold: f64 },
    Equal { signal: String, threshold: f64 },
    And(Box<Predicate>, Box<Predicate>),
}

impl Predicate {
    /// Evaluate this predicate against a map of named signal values.
    pub fn evaluate(&self, signals: &HashMap<String, f64>) -> bool {
        match self {
            Predicate::LessThan { signal, threshold } => {
                signals.get(signal).map_or(false, |v| *v < *threshold)
            }
            Predicate::GreaterThan { signal, threshold } => {
                signals.get(signal).map_or(false, |v| *v > *threshold)
            }
            Predicate::Equal { signal, threshold } => {
                signals.get(signal).map_or(false, |v| *v == *threshold)
            }
            Predicate::And(a, b) => a.evaluate(signals) && b.evaluate(signals),
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
