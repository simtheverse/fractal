//! Tests for FPA-028: Event configuration in composition fragments.

use fpa_config::{load_from_str, TriggerConfig};

/// System-level events parse correctly from TOML.
#[test]
fn system_level_events_parse() {
    let toml_str = r#"
[system]
timestep = 0.016667

[partitions.physics]
implementation = "default_physics"

[[events]]
id = "timeout"
action = "stop_simulation"
scope = "system"

[events.trigger]
type = "time"
at = 10.0

[[events]]
id = "overheat"
action = "activate_cooling"

[events.trigger]
type = "condition"
signal = "physics.temperature"
predicate = "greater_than"
value = 100.0

[events.parameters]
intensity = 0.8
"#;

    let fragment = load_from_str(toml_str).expect("should parse system-level events");
    assert_eq!(fragment.events.len(), 2);

    let timeout = &fragment.events[0];
    assert_eq!(timeout.id, "timeout");
    assert_eq!(timeout.action, "stop_simulation");
    assert_eq!(timeout.scope.as_deref(), Some("system"));
    match &timeout.trigger {
        TriggerConfig::Time { at } => assert!((at - 10.0).abs() < f64::EPSILON),
        _ => panic!("expected Time trigger"),
    }

    let overheat = &fragment.events[1];
    assert_eq!(overheat.id, "overheat");
    assert_eq!(overheat.action, "activate_cooling");
    assert!(overheat.scope.is_none());
    match &overheat.trigger {
        TriggerConfig::Condition {
            signal,
            predicate,
            value,
        } => {
            assert_eq!(signal, "physics.temperature");
            assert_eq!(predicate, "greater_than");
            assert!((value - 100.0).abs() < f64::EPSILON);
        }
        _ => panic!("expected Condition trigger"),
    }
    assert_eq!(
        overheat.parameters.get("intensity").and_then(|v| v.as_float()),
        Some(0.8)
    );
}

/// Partition-level events parse correctly from TOML.
#[test]
fn partition_level_events_parse() {
    let toml_str = r#"
[system]
timestep = 0.016667

[partitions.physics]
implementation = "default_physics"

[[partitions.physics.events]]
id = "stall_detect"
action = "reset_velocity"

[partitions.physics.events.trigger]
type = "condition"
signal = "physics.velocity"
predicate = "less_than"
value = 0.01
"#;

    let fragment = load_from_str(toml_str).expect("should parse partition-level events");
    let physics = fragment.partitions.get("physics").expect("physics partition");
    assert_eq!(physics.events.len(), 1);

    let stall = &physics.events[0];
    assert_eq!(stall.id, "stall_detect");
    assert_eq!(stall.action, "reset_velocity");
    match &stall.trigger {
        TriggerConfig::Condition {
            signal,
            predicate,
            value,
        } => {
            assert_eq!(signal, "physics.velocity");
            assert_eq!(predicate, "less_than");
            assert!((value - 0.01).abs() < f64::EPSILON);
        }
        _ => panic!("expected Condition trigger"),
    }
}

/// Event config schema is identical at system and partition levels.
/// Both use the same EventConfig struct.
#[test]
fn event_config_schema_identical_at_both_levels() {
    let toml_str = r#"
[system]
timestep = 0.016667

[partitions.physics]
implementation = "default_physics"

[[events]]
id = "sys_event"
action = "sys_action"

[events.trigger]
type = "time"
at = 5.0

[[partitions.physics.events]]
id = "part_event"
action = "part_action"

[partitions.physics.events.trigger]
type = "time"
at = 5.0
"#;

    let fragment = load_from_str(toml_str).expect("should parse");

    let sys_event = &fragment.events[0];
    let part_event = &fragment.partitions.get("physics").unwrap().events[0];

    // Both have the same structure
    assert_eq!(sys_event.id, "sys_event");
    assert_eq!(part_event.id, "part_event");

    // Both use the same TriggerConfig enum
    match (&sys_event.trigger, &part_event.trigger) {
        (TriggerConfig::Time { at: at1 }, TriggerConfig::Time { at: at2 }) => {
            assert!((at1 - at2).abs() < f64::EPSILON, "same trigger config type and value");
        }
        _ => panic!("both should be Time triggers"),
    }
}

/// EventConfig with a time trigger converts to EventDefinition correctly.
#[test]
fn event_config_converts_to_event_definition_time_trigger() {
    use fpa_config::EventConfig;
    use fpa_events::{EventDefinition, EventTrigger};
    use std::collections::HashMap;

    let config = EventConfig {
        id: "timeout".to_string(),
        trigger: TriggerConfig::Time { at: 10.0 },
        action: "stop_simulation".to_string(),
        scope: Some("system".to_string()),
        parameters: HashMap::new(),
    };

    let def = EventDefinition::try_from(&config).expect("conversion should succeed");
    assert_eq!(def.id, "timeout");
    assert_eq!(def.action.action_id, "stop_simulation");
    assert_eq!(def.action.scope, "system");
    assert!(def.armed, "events should start armed by default");
    match &def.trigger {
        EventTrigger::Time { at } => assert!((at - 10.0).abs() < f64::EPSILON),
        _ => panic!("expected Time trigger"),
    }
}

/// EventConfig with a condition trigger converts to EventDefinition correctly.
#[test]
fn event_config_converts_to_event_definition_condition_trigger() {
    use fpa_config::EventConfig;
    use fpa_events::{EventDefinition, EventTrigger};
    use std::collections::HashMap;

    let mut params = HashMap::new();
    params.insert("intensity".to_string(), toml::Value::Float(0.8));

    let config = EventConfig {
        id: "overheat".to_string(),
        trigger: TriggerConfig::Condition {
            signal: "physics.temperature".to_string(),
            predicate: ">".to_string(),
            value: 100.0,
        },
        action: "activate_cooling".to_string(),
        scope: None,
        parameters: params,
    };

    let def = EventDefinition::try_from(&config).expect("conversion should succeed");
    assert_eq!(def.id, "overheat");
    assert_eq!(def.action.action_id, "activate_cooling");
    assert_eq!(def.action.scope, "", "None scope should default to empty string");
    assert_eq!(
        def.action.parameters.get("intensity").and_then(|v| v.as_float()),
        Some(0.8)
    );
    assert!(def.armed);
    match &def.trigger {
        EventTrigger::Condition { predicate } => {
            let mut signals = std::collections::HashMap::new();
            signals.insert("physics.temperature".to_string(), 150.0);
            assert!(predicate.evaluate(&signals), "150 > 100 should be true");
            signals.insert("physics.temperature".to_string(), 50.0);
            assert!(!predicate.evaluate(&signals), "50 > 100 should be false");
        }
        _ => panic!("expected Condition trigger"),
    }
}

/// Unknown predicate operator produces an error.
#[test]
fn event_config_unknown_predicate_errors() {
    use fpa_config::EventConfig;
    use fpa_events::EventDefinition;
    use std::collections::HashMap;

    let config = EventConfig {
        id: "bad".to_string(),
        trigger: TriggerConfig::Condition {
            signal: "x".to_string(),
            predicate: "!=".to_string(),
            value: 1.0,
        },
        action: "noop".to_string(),
        scope: None,
        parameters: HashMap::new(),
    };

    let result = EventDefinition::try_from(&config);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("unknown predicate operator"));
}

/// End-to-end: parse TOML events, convert to EventDefinitions, run EventEngine.
#[test]
fn parsed_event_config_produces_working_event_definition() {
    use fpa_events::{EventDefinition, EventEngine};
    use std::collections::HashMap;

    let toml_str = r#"
[system]
timestep = 0.016667

[partitions.physics]
implementation = "default_physics"

[[events]]
id = "timeout"
action = "stop_simulation"
scope = "system"

[events.trigger]
type = "time"
at = 10.0

[[events]]
id = "overheat"
action = "activate_cooling"

[events.trigger]
type = "condition"
signal = "physics.temperature"
predicate = "greater_than"
value = 100.0
"#;

    let fragment = load_from_str(toml_str).expect("should parse");

    // Convert all EventConfig entries to EventDefinition
    let definitions: Vec<EventDefinition> = fragment
        .events
        .iter()
        .map(|ec| EventDefinition::try_from(ec).expect("conversion should succeed"))
        .collect();

    assert_eq!(definitions.len(), 2);

    let engine = EventEngine::new(definitions);

    // At time 5.0 with low temperature: nothing fires
    let mut signals = HashMap::new();
    signals.insert("physics.temperature".to_string(), 50.0);
    let fired = engine.evaluate(5.0, &signals);
    assert!(fired.is_empty(), "no events should fire at t=5, temp=50");

    // At time 11.0 with low temperature: only timeout fires
    let fired = engine.evaluate(11.0, &signals);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].action_id, "stop_simulation");

    // At time 5.0 with high temperature: only overheat fires
    signals.insert("physics.temperature".to_string(), 150.0);
    let fired = engine.evaluate(5.0, &signals);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].action_id, "activate_cooling");

    // At time 11.0 with high temperature: both fire
    let fired = engine.evaluate(11.0, &signals);
    assert_eq!(fired.len(), 2);
    assert_eq!(fired[0].action_id, "stop_simulation");
    assert_eq!(fired[1].action_id, "activate_cooling");
}

/// FPA-029: validated_event_definition rejects unregistered actions at config load time.
#[test]
fn validated_conversion_rejects_unregistered_action() {
    use fpa_config::{validated_event_definition, EventConfig};
    use fpa_events::ActionRegistry;
    use std::collections::HashMap;

    let mut registry = ActionRegistry::new();
    registry.register("stop_simulation", "system");

    let config = EventConfig {
        id: "e1".to_string(),
        trigger: TriggerConfig::Time { at: 10.0 },
        action: "bogus_action".to_string(),
        scope: Some("system".to_string()),
        parameters: HashMap::new(),
    };

    let result = validated_event_definition(&config, &registry);
    assert!(result.is_err(), "unregistered action should be rejected at config load time");
    assert!(
        result.unwrap_err().contains("not registered"),
        "error should explain the action is not registered"
    );
}

/// FPA-029: validated_event_definition accepts registered actions.
#[test]
fn validated_conversion_accepts_registered_action() {
    use fpa_config::{validated_event_definition, EventConfig};
    use fpa_events::ActionRegistry;
    use std::collections::HashMap;

    let mut registry = ActionRegistry::new();
    registry.register("stop_simulation", "system");

    let config = EventConfig {
        id: "timeout".to_string(),
        trigger: TriggerConfig::Time { at: 10.0 },
        action: "stop_simulation".to_string(),
        scope: Some("system".to_string()),
        parameters: HashMap::new(),
    };

    let result = validated_event_definition(&config, &registry);
    assert!(result.is_ok(), "registered action should be accepted");
}

/// FPA-029: validated_event_definition rejects cross-scope action usage.
#[test]
fn validated_conversion_rejects_cross_scope_action() {
    use fpa_config::{validated_event_definition, EventConfig};
    use fpa_events::ActionRegistry;
    use std::collections::HashMap;

    let mut registry = ActionRegistry::new();
    registry.register("ignite", "system.physics");

    let config = EventConfig {
        id: "e1".to_string(),
        trigger: TriggerConfig::Time { at: 5.0 },
        action: "ignite".to_string(),
        scope: Some("system.gnc".to_string()),
        parameters: HashMap::new(),
    };

    let result = validated_event_definition(&config, &registry);
    assert!(
        result.is_err(),
        "action from system.physics should not be usable at system.gnc"
    );
}

/// Fragments without events still parse (backwards compatibility).
#[test]
fn fragment_without_events_parses() {
    let toml_str = r#"
[system]
timestep = 0.016667

[partitions.physics]
implementation = "default_physics"
"#;

    let fragment = load_from_str(toml_str).expect("should parse without events");
    assert!(fragment.events.is_empty());
    let physics = fragment.partitions.get("physics").unwrap();
    assert!(physics.events.is_empty());
}
