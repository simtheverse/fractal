// FPA-033 — Compositor Tests: Inter-partition Bus Communication
//
// Verifies that partitions communicate through the bus using publish/subscribe
// patterns from the reference domains. Tests compositional properties (FPA-037):
// delivery ordering, message conservation, threshold semantics — not exact
// partition output values.
//
// Traces to: FPA-005 (typed messages), FPA-007 (delivery semantics),
// FPA-008 (layer-scoped bus), FPA-033 (compositor tests).

use std::sync::Arc;

use fpa_bus::{BusExt, BusReader, InProcessBus};
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::{SensorReading, TestCommand};
use fpa_contract::{Partition, StateContribution};

use fpa_testkit::test_partitions::{Follower, Recorder, Sensor};

/// Sensor publishes SensorReading on the bus each step.
#[test]
fn sensor_publishes_readings_on_bus() {
    let bus = Arc::new(InProcessBus::new("test"));
    let mut reader = bus.subscribe::<SensorReading>();

    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus.clone(), 2.0, 1.0)),
    ];
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    let reading = reader.read().expect("sensor should publish a reading");
    assert_eq!(reading.source, "sensor");
    // step 1: value = 1 * 2.0 + 1.0 = 3.0
    assert!((reading.value - 3.0).abs() < 1e-12);
}

/// Follower subscribes to SensorReading and publishes TestCommand when
/// threshold is crossed — the core inter-partition communication pattern.
#[test]
fn follower_publishes_command_on_threshold_crossing() {
    let bus = Arc::new(InProcessBus::new("test"));
    let mut cmd_reader = bus.subscribe::<TestCommand>();

    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus.clone(), 2.0, 0.0)),
        Box::new(Follower::new("follower", bus.clone(), 5.0)),
    ];
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();

    // tick 1: sensor value = 2.0 (below threshold 5.0) → no command
    compositor.run_tick(1.0).unwrap();
    assert!(cmd_reader.read().is_none(), "no command below threshold");

    // tick 2: sensor value = 4.0 → still below
    compositor.run_tick(1.0).unwrap();
    assert!(cmd_reader.read().is_none(), "no command below threshold");

    // tick 3: sensor value = 6.0 → crosses threshold
    compositor.run_tick(1.0).unwrap();
    let cmd = cmd_reader.read().expect("command should be published when threshold crossed");
    assert_eq!(cmd.sequence, 1);
    assert!(cmd.command.contains("threshold_crossed"));
}

/// Full pipeline: Sensor → Follower → Recorder with queued delivery.
/// Verifies message conservation: every command published by Follower
/// is received by Recorder (Queued delivery, no drops).
#[test]
fn sensor_follower_recorder_pipeline() {
    let bus = Arc::new(InProcessBus::new("test"));

    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus.clone(), 1.5, 0.0)),
        Box::new(Follower::new("follower", bus.clone(), 5.0)),
        Box::new(Recorder::new("recorder", bus.clone())),
    ];
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();

    // Run 10 ticks. Sensor values: 1.5, 3.0, 4.5, 6.0, 7.5, 9.0, 10.5, 12.0, 13.5, 15.0
    // Threshold 5.0 crossed at tick 4 (value 6.0) onward → 7 commands
    for _ in 0..10 {
        compositor.run_tick(1.0).unwrap();
    }

    let state = compositor.dump().unwrap();
    let partitions = state.as_table().unwrap()["partitions"].as_table().unwrap();

    // Follower state reflects last reading and command count
    let follower_sc = StateContribution::from_toml(&partitions["follower"]).unwrap();
    let follower_state = follower_sc.state.as_table().unwrap();
    let commands_sent = follower_state["commands_sent"].as_integer().unwrap();
    assert!(commands_sent > 0, "follower should have sent commands");

    // Recorder should have received all commands (queued, no drops)
    let recorder_sc = StateContribution::from_toml(&partitions["recorder"]).unwrap();
    let recorder_state = recorder_sc.state.as_table().unwrap();
    let commands_received = recorder_state["commands_received"].as_integer().unwrap();
    assert_eq!(
        commands_received, commands_sent,
        "recorder should receive every command follower sent (queued delivery, no drops)"
    );

    // Recorder should have logged SharedContext entries
    let entries_logged = recorder_state["entries_logged"].as_integer().unwrap();
    assert!(entries_logged > 0, "recorder should have logged SharedContext entries");

    // Sensor state has history (complex/nested state)
    let sensor_sc = StateContribution::from_toml(&partitions["sensor"]).unwrap();
    let sensor_state = sensor_sc.state.as_table().unwrap();
    let history = sensor_state["history"].as_array().unwrap();
    assert_eq!(history.len(), 10, "sensor history should have 10 entries");

    compositor.shutdown().unwrap();
}

/// Queued delivery preserves ordering: commands arrive in sequence order.
#[test]
fn queued_delivery_preserves_command_order() {
    let bus = Arc::new(InProcessBus::new("test"));
    let mut cmd_reader = bus.subscribe::<TestCommand>();

    // Sensor with scale=10 so every tick exceeds threshold immediately
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus.clone(), 10.0, 0.0)),
        Box::new(Follower::new("follower", bus.clone(), 1.0)),
    ];
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();
    for _ in 0..5 {
        compositor.run_tick(1.0).unwrap();
    }

    // Drain all queued commands and verify ordering
    let commands = cmd_reader.read_all();
    assert_eq!(commands.len(), 5, "should have 5 commands");
    for (i, cmd) in commands.iter().enumerate() {
        assert_eq!(
            cmd.sequence,
            (i + 1) as u64,
            "commands should arrive in sequence order"
        );
    }

    compositor.shutdown().unwrap();
}

/// Sensor state round-trips through contribute_state/load_state,
/// including nested history array (complex state).
#[test]
fn sensor_state_round_trip_with_history() {
    let bus = Arc::new(InProcessBus::new("test"));
    let mut sensor = Sensor::new("s", bus, 2.0, 1.0);
    sensor.init().unwrap();
    for _ in 0..3 {
        sensor.step(1.0).unwrap();
    }

    let state = sensor.contribute_state().unwrap();

    let bus2 = Arc::new(InProcessBus::new("test2"));
    let mut sensor2 = Sensor::new("s", bus2, 0.0, 0.0);
    sensor2.load_state(state.clone()).unwrap();

    let reloaded = sensor2.contribute_state().unwrap();
    assert_eq!(state, reloaded, "sensor state should round-trip");
}

/// Follower and Recorder state round-trips.
#[test]
fn follower_recorder_state_round_trip() {
    let bus = Arc::new(InProcessBus::new("test"));

    // Follower
    let mut follower = Follower::new("f", bus.clone(), 3.0);
    let mut table = toml::map::Map::new();
    table.insert("last_reading".to_string(), toml::Value::Float(7.5));
    table.insert("commands_sent".to_string(), toml::Value::Integer(4));
    table.insert("threshold".to_string(), toml::Value::Float(3.0));
    let state = toml::Value::Table(table);
    follower.load_state(state.clone()).unwrap();
    let reloaded = follower.contribute_state().unwrap();
    assert_eq!(state, reloaded, "follower state should round-trip");

    // Recorder
    let mut recorder = Recorder::new("r", bus);
    let mut table = toml::map::Map::new();
    table.insert("entries_logged".to_string(), toml::Value::Integer(10));
    table.insert("commands_received".to_string(), toml::Value::Integer(5));
    table.insert("last_tick_seen".to_string(), toml::Value::Integer(10));
    let state = toml::Value::Table(table);
    recorder.load_state(state.clone()).unwrap();
    let reloaded = recorder.contribute_state().unwrap();
    assert_eq!(state, reloaded, "recorder state should round-trip");
}

/// Config-driven construction via composition function (FPA-019).
#[test]
fn config_driven_composition() {
    let fragment = fpa_config::load_from_str(
        include_str!("../test-configs/sensor-follower.toml"),
    )
    .unwrap();

    let registry = fpa_testkit::registry::with_all_test_partitions();
    let bus = Arc::new(InProcessBus::new("system-bus"));

    let mut system = fpa_testkit::system::System::from_fragment(&fragment, &registry, bus).unwrap();
    let state = system.run(10, 1.0).unwrap();

    let partitions = state.as_table().unwrap()["partitions"].as_table().unwrap();

    // All three partitions should be present
    assert!(partitions.contains_key("sensor"), "sensor should be in output");
    assert!(partitions.contains_key("follower"), "follower should be in output");
    assert!(partitions.contains_key("recorder"), "recorder should be in output");

    // Sensor stepped 10 times
    let sensor_sc = StateContribution::from_toml(&partitions["sensor"]).unwrap();
    let step_count = sensor_sc.state.as_table().unwrap()["step_count"]
        .as_integer()
        .unwrap();
    assert_eq!(step_count, 10);
}
