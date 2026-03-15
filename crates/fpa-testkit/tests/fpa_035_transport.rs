// FPA-035 — Transport Parameterization: Bus-Communicating Partitions
//
// Verifies that the same composition with bus-communicating partitions
// produces identical final state under all three transport modes
// (InProcess, Async, Network). This extends FPA-004's transport
// equivalence guarantee to partitions that actively use the bus.
//
// Traces to: FPA-004 (transport abstraction), FPA-035 (parameterized tests).

use std::sync::Arc;

use fpa_bus::{AsyncBus, Bus, InProcessBus, NetworkBus};
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::{SensorReading, TestCommand};
use fpa_contract::{Partition, StateContribution};

use fpa_testkit::test_partitions::{Follower, Recorder, Sensor};

/// Build and run a sensor-follower-recorder composition with the given bus.
/// Uses explicit vector ordering (Sensor → Follower → Recorder) so that
/// direct bus messages flow within each tick.
fn run_pipeline(bus: Arc<dyn Bus>, ticks: u64) -> toml::Value {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Sensor::new("sensor", bus.clone(), 1.5, 0.0)),
        Box::new(Follower::new("follower", bus.clone(), 5.0)),
        Box::new(Recorder::new("recorder", bus.clone())),
    ];
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();
    for _ in 0..ticks {
        compositor.run_tick(1.0).unwrap();
    }

    let state = compositor.dump().unwrap();
    compositor.shutdown().unwrap();
    state
}

fn make_network_bus(id: &str) -> NetworkBus {
    let bus = NetworkBus::new(id).with_framework_codecs();
    // Register codecs for test message types so NetworkBus serializes them.
    bus.register_codec::<SensorReading>();
    bus.register_codec::<TestCommand>();
    bus
}

/// Same composition produces identical state with InProcessBus and AsyncBus.
#[test]
fn same_result_inprocess_and_async() {
    let state_ip = run_pipeline(Arc::new(InProcessBus::new("ip")), 10);
    let state_async = run_pipeline(Arc::new(AsyncBus::new("ab")), 10);
    assert_eq!(state_ip, state_async);
}

/// Same composition produces identical state with InProcessBus and NetworkBus.
#[test]
fn same_result_inprocess_and_network() {
    let state_ip = run_pipeline(Arc::new(InProcessBus::new("ip")), 10);
    let state_net = run_pipeline(Arc::new(make_network_bus("nb")), 10);
    assert_eq!(state_ip, state_net);
}

/// All three transports produce identical state.
#[test]
fn same_result_all_three_transports() {
    let state_ip = run_pipeline(Arc::new(InProcessBus::new("ip")), 10);
    let state_async = run_pipeline(Arc::new(AsyncBus::new("ab")), 10);
    let state_net = run_pipeline(Arc::new(make_network_bus("nb")), 10);
    assert_eq!(state_ip, state_async);
    assert_eq!(state_async, state_net);
}

/// Compositional property: queued command count is conserved across transports
/// and matches the expected count (FPA-037).
#[test]
fn command_conservation_across_transports() {
    for (label, bus) in [
        ("inprocess", Arc::new(InProcessBus::new("ip")) as Arc<dyn Bus>),
        ("async", Arc::new(AsyncBus::new("ab")) as Arc<dyn Bus>),
        ("network", Arc::new(make_network_bus("nb")) as Arc<dyn Bus>),
    ] {
        let state = run_pipeline(bus, 10);
        let partitions = state.as_table().unwrap()["partitions"].as_table().unwrap();

        let follower_sc = StateContribution::from_toml(&partitions["follower"]).unwrap();
        let commands_sent = follower_sc.state.as_table().unwrap()["commands_sent"]
            .as_integer()
            .unwrap();

        let recorder_sc = StateContribution::from_toml(&partitions["recorder"]).unwrap();
        let commands_received = recorder_sc.state.as_table().unwrap()["commands_received"]
            .as_integer()
            .unwrap();

        // Sensor-first ordering: ticks 4-10 at/above threshold → 7 commands
        assert_eq!(
            commands_sent, 7,
            "{}: follower should send 7 commands", label
        );
        assert_eq!(
            commands_sent, commands_received,
            "{}: queued command count should be conserved (sent={}, received={})",
            label, commands_sent, commands_received
        );
    }
}

/// Config-driven composition via System under all transports.
#[test]
fn system_composition_all_transports() {
    let fragment = fpa_config::load_from_str(
        include_str!("../test-configs/sensor-follower.toml"),
    )
    .unwrap();
    let registry = fpa_testkit::registry::with_all_test_partitions();

    let state_ip = {
        let bus = Arc::new(InProcessBus::new("ip"));
        let mut sys = fpa_testkit::system::System::from_fragment(&fragment, &registry, bus).unwrap();
        sys.run(10, 1.0).unwrap()
    };

    let state_async = {
        let bus = Arc::new(AsyncBus::new("ab"));
        let mut sys = fpa_testkit::system::System::from_fragment(&fragment, &registry, bus).unwrap();
        sys.run(10, 1.0).unwrap()
    };

    let state_net = {
        let bus: Arc<dyn Bus> = Arc::new(make_network_bus("nb"));
        let mut sys = fpa_testkit::system::System::from_fragment(&fragment, &registry, bus).unwrap();
        sys.run(10, 1.0).unwrap()
    };

    assert_eq!(state_ip, state_async, "InProcess vs Async should match");
    assert_eq!(state_async, state_net, "Async vs Network should match");
}
