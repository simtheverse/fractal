//! FPA-026: Condition-triggered events and no-cascade snapshot semantics.

use std::collections::HashMap;

use fpa_events::{EventAction, EventDefinition, EventEngine, EventTrigger, Predicate};

fn condition_event(id: &str, predicate: Predicate) -> EventDefinition {
    EventDefinition {
        id: id.to_string(),
        trigger: EventTrigger::Condition { predicate },
        action: EventAction {
            action_id: format!("action_{}", id),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    }
}

#[test]
fn condition_does_not_fire_when_not_met() {
    let engine = EventEngine::new(vec![condition_event(
        "c1",
        Predicate::LessThan { signal: "value_a".to_string(), threshold: 500.0 },
    )]);
    let mut signals = HashMap::new();
    signals.insert("value_a".to_string(), 600.0);
    let actions = engine.evaluate(0.0, &signals);
    assert!(actions.is_empty(), "event should not fire when condition is not met");
}

#[test]
fn condition_fires_when_met() {
    let engine = EventEngine::new(vec![condition_event(
        "c1",
        Predicate::LessThan { signal: "value_a".to_string(), threshold: 500.0 },
    )]);
    let mut signals = HashMap::new();
    signals.insert("value_a".to_string(), 400.0);
    let actions = engine.evaluate(0.0, &signals);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_id, "action_c1");
}

#[test]
fn compound_and_condition_fires_only_when_both_satisfied() {
    let predicate = Predicate::And(
        Box::new(Predicate::GreaterThan { signal: "value_a".to_string(), threshold: 100.0 }),
        Box::new(Predicate::LessThan { signal: "value_a".to_string(), threshold: 500.0 }),
    );
    let engine = EventEngine::new(vec![condition_event("c2", predicate)]);

    // Both satisfied: 200 > 100 AND 200 < 500
    let mut signals = HashMap::new();
    signals.insert("value_a".to_string(), 200.0);
    assert_eq!(engine.evaluate(0.0, &signals).len(), 1);

    // First not satisfied: 50 is NOT > 100
    signals.insert("value_a".to_string(), 50.0);
    assert!(engine.evaluate(0.0, &signals).is_empty());

    // Second not satisfied: 600 is NOT < 500
    signals.insert("value_a".to_string(), 600.0);
    assert!(engine.evaluate(0.0, &signals).is_empty());
}

#[test]
fn no_cascade_snapshot_semantics() {
    // Event A: fires when signal_x > 10.0, action would set signal_x to 5.0
    // Event B: fires when signal_x < 10.0
    // With signal_x = 15.0, Event A fires but Event B must NOT fire
    // because the snapshot is immutable during evaluation.

    let mut params_a = HashMap::new();
    params_a.insert(
        "set_signal_x".to_string(),
        toml::Value::Float(5.0),
    );

    let event_a = EventDefinition {
        id: "a".to_string(),
        trigger: EventTrigger::Condition {
            predicate: Predicate::GreaterThan { signal: "signal_x".to_string(), threshold: 10.0 },
        },
        action: EventAction {
            action_id: "modify_signal".to_string(),
            scope: "system".to_string(),
            parameters: params_a,
        },
        armed: true,
    };

    let event_b = EventDefinition {
        id: "b".to_string(),
        trigger: EventTrigger::Condition {
            predicate: Predicate::LessThan { signal: "signal_x".to_string(), threshold: 10.0 },
        },
        action: EventAction {
            action_id: "react_to_low".to_string(),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    };

    let engine = EventEngine::new(vec![event_a, event_b]);
    let mut signals = HashMap::new();
    signals.insert("signal_x".to_string(), 15.0);

    let actions = engine.evaluate(0.0, &signals);

    // Only Event A should fire; Event B sees the original snapshot (15.0)
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_id, "modify_signal");
}

/// Equality predicate uses exact floating-point comparison (FPA-026).
///
/// The spec requires: "Equality predicates (==) shall use exact floating-point
/// comparison." This test verifies that a near-miss value does not trigger
/// an equality predicate.
#[test]
fn equality_predicate_uses_exact_float_comparison() {
    let engine = EventEngine::new(vec![condition_event(
        "eq_check",
        Predicate::Equal { signal: "value".to_string(), threshold: 1.0 },
    )]);

    // Exact match fires
    let mut signals = HashMap::new();
    signals.insert("value".to_string(), 1.0);
    let actions = engine.evaluate(0.0, &signals);
    assert_eq!(actions.len(), 1, "exact 1.0 should fire");

    // Near-miss does NOT fire (exact comparison, not approximate)
    signals.insert("value".to_string(), 0.9999999999);
    let actions = engine.evaluate(0.0, &signals);
    assert!(actions.is_empty(), "0.9999999999 != 1.0 under exact comparison");

    // Another near-miss
    signals.insert("value".to_string(), 1.0000000001);
    let actions = engine.evaluate(0.0, &signals);
    assert!(actions.is_empty(), "1.0000000001 != 1.0 under exact comparison");
}

/// Condition predicates reference signals by name string (e.g., "altitude",
/// "velocity"), not by memory address or index.  Two events on different
/// named signals fire independently based on their signal's value.
#[test]
fn conditions_reference_signals_by_name() {
    let event_alt = condition_event(
        "altitude_check",
        Predicate::GreaterThan { signal: "altitude".to_string(), threshold: 10000.0 },
    );
    let event_vel = condition_event(
        "velocity_check",
        Predicate::LessThan { signal: "velocity".to_string(), threshold: 100.0 },
    );

    let engine = EventEngine::new(vec![event_alt, event_vel]);

    // Only "altitude" condition met.
    let mut signals = HashMap::new();
    signals.insert("altitude".to_string(), 15000.0);
    signals.insert("velocity".to_string(), 200.0);
    let actions = engine.evaluate(0.0, &signals);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_id, "action_altitude_check");

    // Only "velocity" condition met.
    signals.insert("altitude".to_string(), 5000.0);
    signals.insert("velocity".to_string(), 50.0);
    let actions = engine.evaluate(0.0, &signals);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_id, "action_velocity_check");

    // Both conditions met.
    signals.insert("altitude".to_string(), 15000.0);
    signals.insert("velocity".to_string(), 50.0);
    let actions = engine.evaluate(0.0, &signals);
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].action_id, "action_altitude_check");
    assert_eq!(actions[1].action_id, "action_velocity_check");
}

#[test]
fn cross_signal_compound_predicate_fires_when_both_conditions_met() {
    // Predicate: signal_a > 1.0 AND signal_b > 500.0
    // This is the spec example from FPA-026 that was previously unsupported.
    let predicate = Predicate::And(
        Box::new(Predicate::GreaterThan { signal: "signal_a".to_string(), threshold: 1.0 }),
        Box::new(Predicate::GreaterThan { signal: "signal_b".to_string(), threshold: 500.0 }),
    );
    let event = EventDefinition {
        id: "cross_signal".to_string(),
        trigger: EventTrigger::Condition { predicate },
        action: EventAction {
            action_id: "action_cross".to_string(),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    };
    let engine = EventEngine::new(vec![event]);

    // Both conditions met: signal_a=2.0 > 1.0 AND signal_b=600.0 > 500.0
    let mut signals = HashMap::new();
    signals.insert("signal_a".to_string(), 2.0);
    signals.insert("signal_b".to_string(), 600.0);
    let actions = engine.evaluate(0.0, &signals);
    assert_eq!(actions.len(), 1, "both cross-signal conditions met, event should fire");
    assert_eq!(actions[0].action_id, "action_cross");

    // Only first condition met: signal_a=2.0 > 1.0, signal_b=400.0 NOT > 500.0
    signals.insert("signal_b".to_string(), 400.0);
    let actions = engine.evaluate(0.0, &signals);
    assert!(actions.is_empty(), "signal_b condition not met, event should not fire");

    // Only second condition met: signal_a=0.5 NOT > 1.0, signal_b=600.0 > 500.0
    signals.insert("signal_a".to_string(), 0.5);
    signals.insert("signal_b".to_string(), 600.0);
    let actions = engine.evaluate(0.0, &signals);
    assert!(actions.is_empty(), "signal_a condition not met, event should not fire");

    // Neither condition met
    signals.insert("signal_a".to_string(), 0.5);
    signals.insert("signal_b".to_string(), 400.0);
    let actions = engine.evaluate(0.0, &signals);
    assert!(actions.is_empty(), "neither condition met, event should not fire");
}
