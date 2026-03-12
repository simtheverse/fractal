//! FPA-025: Time-triggered events.

use std::collections::HashMap;

use fpa_events::{EventAction, EventDefinition, EventEngine, EventTrigger};

fn time_event(id: &str, at: f64) -> EventDefinition {
    EventDefinition {
        id: id.to_string(),
        trigger: EventTrigger::Time { at },
        action: EventAction {
            action_id: "notify".to_string(),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    }
}

#[test]
fn time_event_does_not_fire_before_trigger_time() {
    let engine = EventEngine::new(vec![time_event("t1", 5.0)]);
    let signals = HashMap::new();
    let actions = engine.evaluate(3.0, &signals);
    assert!(actions.is_empty(), "event should not fire before trigger time");
}

#[test]
fn time_event_fires_at_trigger_time() {
    let engine = EventEngine::new(vec![time_event("t1", 5.0)]);
    let signals = HashMap::new();
    let actions = engine.evaluate(5.0, &signals);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_id, "notify");
}

#[test]
fn time_event_fires_after_trigger_time() {
    let engine = EventEngine::new(vec![time_event("t1", 5.0)]);
    let signals = HashMap::new();
    let actions = engine.evaluate(6.0, &signals);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_id, "notify");
}

// --- Wall-clock vs logical time tests (FPA-025 Layer 0 / Layer 1) ---

/// Layer 0 events use wall-clock time. In a 2x speed scenario the wall clock
/// reaches 5 s while logical time is already at 10 s.  The Layer 0 caller
/// passes wall-clock elapsed time, so the event at T=5.0 fires at wall 5.0.
#[test]
fn layer_0_time_event_uses_wall_clock() {
    let engine = EventEngine::new(vec![time_event("wall_5", 5.0)]);
    let signals = HashMap::new();

    // Wall clock = 3 s (logical would be 6 s at 2x, but Layer 0 ignores that).
    let actions = engine.evaluate(3.0, &signals);
    assert!(actions.is_empty(), "wall clock has not reached 5.0 yet");

    // Wall clock = 5 s — event fires.
    let actions = engine.evaluate(5.0, &signals);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_id, "notify");
}

/// Layer 1 events use logical (simulation) time. An event at logical T+60
/// fires when the caller passes logical_time=60.0, regardless of how much
/// wall-clock time has actually elapsed.
#[test]
fn layer_1_time_event_uses_logical_time() {
    let engine = EventEngine::new(vec![time_event("logical_60", 60.0)]);
    let signals = HashMap::new();

    // Logical time 59 — not yet.
    assert!(engine.evaluate(59.0, &signals).is_empty());

    // Logical time 60 — fires, even if only 30 s of wall clock passed.
    let actions = engine.evaluate(60.0, &signals);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_id, "notify");
}

/// When the system is paused, logical time does not advance, so a Layer 1
/// event does not fire. We show this by evaluating with the same logical
/// time twice — the event stays unfired because the time never reaches the
/// trigger threshold.
#[test]
fn layer_1_time_event_does_not_advance_while_paused() {
    let engine = EventEngine::new(vec![time_event("pause_test", 10.0)]);
    let signals = HashMap::new();

    // Logical time stuck at 5.0 (system paused).
    assert!(engine.evaluate(5.0, &signals).is_empty());
    assert!(engine.evaluate(5.0, &signals).is_empty(), "paused: logical time unchanged, still should not fire");
}

/// When multiple time events are eligible in the same evaluation, they all
/// fire and are returned in config (insertion) order.
#[test]
fn multiple_time_events_fire_in_config_order() {
    let first = EventDefinition {
        id: "first".to_string(),
        trigger: EventTrigger::Time { at: 1.0 },
        action: EventAction {
            action_id: "action_first".to_string(),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    };
    let second = EventDefinition {
        id: "second".to_string(),
        trigger: EventTrigger::Time { at: 2.0 },
        action: EventAction {
            action_id: "action_second".to_string(),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    };
    let engine = EventEngine::new(vec![first, second]);
    let signals = HashMap::new();

    let actions = engine.evaluate(5.0, &signals);
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].action_id, "action_first", "first event fires first (config order)");
    assert_eq!(actions[1].action_id, "action_second", "second event fires second (config order)");
}
