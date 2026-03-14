//! Conversion from configuration types to runtime event definitions.

use crate::fragment::{EventConfig, TriggerConfig};
use fpa_events::{EventAction, EventDefinition, EventTrigger, Predicate};

/// Parse a predicate string and value into a [`Predicate`].
///
/// Supported operators: `">"` / `"greater_than"`, `"<"` / `"less_than"`,
/// `"=="` / `"equal"`.
fn parse_predicate(predicate: &str, value: f64) -> Result<Predicate, String> {
    match predicate {
        ">" | "greater_than" => Ok(Predicate::GreaterThan(value)),
        "<" | "less_than" => Ok(Predicate::LessThan(value)),
        "==" | "equal" => Ok(Predicate::Equal(value)),
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
                signal: signal.clone(),
                predicate: parse_predicate(predicate, *value)?,
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
