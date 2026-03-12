//! Tests for FPA-024: Event Engine integration in Compositor.

use std::collections::HashMap;

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::Counter;
use fpa_events::{EventAction, EventDefinition, EventEngine, EventTrigger, Predicate};

/// Helper to create a compositor with a single counter partition and init it.
fn setup_compositor() -> Compositor {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> =
        vec![Box::new(Counter::new("counter"))];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, bus);
    compositor.init().unwrap();
    compositor
}

/// Event engine is integrated into the compositor Phase 3.
#[test]
fn event_engine_integrated_in_phase3() {
    let mut compositor = setup_compositor();

    // Create a time-triggered event that fires at t >= 0.0
    let events = vec![EventDefinition {
        id: "immediate".to_string(),
        trigger: EventTrigger::Time { at: 0.0 },
        action: EventAction {
            action_id: "test_action".to_string(),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    }];
    compositor.set_event_engine(EventEngine::new(events));

    // Before any tick, no triggered actions
    assert!(compositor.last_triggered_actions().is_empty());

    // After one tick, the event should fire
    compositor.run_tick(1.0).unwrap();
    assert_eq!(compositor.last_triggered_actions(), &["test_action"]);
}

/// Time-triggered event fires only after enough ticks have elapsed.
#[test]
fn time_triggered_event_fires_after_enough_ticks() {
    let mut compositor = setup_compositor();

    // Event fires at t >= 3.0; with dt=1.0, tick_count*dt reaches 3.0 at tick 3
    let events = vec![EventDefinition {
        id: "delayed".to_string(),
        trigger: EventTrigger::Time { at: 3.0 },
        action: EventAction {
            action_id: "delayed_action".to_string(),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    }];
    compositor.set_event_engine(EventEngine::new(events));

    // Ticks 1 and 2: current_time = 1.0, 2.0 — should NOT fire
    compositor.run_tick(1.0).unwrap();
    assert!(
        compositor.last_triggered_actions().is_empty(),
        "should not fire at t=1.0"
    );

    compositor.run_tick(1.0).unwrap();
    assert!(
        compositor.last_triggered_actions().is_empty(),
        "should not fire at t=2.0"
    );

    // Tick 3: current_time = 3.0 — should fire
    compositor.run_tick(1.0).unwrap();
    assert_eq!(
        compositor.last_triggered_actions(),
        &["delayed_action"],
        "should fire at t=3.0"
    );
}

/// Condition-triggered event fires when partition output meets the condition.
#[test]
fn condition_triggered_event_fires_on_partition_output() {
    let mut compositor = setup_compositor();

    // Event fires when counter.count > 1.0
    // Counter increments each tick, so after tick 2 the read buffer has count=1
    // (from tick 1's output), and after tick 3 the read buffer has count=2.
    let events = vec![EventDefinition {
        id: "count_check".to_string(),
        trigger: EventTrigger::Condition {
            signal: "counter.count".to_string(),
            predicate: Predicate::GreaterThan(1.0),
        },
        action: EventAction {
            action_id: "threshold_reached".to_string(),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    }];
    compositor.set_event_engine(EventEngine::new(events));

    // Tick 1: read buffer is empty (no previous output), event should not fire
    compositor.run_tick(1.0).unwrap();
    assert!(
        compositor.last_triggered_actions().is_empty(),
        "tick 1: read buffer empty, should not fire"
    );

    // Tick 2: read buffer has count=1 from tick 1, not > 1.0
    compositor.run_tick(1.0).unwrap();
    assert!(
        compositor.last_triggered_actions().is_empty(),
        "tick 2: count=1, not > 1.0"
    );

    // Tick 3: read buffer has count=2 from tick 2, > 1.0 — should fire
    compositor.run_tick(1.0).unwrap();
    assert_eq!(
        compositor.last_triggered_actions(),
        &["threshold_reached"],
        "tick 3: count=2 > 1.0, should fire"
    );
}

/// Events are evaluated against pre-step state (snapshot semantics).
/// The read buffer is the snapshot from the previous tick, established
/// during Phase 1 (swap). Events in Phase 3 see this snapshot, not the
/// outputs written during Phase 2.
#[test]
fn events_evaluated_against_pre_step_state() {
    let mut compositor = setup_compositor();

    // Event fires when counter.count equals 1 (the pre-step snapshot value).
    // After tick 2's swap, the read buffer has count=1 (tick 1's output).
    // During tick 2, the counter steps to count=2, but the event engine
    // should still see count=1 from the read buffer.
    let events = vec![EventDefinition {
        id: "snapshot_check".to_string(),
        trigger: EventTrigger::Condition {
            signal: "counter.count".to_string(),
            predicate: Predicate::Equal(1.0),
        },
        action: EventAction {
            action_id: "snapshot_action".to_string(),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    }];
    compositor.set_event_engine(EventEngine::new(events));

    // Tick 1: read buffer empty, no signal
    compositor.run_tick(1.0).unwrap();
    assert!(compositor.last_triggered_actions().is_empty());

    // Tick 2: read buffer has count=1 (from tick 1) — event should fire
    // even though the counter itself is now at count=2 after stepping
    compositor.run_tick(1.0).unwrap();
    assert_eq!(
        compositor.last_triggered_actions(),
        &["snapshot_action"],
        "should fire against pre-step snapshot where count=1"
    );

    // Tick 3: read buffer has count=2 (from tick 2) — Equal(1.0) should NOT fire
    compositor.run_tick(1.0).unwrap();
    assert!(
        compositor.last_triggered_actions().is_empty(),
        "should not fire when count=2 in read buffer"
    );
}

/// Compositor without event engine still works (no regression).
#[test]
fn compositor_without_event_engine_works() {
    let mut compositor = setup_compositor();

    compositor.run_tick(1.0).unwrap();
    compositor.run_tick(1.0).unwrap();
    assert_eq!(compositor.tick_count(), 2);
    assert!(compositor.last_triggered_actions().is_empty());
}

/// Multiple time-triggered events that both fire on the same tick produce
/// all expected action IDs in `last_triggered_actions()`.
#[test]
fn multiple_events_fire_in_same_tick() {
    let mut compositor = setup_compositor();

    let events = vec![
        EventDefinition {
            id: "event_a".to_string(),
            trigger: EventTrigger::Time { at: 1.0 },
            action: EventAction {
                action_id: "action_a".to_string(),
                scope: "system".to_string(),
                parameters: HashMap::new(),
            },
            armed: true,
        },
        EventDefinition {
            id: "event_b".to_string(),
            trigger: EventTrigger::Time { at: 1.0 },
            action: EventAction {
                action_id: "action_b".to_string(),
                scope: "system".to_string(),
                parameters: HashMap::new(),
            },
            armed: true,
        },
    ];
    compositor.set_event_engine(EventEngine::new(events));

    // Tick 1: current_time = 1.0 — both events fire
    compositor.run_tick(1.0).unwrap();
    let actions = compositor.last_triggered_actions();
    assert_eq!(actions.len(), 2, "both events should fire on the same tick");
    assert!(actions.contains(&"action_a".to_string()));
    assert!(actions.contains(&"action_b".to_string()));
}

/// System-level and partition-level events use the same EventDefinition schema
/// and both fire correctly through the same EventEngine.
///
/// FPA-024 VE: "system-level event and partition-level event both trigger."
/// The scope field in EventAction distinguishes system vs partition scope,
/// but the schema and engine are identical.
#[test]
fn system_and_partition_events_use_same_schema() {
    let mut compositor = setup_compositor();

    let events = vec![
        EventDefinition {
            id: "sys_event".to_string(),
            trigger: EventTrigger::Time { at: 1.0 },
            action: EventAction {
                action_id: "sys_action".to_string(),
                scope: "system".to_string(),
                parameters: HashMap::new(),
            },
            armed: true,
        },
        EventDefinition {
            id: "part_event".to_string(),
            trigger: EventTrigger::Time { at: 1.0 },
            action: EventAction {
                action_id: "part_action".to_string(),
                scope: "partition:counter".to_string(),
                parameters: HashMap::new(),
            },
            armed: true,
        },
    ];
    compositor.set_event_engine(EventEngine::new(events));

    compositor.run_tick(1.0).unwrap();
    let actions = compositor.last_triggered_actions();
    assert_eq!(actions.len(), 2, "both system and partition events should fire");
    assert!(
        actions.contains(&"sys_action".to_string()),
        "system-scoped event should trigger"
    );
    assert!(
        actions.contains(&"part_action".to_string()),
        "partition-scoped event should trigger"
    );
}

/// A disarmed event does not fire during run_tick, even when its trigger
/// condition is met.
#[test]
fn disarmed_event_does_not_fire_in_compositor() {
    let mut compositor = setup_compositor();

    // Create an event that would fire immediately (at >= 0.0), but start it disarmed
    let mut engine = EventEngine::new(vec![EventDefinition {
        id: "disarmed_event".to_string(),
        trigger: EventTrigger::Time { at: 0.0 },
        action: EventAction {
            action_id: "should_not_fire".to_string(),
            scope: "system".to_string(),
            parameters: HashMap::new(),
        },
        armed: true,
    }]);

    // Disarm the event before installing the engine
    engine.disarm("disarmed_event");
    compositor.set_event_engine(engine);

    // Run a tick — the event's time condition is satisfied but it is disarmed
    compositor.run_tick(1.0).unwrap();
    assert!(
        compositor.last_triggered_actions().is_empty(),
        "disarmed event should not fire even when trigger condition is met"
    );
}
