//! Conversion from configuration types to runtime event definitions.

use crate::fragment::{EventConfig, TriggerConfig};
use fpa_events::{EventAction, EventDefinition, EventTrigger, Predicate};

/// Parse a predicate string and value into a [`Predicate`] for a given signal.
///
/// Supported operators: `">"` / `"greater_than"`, `"<"` / `"less_than"`,
/// `"=="` / `"equal"`.
fn parse_predicate(signal: &str, predicate: &str, value: f64) -> Result<Predicate, String> {
    match predicate {
        ">" | "greater_than" => Ok(Predicate::GreaterThan { signal: signal.to_string(), threshold: value }),
        "<" | "less_than" => Ok(Predicate::LessThan { signal: signal.to_string(), threshold: value }),
        "==" | "equal" => Ok(Predicate::Equal { signal: signal.to_string(), threshold: value }),
        other => Err(format!("unknown predicate operator: '{}'", other)),
    }
}

impl TryFrom<&EventConfig> for EventDefinition {
    type Error = String;

    fn try_from(config: &EventConfig) -> Result<Self, Self::Error> {
        let trigger = match &config.trigger {
            TriggerConfig::Time { at } => EventTrigger::Time { at: *at },
            TriggerConfig::Condition {
                signal,
                predicate,
                value,
            } => EventTrigger::Condition {
                predicate: parse_predicate(signal, predicate, *value)?,
            },
        };

        let action = EventAction {
            action_id: config.action.clone(),
            scope: config.scope.clone().unwrap_or_default(),
            parameters: config.parameters.clone(),
        };

        Ok(EventDefinition {
            id: config.id.clone(),
            trigger,
            action,
            armed: true,
        })
    }
}
