// FPA-004 — Transport Abstraction
//
// Verifies that InProcessBus implements the Bus trait, that the Transport enum
// has the required variants, and that the transport mode is queryable.

use fpa_bus::{Bus, BusExt, BusReader, InProcessBus, Transport};
use fpa_contract::{DeliverySemantic, Message};

#[derive(Clone, Debug)]
struct Ping;

impl Message for Ping {
    const NAME: &'static str = "Ping";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::Queued;
}

#[test]
fn in_process_bus_implements_bus_trait() {
    // Verify InProcessBus implements Bus by calling trait methods.
    fn assert_bus<T: Bus>(bus: &T) {
        let _ = bus.transport();
        let _ = bus.id();
    }
    let bus = InProcessBus::new("test");
    assert_bus(&bus);
}

#[test]
fn transport_enum_has_required_variants() {
    let _in_process = Transport::InProcess;
    let _async_transport = Transport::Async;
    let _network = Transport::Network;
}

#[test]
fn transport_mode_is_queryable() {
    let bus = InProcessBus::new("layer-0");
    assert_eq!(bus.transport(), Transport::InProcess);
}

#[test]
fn bus_id_is_queryable() {
    let bus = InProcessBus::new("my-bus");
    assert_eq!(bus.id(), "my-bus");
}

#[test]
fn bus_can_publish_and_subscribe() {
    let bus = InProcessBus::new("test");
    let mut reader = bus.subscribe::<Ping>();
    bus.publish(Ping);
    assert!(reader.read().is_some());
}
