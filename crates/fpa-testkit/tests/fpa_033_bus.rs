// FPA-033 — Compositor Tests: Inter-partition Bus Communication
//
// Verifies that partitions communicate through the bus using publish/subscribe
// patterns from the reference domains. Tests compositional properties (FPA-037):
// delivery ordering, message conservation, threshold semantics — not exact
// partition output values.
//
// Important: direct bus messages published during step() are immediately visible
// to partitions stepped later in the same tick. This creates a stepping-order
// dependence for intra-tick bus communication that is distinct from the
// double-buffer isolation guarantee for SharedContext (FPA-014). See
// docs/feedback/FPA-014-bus-message-ordering.md for the full finding.
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

    compositor.shutdown().unwrap();
}

/// Follower subscribes to SensorReading and publishes TestCommand when
/// sensor value is at or above threshold.
///
/// Stepping order: Sensor before Follower ensures same-tick visibility
/// of bus messages. See stepping_order_affects_bus_communication for
/// the complementary test.
#[test]
fn follower_publishes_command_above_threshold() {
    let bus = Arc::new(InProcessBus::new("test"));
    let mut cmd_reader = bus.subscribe::<TestCommand>();

    // Sensor steps before Follower (vector order).
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

    // tick 3: sensor value = 6.0 → at/above threshold → command
    compositor.run_tick(1.0).unwrap();
    let cmd = cmd_reader.read().expect("command should be published at/above threshold");
    assert_eq!(cmd.sequence, 1);
    assert!(cmd.command.contains("threshold_crossed"));

    compositor.shutdown().unwrap();
}

/// Full pipeline: Sensor → Follower → Recorder with queued delivery.
///
/// With Sensor stepping first (vector order), Follower sees sensor's
/// current-tick reading immediately. Sensor values at scale=1.5:
/// tick 1: 1.5, tick 2: 3.0, tick 3: 4.5, tick 4: 6.0, ... tick 10: 15.0
/// Threshold 5.0 reached at tick 4 (value 6.0) onward → 7 commands.
///
/// Recorder sees SharedContext from the *previous* tick (published after
/// all partitions step, consumed next tick) → 9 entries in 10 ticks.
/// Recorder sees TestCommand immediately (Queued, same-tick delivery).
#[test]
fn sensor_follower_recorder_pipeline() {
    let bus = Arc::new(InProcessBus::new("test"));

    // Explicit vector order: Sensor → Follower → Recorder
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus.clone(), 1.5, 0.0)),
        Box::new(Follower::new("follower", bus.clone(), 5.0)),
        Box::new(Recorder::new("recorder", bus.clone())),
    ];
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();
    for _ in 0..10 {
        compositor.run_tick(1.0).unwrap();
    }

    let state = compositor.dump().unwrap();
    let partitions = state.as_table().unwrap()["partitions"].as_table().unwrap();

    // Follower: last reading = tick 10 value = 15.0, commands = 7 (ticks 4-10)
    let follower_sc = StateContribution::from_toml(&partitions["follower"]).unwrap();
    let follower_state = follower_sc.state.as_table().unwrap();
    assert!(
        (follower_state["last_reading"].as_float().unwrap() - 15.0).abs() < 1e-12,
        "follower should see final sensor reading"
    );
    let commands_sent = follower_state["commands_sent"].as_integer().unwrap();
    assert_eq!(commands_sent, 7, "7 ticks at/above threshold 5.0 (ticks 4-10)");

    // Recorder: all 7 commands received (queued, no drops)
    let recorder_sc = StateContribution::from_toml(&partitions["recorder"]).unwrap();
    let recorder_state = recorder_sc.state.as_table().unwrap();
    assert_eq!(
        recorder_state["commands_received"].as_integer().unwrap(),
        7,
        "recorder should receive all 7 commands (queued delivery, no drops)"
    );

    // Recorder: SharedContext is published after all partitions step (end of Phase 2),
    // so on tick 1 Recorder reads nothing (no previous SharedContext yet).
    // On ticks 2-10 Recorder reads the previous tick's SharedContext → 9 entries.
    assert_eq!(
        recorder_state["entries_logged"].as_integer().unwrap(),
        9,
        "recorder sees previous-tick SharedContext: 9 of 10 ticks"
    );

    // Sensor: 10 history entries
    let sensor_sc = StateContribution::from_toml(&partitions["sensor"]).unwrap();
    let history = sensor_sc.state.as_table().unwrap()["history"].as_array().unwrap();
    assert_eq!(history.len(), 10, "sensor history should have 10 entries");

    compositor.shutdown().unwrap();
}

/// Queued delivery preserves ordering: commands arrive in sequence order.
#[test]
fn queued_delivery_preserves_command_order() {
    let bus = Arc::new(InProcessBus::new("test"));
    let mut cmd_reader = bus.subscribe::<TestCommand>();

    // Sensor with scale=10 so every tick exceeds threshold immediately.
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

/// Stepping order affects bus communication timing (FPA-014 finding).
///
/// Direct bus messages published during step() are immediately visible to
/// partitions stepped later in the same tick. This means stepping order
/// determines whether Follower sees the current-tick or previous-tick
/// SensorReading — a dependence that FPA-014's double-buffer isolation
/// does not cover (it only covers SharedContext/contribute_state output).
///
/// This test explicitly demonstrates the difference:
/// - Sensor-first: Follower sees current-tick reading → 7 commands in 10 ticks
/// - Follower-first: Follower sees previous-tick reading → 6 commands in 10 ticks
#[test]
fn stepping_order_affects_bus_communication() {
    // Order A: Sensor steps first → Follower reads current-tick value
    let bus_a = Arc::new(InProcessBus::new("a"));
    let parts_a: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus_a.clone(), 1.5, 0.0)),
        Box::new(Follower::new("follower", bus_a.clone(), 5.0)),
    ];
    let mut comp_a = Compositor::new(parts_a, bus_a);
    comp_a.init().unwrap();
    for _ in 0..10 {
        comp_a.run_tick(1.0).unwrap();
    }
    let state_a = comp_a.dump().unwrap();
    let follower_a = StateContribution::from_toml(
        &state_a.as_table().unwrap()["partitions"].as_table().unwrap()["follower"],
    )
    .unwrap();
    let cmds_a = follower_a.state.as_table().unwrap()["commands_sent"]
        .as_integer()
        .unwrap();
    comp_a.shutdown().unwrap();

    // Order B: Follower steps first → reads previous-tick (or no) SensorReading
    let bus_b = Arc::new(InProcessBus::new("b"));
    let parts_b: Vec<Box<dyn Partition>> = vec![
        Box::new(Follower::new("follower", bus_b.clone(), 5.0)),
        Box::new(Sensor::new("sensor", bus_b.clone(), 1.5, 0.0)),
    ];
    let mut comp_b = Compositor::new(parts_b, bus_b);
    comp_b.init().unwrap();
    for _ in 0..10 {
        comp_b.run_tick(1.0).unwrap();
    }
    let state_b = comp_b.dump().unwrap();
    let follower_b = StateContribution::from_toml(
        &state_b.as_table().unwrap()["partitions"].as_table().unwrap()["follower"],
    )
    .unwrap();
    let cmds_b = follower_b.state.as_table().unwrap()["commands_sent"]
        .as_integer()
        .unwrap();
    comp_b.shutdown().unwrap();

    // Sensor values: 1.5, 3.0, 4.5, 6.0, 7.5, 9.0, 10.5, 12.0, 13.5, 15.0
    // Threshold 5.0.
    // Order A (sensor-first): Follower sees current tick → ticks 4-10 = 7 commands
    // Order B (follower-first): Follower sees previous tick → ticks 5-10 = 6 commands
    assert_eq!(cmds_a, 7, "sensor-first: Follower sees current-tick reading");
    assert_eq!(cmds_b, 6, "follower-first: Follower sees previous-tick reading");
    assert_ne!(
        cmds_a, cmds_b,
        "stepping order should produce different command counts — \
         direct bus messages are not isolated by the double buffer"
    );
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

/// Config-driven composition via composition function (FPA-019).
///
/// The TOML fragment uses prefixed IDs (a_sensor, b_follower, c_recorder)
/// to ensure BTreeMap ordering matches the pipeline's data flow. This test
/// verifies full inter-partition communication through the System entry point.
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
    assert!(partitions.contains_key("a_sensor"), "sensor should be in output");
    assert!(partitions.contains_key("b_follower"), "follower should be in output");
    assert!(partitions.contains_key("c_recorder"), "recorder should be in output");

    // Sensor stepped 10 times
    let sensor_sc = StateContribution::from_toml(&partitions["a_sensor"]).unwrap();
    assert_eq!(
        sensor_sc.state.as_table().unwrap()["step_count"].as_integer().unwrap(),
        10
    );

    // Follower received readings and sent commands (same-tick because a_ < b_)
    let follower_sc = StateContribution::from_toml(&partitions["b_follower"]).unwrap();
    let follower_state = follower_sc.state.as_table().unwrap();
    assert!(
        (follower_state["last_reading"].as_float().unwrap() - 15.0).abs() < 1e-12,
        "follower should see final sensor reading (15.0)"
    );
    assert_eq!(
        follower_state["commands_sent"].as_integer().unwrap(),
        7,
        "7 ticks at/above threshold 5.0"
    );

    // Recorder received all commands
    let recorder_sc = StateContribution::from_toml(&partitions["c_recorder"]).unwrap();
    assert_eq!(
        recorder_sc.state.as_table().unwrap()["commands_received"].as_integer().unwrap(),
        7,
        "recorder should receive all 7 commands"
    );
}
