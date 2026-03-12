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
