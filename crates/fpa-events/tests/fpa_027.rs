//! FPA-027: Partition-scoped arming and disarming.

use std::collections::HashMap;

use fpa_events::{EventAction, EventDefinition, EventEngine, EventTrigger, Predicate};

fn armed_condition_event(id: &str, signal: &str, threshold: f64) -> EventDefinition {
    EventDefinition {
        id: id.to_string(),
        trigger: EventTrigger::Condition {
            predicate: Predicate::GreaterThan { signal: signal.to_string(), threshold },
        },
        action: EventAction {
            action_id: format!("action_{}", id),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    }
}

#[test]
fn armed_event_fires_when_condition_met() {
    let engine = EventEngine::new(vec![armed_condition_event("e1", "temp", 100.0)]);
    let mut signals = HashMap::new();
    signals.insert("temp".to_string(), 150.0);
    let actions = engine.evaluate(0.0, &signals);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_id, "action_e1");
}

#[test]
fn disarmed_event_does_not_fire() {
    let mut engine = EventEngine::new(vec![armed_condition_event("e1", "temp", 100.0)]);
    engine.disarm("e1");

    let mut signals = HashMap::new();
    signals.insert("temp".to_string(), 150.0);
    let actions = engine.evaluate(0.0, &signals);
    assert!(actions.is_empty(), "disarmed event should not fire");
}

#[test]
fn two_partitions_arm_independent_events() {
    // Simulate two partitions each arming their own event.
    let mut engine = EventEngine::new(vec![
        armed_condition_event("partition_a_event", "pressure", 50.0),
        armed_condition_event("partition_b_event", "altitude", 1000.0),
    ]);

    // Both start armed; disarm partition_a's event, then re-arm it.
    engine.disarm("partition_a_event");
    engine.arm("partition_a_event");

    let mut signals = HashMap::new();
    signals.insert("pressure".to_string(), 60.0);
    signals.insert("altitude".to_string(), 1500.0);

    let actions = engine.evaluate(0.0, &signals);
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].action_id, "action_partition_a_event");
    assert_eq!(actions[1].action_id, "action_partition_b_event");
}

/// Two independent EventEngine instances (one system-level, one partition-level)
/// do not interfere with each other.
#[test]
fn independent_engines_for_system_and_partition() {
    // System-level engine with a system event
    let system_engine = EventEngine::new(vec![armed_condition_event("sys_halt", "time_remaining", 0.0)]);

    // Partition-level engine with a partition-internal event
    let mut partition_engine = EventEngine::new(vec![armed_condition_event("part_stall", "velocity", 0.01)]);

    // Disarming in the partition engine does not affect the system engine
    partition_engine.disarm("part_stall");

    let mut sys_signals = HashMap::new();
    sys_signals.insert("time_remaining".to_string(), -1.0);

    // System engine should still fire (it's not affected by partition disarm)
    // Note: GreaterThan(0.0) and value -1.0 means condition is NOT met
    // Use a value that exceeds threshold
    sys_signals.insert("time_remaining".to_string(), 5.0);
    let sys_actions = system_engine.evaluate(0.0, &sys_signals);
    assert_eq!(sys_actions.len(), 1, "system event should fire independently");
    assert_eq!(sys_actions[0].action_id, "action_sys_halt");

    // Partition engine should not fire (its event is disarmed)
    let mut part_signals = HashMap::new();
    part_signals.insert("velocity".to_string(), 0.005);
    let part_actions = partition_engine.evaluate(0.0, &part_signals);
    assert!(part_actions.is_empty(), "disarmed partition event should not fire");

    // Re-arm partition event: now it fires, system unaffected
    partition_engine.arm("part_stall");

    // But velocity 0.005 is NOT > 0.01, so it shouldn't fire
    let part_actions = partition_engine.evaluate(0.0, &part_signals);
    assert!(part_actions.is_empty(), "condition not met, should not fire");

    // With velocity above threshold, partition event fires
    part_signals.insert("velocity".to_string(), 0.02);
    let part_actions = partition_engine.evaluate(0.0, &part_signals);
    assert_eq!(part_actions.len(), 1, "partition event should fire when condition met");
    assert_eq!(part_actions[0].action_id, "action_part_stall");
}

/// A partition-scoped event can reference a signal name that exists only in
/// the partition's own signal map (never published on any system bus).
/// The partition engine evaluates it correctly.
#[test]
fn partition_scoped_event_on_internal_signal() {
    // "internal_temp" is a partition-internal signal — it appears only in
    // the HashMap passed to this partition's EventEngine.
    let partition_engine = EventEngine::new(vec![armed_condition_event(
        "overheat",
        "internal_temp",
        200.0,
    )]);

    // Signal present only in the partition's local map.
    let mut partition_signals = HashMap::new();
    partition_signals.insert("internal_temp".to_string(), 250.0);

    let actions = partition_engine.evaluate(0.0, &partition_signals);
    assert_eq!(actions.len(), 1, "partition-internal signal should trigger event");
    assert_eq!(actions[0].action_id, "action_overheat");

    // An empty system-level signal map would not contain "internal_temp",
    // confirming the signal is partition-scoped.
    let system_signals: HashMap<String, f64> = HashMap::new();
    let actions = partition_engine.evaluate(0.0, &system_signals);
    assert!(actions.is_empty(), "signal absent from system map, event should not fire");
}

/// Disarm then re-arm an event: it fires again on the next evaluation.
#[test]
fn rearmed_event_fires_again() {
    let mut engine = EventEngine::new(vec![armed_condition_event("e1", "fuel", 50.0)]);
    let mut signals = HashMap::new();
    signals.insert("fuel".to_string(), 80.0);

    // Initially armed — fires.
    assert_eq!(engine.evaluate(0.0, &signals).len(), 1);

    // Disarm — does not fire.
    engine.disarm("e1");
    assert!(engine.evaluate(0.0, &signals).is_empty());

    // Re-arm — fires again.
    engine.arm("e1");
    let actions = engine.evaluate(0.0, &signals);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_id, "action_e1");
}
