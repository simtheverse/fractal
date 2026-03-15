// Network bus tests (FPA-004).
//
// Verifies that NetworkBus implements the Bus trait with Transport::Network,
// delivers messages correctly, and can coexist with InProcessBus at different
// layers. Serialization-specific tests are gated behind the json-codec feature.

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

#[test]
fn network_bus_reports_network_transport() {
    let bus = NetworkBus::new("net-0");
    assert_eq!(bus.transport(), Transport::Network);
}

#[test]
fn network_bus_id_is_queryable() {
    let bus = NetworkBus::new("net-layer-0");
    assert_eq!(bus.id(), "net-layer-0");
}

#[test]
fn network_bus_implements_bus_trait() {
    fn assert_bus<T: Bus>(_: &T) {}
    let bus = NetworkBus::new("test");
    assert_bus(&bus);
}

#[test]
fn network_bus_publish_subscribe_queued() {
    let bus = NetworkBus::new("net");
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
    let bus = NetworkBus::new("net");
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
    let bus = NetworkBus::new("net");
    let mut reader = bus.subscribe::<Ping>();

    bus.publish(Ping(10));
    bus.publish(Ping(20));

    let all = reader.read_all();
    assert_eq!(all, vec![Ping(10), Ping(20)]);
    assert_eq!(reader.read(), None);
}

#[test]
fn network_and_inprocess_coexist_at_different_layers() {
    let net_bus = NetworkBus::new("layer-0");
    let ip_bus = InProcessBus::new("layer-1");

    assert_eq!(net_bus.transport(), Transport::Network);
    assert_eq!(ip_bus.transport(), Transport::InProcess);

    let mut net_ping = net_bus.subscribe::<Ping>();
    let mut net_sensor = net_bus.subscribe::<SensorReading>();
    let mut ip_ping = ip_bus.subscribe::<Ping>();
    let mut ip_sensor = ip_bus.subscribe::<SensorReading>();

    net_bus.publish(Ping(100));
    net_bus.publish(Ping(101));
    ip_bus.publish(SensorReading(42.0));
    ip_bus.publish(SensorReading(43.0));
    ip_bus.publish(Ping(200));
    net_bus.publish(SensorReading(99.0));

    let net_pings = net_ping.read_all();
    assert_eq!(net_pings, vec![Ping(100), Ping(101)]);
    let ip_pings = ip_ping.read_all();
    assert_eq!(ip_pings, vec![Ping(200)]);
    assert_eq!(net_sensor.read(), Some(SensorReading(99.0)));
    assert_eq!(net_sensor.read(), None);
    assert_eq!(ip_sensor.read(), Some(SensorReading(43.0)));
    assert_eq!(ip_sensor.read(), None);
    assert_eq!(net_ping.read(), None);
    assert_eq!(ip_ping.read(), None);
}

#[test]
fn all_three_transports_are_fully_isolated() {
    let net = NetworkBus::new("net");
    let ip = InProcessBus::new("ip");
    let ab = AsyncBus::new("ab");

    let mut net_r = net.subscribe::<Ping>();
    let mut ip_r = ip.subscribe::<Ping>();
    let mut ab_r = ab.subscribe::<Ping>();

    net.publish(Ping(1));
    assert_eq!(net_r.read(), Some(Ping(1)));
    assert_eq!(ip_r.read(), None);
    assert_eq!(ab_r.read(), None);

    ab.publish(Ping(2));
    assert_eq!(net_r.read(), None);
    assert_eq!(ip_r.read(), None);
    assert_eq!(ab_r.read(), Some(Ping(2)));
}

#[test]
fn network_bus_multiple_subscribers_independent() {
    let bus = NetworkBus::new("net");

    let mut r1 = bus.subscribe::<Ping>();
    let mut r2 = bus.subscribe::<Ping>();
    let mut r3 = bus.subscribe::<Ping>();

    bus.publish(Ping(10));
    bus.publish(Ping(20));

    assert_eq!(r1.read_all(), vec![Ping(10), Ping(20)]);
    assert_eq!(r2.read_all(), vec![Ping(10), Ping(20)]);
    assert_eq!(r3.read_all(), vec![Ping(10), Ping(20)]);

    assert_eq!(r1.read(), None);
    assert_eq!(r2.read(), None);
    assert_eq!(r3.read(), None);
}

#[test]
fn network_bus_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<NetworkBus>();
}

// --- Serialization-specific tests (require json-codec feature) ---

#[cfg(feature = "json-codec")]
mod serialization {
    use super::*;

    /// Helper: create a NetworkBus with JSON codecs for test message types.
    fn test_bus(id: &str) -> NetworkBus {
        let bus = NetworkBus::new(id);
        bus.register_codec::<Ping>();
        bus.register_codec::<SensorReading>();
        bus
    }

    /// Proves that messages survive a full serialization round-trip through the
    /// NetworkBus — serialized to JSON bytes on publish, deserialized on read.
    #[test]
    fn serialization_round_trip_through_bus() {
        let bus = test_bus("net");

        let mut ping_reader = bus.subscribe::<Ping>();
        bus.publish(Ping(42));
        let received = ping_reader.read().expect("should receive serialized Ping");
        assert_eq!(received, Ping(42));

        let mut sensor_reader = bus.subscribe::<SensorReading>();
        bus.publish(SensorReading(98.6));
        bus.publish(SensorReading(99.1));
        let received = sensor_reader.read().expect("should receive serialized SensorReading");
        assert_eq!(received, SensorReading(99.1));
    }

    /// Verifies that NetworkBus with serialization produces identical results
    /// to InProcessBus for the same publish/subscribe pattern.
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
}
