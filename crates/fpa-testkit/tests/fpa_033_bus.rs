// FPA-033 — Compositor Tests: Inter-partition Bus Communication
//
// Verifies that partitions communicate through the bus using publish/subscribe
// patterns from the reference domains. Tests compositional properties (FPA-037):
// delivery ordering, message conservation, threshold semantics — not exact
// partition output values.
//
// DeferredBus ensures bus messages published during step() are not visible
// until the next tick (one-tick delay), matching SharedContext's double-buffer
// isolation guarantee. This enforces FPA-014's intra-tick isolation for all
// inter-partition communication, not just SharedContext.
//
// Traces to: FPA-005 (typed messages), FPA-007 (delivery semantics),
// FPA-008 (layer-scoped bus), FPA-014 (intra-tick isolation),
// FPA-033 (compositor tests).

use std::sync::Arc;

use fpa_bus::{Bus, BusExt, BusReader, DeferredBus, InProcessBus};
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::{SensorReading, TestCommand};
use fpa_contract::{Partition, StateContribution};

use fpa_testkit::test_partitions::{Follower, Recorder, Sensor};

/// Sensor publishes SensorReading on the bus each step.
///
/// The external reader subscribes on the inner bus and reads after run_tick
/// returns (after flush), so it sees the deferred message.
#[test]
fn sensor_publishes_readings_on_bus() {
    let inner = Arc::new(InProcessBus::new("test"));
    let deferred = Arc::new(DeferredBus::new(inner));
    let bus: Arc<dyn Bus> = deferred.clone();
    let mut reader = bus.subscribe::<SensorReading>();

    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus.clone(), 2.0, 1.0)),
    ];
    let mut compositor = Compositor::from_deferred_bus(partitions, deferred);

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
/// Under deferred delivery, Follower reads the *previous* tick's
/// SensorReading regardless of stepping order. With scale=2.0, offset=0.0,
/// threshold=5.0:
///   tick 1: sensor=2.0, follower reads nothing → no command
///   tick 2: sensor=4.0, follower reads 2.0 → no command
///   tick 3: sensor=6.0, follower reads 4.0 → no command
///   tick 4: sensor=8.0, follower reads 6.0 → command!
#[test]
fn follower_publishes_command_above_threshold() {
    let inner = Arc::new(InProcessBus::new("test"));
    let deferred = Arc::new(DeferredBus::new(inner));
    let bus: Arc<dyn Bus> = deferred.clone();
    let mut cmd_reader = bus.subscribe::<TestCommand>();

    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus.clone(), 2.0, 0.0)),
        Box::new(Follower::new("follower", bus.clone(), 5.0)),
    ];
    let mut compositor = Compositor::from_deferred_bus(partitions, deferred);

    compositor.init().unwrap();

    // tick 1: sensor=2.0, follower reads nothing → no command
    compositor.run_tick(1.0).unwrap();
    assert!(cmd_reader.read().is_none(), "no command: no reading yet");

    // tick 2: sensor=4.0, follower reads 2.0 → below threshold
    compositor.run_tick(1.0).unwrap();
    assert!(cmd_reader.read().is_none(), "no command: reading 2.0 < 5.0");

    // tick 3: sensor=6.0, follower reads 4.0 → below threshold
    compositor.run_tick(1.0).unwrap();
    assert!(cmd_reader.read().is_none(), "no command: reading 4.0 < 5.0");

    // tick 4: sensor=8.0, follower reads 6.0 → at/above threshold → command
    compositor.run_tick(1.0).unwrap();
    let cmd = cmd_reader.read().expect("command should be published at/above threshold");
    assert_eq!(cmd.sequence, 1);
    assert!(cmd.command.contains("threshold_crossed"));

    compositor.shutdown().unwrap();
}

/// Full pipeline: Sensor → Follower → Recorder with deferred delivery.
///
/// Under deferred delivery (scale=1.5, threshold=5.0, 10 ticks):
/// Sensor values per tick: 1.5, 3.0, 4.5, 6.0, 7.5, 9.0, 10.5, 12.0, 13.5, 15.0
///
/// Follower reads previous-tick SensorReading:
///   tick 1: nothing → no command
///   tick 2: reads 1.5 → no command
///   tick 3: reads 3.0 → no command
///   tick 4: reads 4.5 → no command
///   tick 5: reads 6.0 → command (1)
///   tick 6: reads 7.5 → command (2)
///   ...
///   tick 10: reads 13.5 → command (6)
/// → 6 commands sent, last_reading = 13.5
///
/// Recorder reads previous-tick TestCommand (also deferred):
///   tick 6: reads tick 5's command (1)
///   ...
///   tick 10: reads tick 9's command (5)
///   tick 10's command is flushed but not consumed → 5 commands received
///
/// SharedContext: published after flush (non-deferred), consumed next tick.
///   tick 1: no prior context → 0
///   ticks 2-10: read previous tick → 9 entries
#[test]
fn sensor_follower_recorder_pipeline() {
    let inner = Arc::new(InProcessBus::new("test"));
    let deferred = Arc::new(DeferredBus::new(inner));
    let bus: Arc<dyn Bus> = deferred.clone();

    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus.clone(), 1.5, 0.0)),
        Box::new(Follower::new("follower", bus.clone(), 5.0)),
        Box::new(Recorder::new("recorder", bus.clone())),
    ];
    let mut compositor = Compositor::from_deferred_bus(partitions, deferred);

    compositor.init().unwrap();
    for _ in 0..10 {
        compositor.run_tick(1.0).unwrap();
    }

    let state = compositor.dump().unwrap();
    let partitions = state.as_table().unwrap()["partitions"].as_table().unwrap();

    // Follower: last reading = tick 9 value = 13.5, commands = 6 (ticks 5-10)
    let follower_sc = StateContribution::from_toml(&partitions["follower"]).unwrap();
    let follower_state = follower_sc.state.as_table().unwrap();
    assert!(
        (follower_state["last_reading"].as_float().unwrap() - 13.5).abs() < 1e-12,
        "follower should see previous-tick sensor reading (13.5, not 15.0)"
    );
    let commands_sent = follower_state["commands_sent"].as_integer().unwrap();
    assert_eq!(commands_sent, 6, "6 ticks at/above threshold 5.0 (ticks 5-10, reading previous-tick values)");

    // Recorder: 5 commands received (tick 10's command flushed but not consumed)
    let recorder_sc = StateContribution::from_toml(&partitions["recorder"]).unwrap();
    let recorder_state = recorder_sc.state.as_table().unwrap();
    assert_eq!(
        recorder_state["commands_received"].as_integer().unwrap(),
        5,
        "recorder receives 5 of 6 commands (tick 10's command not yet consumed)"
    );

    // Recorder: SharedContext from previous tick → 9 entries in 10 ticks
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
///
/// Under deferred delivery with scale=10.0, threshold=1.0, 5 ticks:
///   tick 1: sensor=10.0, follower reads nothing → no command
///   tick 2: reads 10.0 → command (1)
///   tick 3: reads 20.0 → command (2)
///   tick 4: reads 30.0 → command (3)
///   tick 5: reads 40.0 → command (4)
/// → 4 commands
#[test]
fn queued_delivery_preserves_command_order() {
    let inner = Arc::new(InProcessBus::new("test"));
    let deferred = Arc::new(DeferredBus::new(inner));
    let bus: Arc<dyn Bus> = deferred.clone();
    let mut cmd_reader = bus.subscribe::<TestCommand>();

    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus.clone(), 10.0, 0.0)),
        Box::new(Follower::new("follower", bus.clone(), 1.0)),
    ];
    let mut compositor = Compositor::from_deferred_bus(partitions, deferred);

    compositor.init().unwrap();
    for _ in 0..5 {
        compositor.run_tick(1.0).unwrap();
    }

    // Drain all queued commands and verify ordering
    let commands = cmd_reader.read_all();
    assert_eq!(commands.len(), 4, "should have 4 commands (deferred: first tick has no reading)");
    for (i, cmd) in commands.iter().enumerate() {
        assert_eq!(
            cmd.sequence,
            (i + 1) as u64,
            "commands should arrive in sequence order"
        );
    }

    compositor.shutdown().unwrap();
}

/// Stepping order does NOT affect bus communication under deferred delivery.
///
/// DeferredBus queues all messages published during Phase 2 and flushes
/// them after the tick barrier. Both orderings read previous-tick values,
/// producing identical results — proving FPA-014 intra-tick isolation
/// holds for bus messages as well as SharedContext.
#[test]
fn stepping_order_does_not_affect_bus_communication() {
    // Order A: Sensor steps first
    let inner_a = Arc::new(InProcessBus::new("a"));
    let deferred_a = Arc::new(DeferredBus::new(inner_a));
    let bus_a: Arc<dyn Bus> = deferred_a.clone();
    let parts_a: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus_a.clone(), 1.5, 0.0)),
        Box::new(Follower::new("follower", bus_a.clone(), 5.0)),
    ];
    let mut comp_a = Compositor::from_deferred_bus(parts_a, deferred_a);
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

    // Order B: Follower steps first
    let inner_b = Arc::new(InProcessBus::new("b"));
    let deferred_b = Arc::new(DeferredBus::new(inner_b));
    let bus_b: Arc<dyn Bus> = deferred_b.clone();
    let parts_b: Vec<Box<dyn Partition>> = vec![
        Box::new(Follower::new("follower", bus_b.clone(), 5.0)),
        Box::new(Sensor::new("sensor", bus_b.clone(), 1.5, 0.0)),
    ];
    let mut comp_b = Compositor::from_deferred_bus(parts_b, deferred_b);
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

    // Both orderings produce 6 commands (ticks 5-10, reading previous-tick values)
    assert_eq!(cmds_a, 6, "sensor-first: 6 commands under deferred delivery");
    assert_eq!(cmds_b, 6, "follower-first: 6 commands under deferred delivery");
    assert_eq!(
        cmds_a, cmds_b,
        "stepping order must not affect results — DeferredBus enforces FPA-014 isolation"
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
/// With DeferredBus, partition IDs no longer need prefixes to control
/// stepping order — results are identical regardless of BTreeMap order.
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

    // All three partitions should be present (unprefixed IDs)
    assert!(partitions.contains_key("sensor"), "sensor should be in output");
    assert!(partitions.contains_key("follower"), "follower should be in output");
    assert!(partitions.contains_key("recorder"), "recorder should be in output");

    // Sensor stepped 10 times
    let sensor_sc = StateContribution::from_toml(&partitions["sensor"]).unwrap();
    assert_eq!(
        sensor_sc.state.as_table().unwrap()["step_count"].as_integer().unwrap(),
        10
    );

    // Follower: deferred delivery → reads previous-tick values
    // last_reading = 13.5 (tick 9's value), commands = 6 (ticks 5-10)
    let follower_sc = StateContribution::from_toml(&partitions["follower"]).unwrap();
    let follower_state = follower_sc.state.as_table().unwrap();
    assert!(
        (follower_state["last_reading"].as_float().unwrap() - 13.5).abs() < 1e-12,
        "follower should see previous-tick sensor reading (13.5)"
    );
    assert_eq!(
        follower_state["commands_sent"].as_integer().unwrap(),
        6,
        "6 ticks at/above threshold 5.0 (deferred: ticks 5-10)"
    );

    // Recorder: 5 commands received (tick 10's command not consumed)
    let recorder_sc = StateContribution::from_toml(&partitions["recorder"]).unwrap();
    assert_eq!(
        recorder_sc.state.as_table().unwrap()["commands_received"].as_integer().unwrap(),
        5,
        "recorder receives 5 of 6 commands (tick 10's command not yet consumed)"
    );
}
