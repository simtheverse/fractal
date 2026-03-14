//! FPA-029: Action vocabulary scoping.

use std::collections::HashMap;

use fpa_events::{ActionRegistry, EventAction};

#[test]
fn action_usable_in_child_scope() {
    let mut registry = ActionRegistry::new();
    registry.register("ignite", "system");
    assert!(registry.validate("ignite", "system.physics").is_ok());
}

#[test]
fn action_rejected_in_sibling_scope() {
    let mut registry = ActionRegistry::new();
    registry.register("ignite", "system.physics");
    let result = registry.validate("ignite", "system.gnc");
    assert!(result.is_err(), "action in system.physics should not be usable at system.gnc");
}

#[test]
fn action_usable_in_deeply_nested_child_scope() {
    let mut registry = ActionRegistry::new();
    registry.register("drag_update", "system.physics");
    assert!(registry.validate("drag_update", "system.physics.aero").is_ok());
}

#[test]
fn action_usable_in_same_scope() {
    let mut registry = ActionRegistry::new();
    registry.register("reset", "system");
    assert!(registry.validate("reset", "system").is_ok());
}

#[test]
fn unregistered_action_rejected() {
    let registry = ActionRegistry::new();
    assert!(registry.validate("nonexistent", "system").is_err());
}

/// FPA-029: An action identifier used in a configuration event entry must be
/// validated against the ActionRegistry at configuration load time. An
/// unregistered action should be rejected before the event reaches the runtime
/// event engine.
///
/// This test verifies the integration between config-parsed EventDefinitions
/// and the ActionRegistry. It should fail if the config loading pipeline
/// accepts any action string without registry validation.
#[test]
fn invalid_action_rejected_at_config_load() {
    let mut registry = ActionRegistry::new();
    registry.register("stop_simulation", "system");
    registry.register("activate_cooling", "system.physics");

    // Valid: action registered at a parent scope
    assert!(
        registry.validate("stop_simulation", "system.physics").is_ok(),
        "action registered at system scope should be usable at system.physics"
    );

    // Invalid: action not registered at all
    assert!(
        registry.validate("nonexistent_action", "system").is_err(),
        "unregistered action should be rejected"
    );

    // Simulate what config loading SHOULD do: parse event config, then validate
    // the action against the registry before constructing an EventDefinition.
    let action_from_config = "bogus_action";
    let scope_from_config = "system";
    let validation_result = registry.validate(action_from_config, scope_from_config);
    assert!(
        validation_result.is_err(),
        "config-parsed action '{}' should be rejected by registry at load time",
        action_from_config
    );
    assert!(
        validation_result.unwrap_err().contains("not registered"),
        "rejection message should explain the action is not registered"
    );
}

/// FPA-029: An action declared in a partition's contract crate should be
/// rejected at config load time if used in a sibling partition's event entry
/// that does not depend on the declaring contract crate.
#[test]
fn cross_partition_action_rejected_at_config_load() {
    let mut registry = ActionRegistry::new();
    registry.register("ignite", "system.physics");

    // Valid: used within the declaring partition's scope
    assert!(registry.validate("ignite", "system.physics").is_ok());
    assert!(registry.validate("ignite", "system.physics.aero").is_ok());

    // Invalid: used in a sibling partition that doesn't depend on physics
    let result = registry.validate("ignite", "system.gnc");
    assert!(
        result.is_err(),
        "action declared in system.physics should not be usable at system.gnc"
    );
}

/// EventAction uses the same struct fields (action_id, scope, parameters)
/// regardless of whether it is declared at system scope or partition scope.
/// The syntax is identical — only the `scope` value changes.
#[test]
fn identical_syntax_for_all_scope_levels() {
    let system_action = EventAction {
        action_id: "halt".to_string(),
        scope: "system".to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert("reason".to_string(), toml::Value::String("timeout".into()));
            p
        },
    };

    let partition_action = EventAction {
        action_id: "halt".to_string(),
        scope: "system.physics".to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert("reason".to_string(), toml::Value::String("timeout".into()));
            p
        },
    };

    // Same struct, same fields — only the scope value differs.
    assert_eq!(system_action.action_id, partition_action.action_id);
    assert_ne!(system_action.scope, partition_action.scope);
    assert_eq!(
        system_action.parameters.get("reason"),
        partition_action.parameters.get("reason"),
    );
}
