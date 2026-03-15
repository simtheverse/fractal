// Network bus tests (FPA-004).
//
// Verifies that NetworkBus implements the Bus trait with Transport::Network,
// delivers messages correctly through real serialization round-trips, and can
// coexist with InProcessBus at different layers.

use fpa_bus::{AsyncBus, Bus, BusExt, BusReader, InProcessBus, NetworkBus, Transport};
use fpa_contract::{DeliverySemantic, Message};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Ping(u32);

impl Message for Ping {
    const NAME: &'static str = "Ping";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::Queued;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SensorReading(f64);

impl Message for SensorReading {
    const NAME: &'static str = "SensorReading";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}

/// Helper: create a NetworkBus with codecs for test message types.
fn test_bus(id: &str) -> NetworkBus {
    let bus = NetworkBus::new(id);
    bus.register_codec::<Ping>();
    bus.register_codec::<SensorReading>();
    bus
}

#[test]
fn network_bus_reports_network_transport() {
    let bus = test_bus("net-0");
    assert_eq!(bus.transport(), Transport::Network);
}

#[test]
fn network_bus_id_is_queryable() {
    let bus = test_bus("net-layer-0");
    assert_eq!(bus.id(), "net-layer-0");
}

#[test]
fn network_bus_implements_bus_trait() {
    fn assert_bus<T: Bus>(_: &T) {}
    let bus = test_bus("test");
    assert_bus(&bus);
}

#[test]
fn network_bus_publish_subscribe_queued() {
    let bus = test_bus("net");
    let mut reader = bus.subscribe::<Ping>();

    bus.publish(Ping(1));
    bus.publish(Ping(2));
    bus.publish(Ping(3));

    assert_eq!(reader.read(), Some(Ping(1)));
    assert_eq!(reader.read(), Some(Ping(2)));
    assert_eq!(reader.read(), Some(Ping(3)));
    assert_eq!(reader.read(), None);
}

#[test]
fn network_bus_publish_subscribe_latest_value() {
    let bus = test_bus("net");
    let mut reader = bus.subscribe::<SensorReading>();

    bus.publish(SensorReading(1.0));
    bus.publish(SensorReading(2.0));
    bus.publish(SensorReading(3.0));

    // LatestValue: only the most recent value is returned.
    assert_eq!(reader.read(), Some(SensorReading(3.0)));
    assert_eq!(reader.read(), None);
}

#[test]
fn network_bus_read_all() {
    let bus = test_bus("net");
    let mut reader = bus.subscribe::<Ping>();

    bus.publish(Ping(10));
    bus.publish(Ping(20));

    let all = reader.read_all();
    assert_eq!(all, vec![Ping(10), Ping(20)]);
    assert_eq!(reader.read(), None);
}

#[test]
fn network_and_inprocess_coexist_at_different_layers() {
    // Layer 0: Network transport
    let net_bus = test_bus("layer-0");
    // Layer 1: InProcess transport
    let ip_bus = InProcessBus::new("layer-1");

    assert_eq!(net_bus.transport(), Transport::Network);
    assert_eq!(ip_bus.transport(), Transport::InProcess);

    // Subscribe to multiple message types on both buses.
    let mut net_ping = net_bus.subscribe::<Ping>();
    let mut net_sensor = net_bus.subscribe::<SensorReading>();
    let mut ip_ping = ip_bus.subscribe::<Ping>();
    let mut ip_sensor = ip_bus.subscribe::<SensorReading>();

    // Publish Ping on NetworkBus only.
    net_bus.publish(Ping(100));
    net_bus.publish(Ping(101));

    // Publish SensorReading on InProcessBus only.
    ip_bus.publish(SensorReading(42.0));
    ip_bus.publish(SensorReading(43.0));

    // Publish Ping on InProcessBus only.
    ip_bus.publish(Ping(200));

    // Publish SensorReading on NetworkBus only.
    net_bus.publish(SensorReading(99.0));

    // NetworkBus Ping: should see 100, 101 (Queued).
    let net_pings = net_ping.read_all();
    assert_eq!(net_pings, vec![Ping(100), Ping(101)], "NetworkBus should have its own Ping messages");

    // InProcessBus Ping: should see only 200 (Queued).
    let ip_pings = ip_ping.read_all();
    assert_eq!(ip_pings, vec![Ping(200)], "InProcessBus should have its own Ping messages");

    // NetworkBus SensorReading: should see only 99.0 (LatestValue).
    assert_eq!(net_sensor.read(), Some(SensorReading(99.0)), "NetworkBus sensor");
    assert_eq!(net_sensor.read(), None, "NetworkBus sensor should be empty after read");

    // InProcessBus SensorReading: should see only 43.0 (LatestValue).
    assert_eq!(ip_sensor.read(), Some(SensorReading(43.0)), "InProcessBus sensor");
    assert_eq!(ip_sensor.read(), None, "InProcessBus sensor should be empty after read");

    // Verify complete isolation: no cross-bus leakage.
    assert_eq!(net_ping.read(), None, "NetworkBus Ping should be drained");
    assert_eq!(ip_ping.read(), None, "InProcessBus Ping should be drained");
}

#[test]
fn all_three_transports_are_fully_isolated() {
    // Verify that NetworkBus, InProcessBus, and AsyncBus are completely independent.
    let net = test_bus("net");
    let ip = InProcessBus::new("ip");
    let ab = AsyncBus::new("ab");

    let mut net_r = net.subscribe::<Ping>();
    let mut ip_r = ip.subscribe::<Ping>();
    let mut ab_r = ab.subscribe::<Ping>();

    // Publish only on NetworkBus.
    net.publish(Ping(1));

    assert_eq!(net_r.read(), Some(Ping(1)), "NetworkBus should receive its message");
    assert_eq!(ip_r.read(), None, "InProcessBus should not receive NetworkBus message");
    assert_eq!(ab_r.read(), None, "AsyncBus should not receive NetworkBus message");

    // Publish only on AsyncBus.
    ab.publish(Ping(2));

    assert_eq!(net_r.read(), None, "NetworkBus should not receive AsyncBus message");
    assert_eq!(ip_r.read(), None, "InProcessBus should not receive AsyncBus message");
    assert_eq!(ab_r.read(), Some(Ping(2)), "AsyncBus should receive its message");
}

#[test]
fn network_bus_multiple_subscribers_independent() {
    let bus = test_bus("net");

    let mut r1 = bus.subscribe::<Ping>();
    let mut r2 = bus.subscribe::<Ping>();
    let mut r3 = bus.subscribe::<Ping>();

    bus.publish(Ping(10));
    bus.publish(Ping(20));

    // Each subscriber independently receives all messages.
    assert_eq!(r1.read_all(), vec![Ping(10), Ping(20)]);
    assert_eq!(r2.read_all(), vec![Ping(10), Ping(20)]);
    assert_eq!(r3.read_all(), vec![Ping(10), Ping(20)]);

    // After draining, all are empty.
    assert_eq!(r1.read(), None);
    assert_eq!(r2.read(), None);
    assert_eq!(r3.read(), None);
}

#[test]
fn network_bus_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<NetworkBus>();
}

/// Proves that messages survive a full serialization round-trip through the
/// NetworkBus — serialized to JSON bytes on publish, deserialized on read.
#[test]
fn serialization_round_trip_through_bus() {
    let bus = test_bus("net");

    // Queued message round-trip
    let mut ping_reader = bus.subscribe::<Ping>();
    bus.publish(Ping(42));
    let received = ping_reader.read().expect("should receive serialized Ping");
    assert_eq!(received, Ping(42));

    // LatestValue message round-trip
    let mut sensor_reader = bus.subscribe::<SensorReading>();
    bus.publish(SensorReading(98.6));
    bus.publish(SensorReading(99.1));
    let received = sensor_reader.read().expect("should receive serialized SensorReading");
    assert_eq!(received, SensorReading(99.1));
}

/// Verifies that NetworkBus produces identical results to InProcessBus
/// for the same publish/subscribe pattern.
#[test]
fn network_bus_identical_to_inprocess() {
    let net = test_bus("net");
    let ip = InProcessBus::new("ip");

    let mut net_ping = net.subscribe::<Ping>();
    let mut ip_ping = ip.subscribe::<Ping>();
    let mut net_sensor = net.subscribe::<SensorReading>();
    let mut ip_sensor = ip.subscribe::<SensorReading>();

    for i in 0..5 {
        net.publish(Ping(i));
        ip.publish(Ping(i));
        net.publish(SensorReading(i as f64 * 1.5));
        ip.publish(SensorReading(i as f64 * 1.5));
    }

    assert_eq!(net_ping.read_all(), ip_ping.read_all());
    assert_eq!(net_sensor.read(), ip_sensor.read());
}
