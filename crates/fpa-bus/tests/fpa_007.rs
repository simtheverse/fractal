// FPA-007 — Bus Delivery Semantics
//
// Verifies that the bus honours per-message delivery semantics:
// - LatestValue: only the most recent value is retained.
// - Queued: all messages are retained in order.
// Also verifies that delivery semantic is declared per message type in the contract.

use fpa_bus::{Bus, InProcessBus};
use fpa_contract::{DeliverySemantic, Message};

// --- Test message types ----------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct Temperature(f64);

impl Message for Temperature {
    const NAME: &'static str = "Temperature";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}

#[derive(Clone, Debug, PartialEq)]
struct Command(String);

impl Message for Command {
    const NAME: &'static str = "Command";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::Queued;
}

// --- Tests -----------------------------------------------------------------

#[test]
fn latest_value_publish_three_read_once_gets_last() {
    let bus = InProcessBus::new("test");
    let mut reader = bus.subscribe::<Temperature>();

    bus.publish(Temperature(10.0));
    bus.publish(Temperature(20.0));
    bus.publish(Temperature(30.0));

    // Only the last value should be returned.
    let val = reader.read().expect("should get a value");
    assert_eq!(val, Temperature(30.0));

    // No further values available.
    assert!(reader.read().is_none());
}

#[test]
fn queued_publish_three_read_gets_all_in_order() {
    let bus = InProcessBus::new("test");
    let mut reader = bus.subscribe::<Command>();

    bus.publish(Command("a".into()));
    bus.publish(Command("b".into()));
    bus.publish(Command("c".into()));

    let all = reader.read_all();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0], Command("a".into()));
    assert_eq!(all[1], Command("b".into()));
    assert_eq!(all[2], Command("c".into()));
}

#[test]
fn queued_read_one_at_a_time_preserves_order() {
    let bus = InProcessBus::new("test");
    let mut reader = bus.subscribe::<Command>();

    bus.publish(Command("x".into()));
    bus.publish(Command("y".into()));
    bus.publish(Command("z".into()));

    assert_eq!(reader.read(), Some(Command("x".into())));
    assert_eq!(reader.read(), Some(Command("y".into())));
    assert_eq!(reader.read(), Some(Command("z".into())));
    assert_eq!(reader.read(), None);
}

#[test]
fn delivery_semantic_declared_per_message_type() {
    // The delivery semantic is a compile-time constant on the Message trait.
    assert_eq!(Temperature::DELIVERY, DeliverySemantic::LatestValue);
    assert_eq!(Command::DELIVERY, DeliverySemantic::Queued);
}
