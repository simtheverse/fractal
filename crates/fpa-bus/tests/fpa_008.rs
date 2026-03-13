// FPA-008 — Layer-scoped Bus
//
// Verifies that each bus instance is independent: messages published on one bus
// are not visible on another. Each compositor owns its own bus instance, and
// sub-partitions communicate only within their layer's bus.

use fpa_bus::{Bus, BusExt, BusReader, InProcessBus};
use fpa_contract::{DeliverySemantic, Message};

#[derive(Clone, Debug, PartialEq)]
struct Event(u32);

impl Message for Event {
    const NAME: &'static str = "Event";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::Queued;
}

#[test]
fn two_buses_are_independent() {
    let bus_a = InProcessBus::new("layer-0");
    let bus_b = InProcessBus::new("layer-1");

    let mut reader_a = bus_a.subscribe::<Event>();
    let mut reader_b = bus_b.subscribe::<Event>();

    // Publish on bus A only.
    bus_a.publish(Event(42));

    // bus A's subscriber should see the message.
    assert_eq!(reader_a.read(), Some(Event(42)));

    // bus B's subscriber must NOT see it.
    assert_eq!(reader_b.read(), None);
}

#[test]
fn publish_on_b_not_visible_on_a() {
    let bus_a = InProcessBus::new("layer-0");
    let bus_b = InProcessBus::new("layer-1");

    let mut reader_a = bus_a.subscribe::<Event>();
    let mut reader_b = bus_b.subscribe::<Event>();

    bus_b.publish(Event(99));

    assert_eq!(reader_b.read(), Some(Event(99)));
    assert_eq!(reader_a.read(), None);
}

#[test]
fn buses_have_distinct_ids() {
    let bus_a = InProcessBus::new("scope-a");
    let bus_b = InProcessBus::new("scope-b");

    assert_ne!(bus_a.id(), bus_b.id());
    assert_eq!(bus_a.id(), "scope-a");
    assert_eq!(bus_b.id(), "scope-b");
}

#[test]
fn multiple_subscribers_on_same_bus_all_receive() {
    let bus = InProcessBus::new("shared");

    let mut r1 = bus.subscribe::<Event>();
    let mut r2 = bus.subscribe::<Event>();

    bus.publish(Event(1));

    // Both subscribers on the same bus should get the message.
    assert_eq!(r1.read(), Some(Event(1)));
    assert_eq!(r2.read(), Some(Event(1)));
}
