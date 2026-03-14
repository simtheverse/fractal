//! Conversion from configuration types to runtime event definitions.

use crate::fragment::{EventConfig, TriggerConfig};
use fpa_events::{ActionRegistry, EventAction, EventDefinition, EventTrigger, Predicate};

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

/// Convert an [`EventConfig`] into an [`EventDefinition`] using the given scope.
fn event_definition_from_config(config: &EventConfig, scope: &str) -> Result<EventDefinition, String> {
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
        scope: scope.to_string(),
        parameters: config.parameters.clone(),
    };

    Ok(EventDefinition {
        id: config.id.clone(),
        trigger,
        action,
        armed: true,
    })
}

/// Convert an [`EventConfig`] to an [`EventDefinition`] with action registry
/// validation (FPA-029).
///
/// Performs the same structural conversion as `TryFrom<&EventConfig>`, then
/// validates that the action identifier is registered and usable at the
/// event's scope. `default_scope` is used when the config omits a scope —
/// callers should pass the scope of the context where the event is defined
/// (e.g., `"system"` for system-level events, `"system.physics"` for
/// partition-level events). Returns an error if the action is not declared
/// in a contract crate visible at that scope.
pub fn validated_event_definition(
    config: &EventConfig,
    registry: &ActionRegistry,
    default_scope: &str,
) -> Result<EventDefinition, String> {
    let scope = config.scope.as_deref().unwrap_or(default_scope);
    let def = event_definition_from_config(config, scope)?;
    registry.validate(&def.action.action_id, scope)?;
    Ok(def)
}

/// Structural conversion only — does NOT validate the action identifier
/// against an [`ActionRegistry`]. Production config loading should use
/// [`validated_event_definition`] instead, which enforces FPA-029 scoping.
/// This impl exists for tests and contexts where registry validation is
/// handled separately.
impl TryFrom<&EventConfig> for EventDefinition {
    type Error = String;

    fn try_from(config: &EventConfig) -> Result<Self, Self::Error> {
        let scope = config.scope.as_deref().unwrap_or_default();
        event_definition_from_config(config, scope)
    }
}
