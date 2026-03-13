// FPA-035 — Parameterized Bus Tests
//
// Verifies that InProcessBus and AsyncBus produce identical behavior for all
// delivery semantics. Each test scenario is defined as a generic function and
// invoked with both bus implementations.

use fpa_bus::{AsyncBus, Bus, BusExt, BusReader, InProcessBus, NetworkBus, Transport};
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

#[derive(Clone, Debug, PartialEq)]
struct Counter(f64);

impl Message for Counter {
    const NAME: &'static str = "Counter";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}

// --- Test scenarios (generic over Bus) -------------------------------------

fn transport_reports_correct_variant(bus: impl Bus, expected: Transport) {
    assert_eq!(
        bus.transport(),
        expected,
        "{expected:?}: transport() returned wrong variant"
    );
}

#[test]
fn transport_reports_correct_variant_inprocess() {
    transport_reports_correct_variant(InProcessBus::new("t"), Transport::InProcess);
}

#[test]
fn transport_reports_correct_variant_async() {
    transport_reports_correct_variant(AsyncBus::new("t"), Transport::Async);
}

fn latest_value_keeps_only_last(bus: impl Bus, transport: Transport) {
    let mut reader = bus.subscribe::<Temperature>();

    bus.publish(Temperature(10.0));
    bus.publish(Temperature(20.0));
    bus.publish(Temperature(30.0));

    let val = reader
        .read()
        .unwrap_or_else(|| panic!("{transport:?}: expected a value from LatestValue read"));
    assert!(
        (val.0 - 30.0).abs() < f64::EPSILON,
        "{transport:?}: expected 30.0, got {}",
        val.0
    );

    assert!(
        reader.read().is_none(),
        "{transport:?}: expected no more values after read"
    );
}

#[test]
fn latest_value_keeps_only_last_inprocess() {
    latest_value_keeps_only_last(InProcessBus::new("t"), Transport::InProcess);
}

#[test]
fn latest_value_keeps_only_last_async() {
    latest_value_keeps_only_last(AsyncBus::new("t"), Transport::Async);
}

fn queued_preserves_all_in_order(bus: impl Bus, transport: Transport) {
    let mut reader = bus.subscribe::<Command>();

    bus.publish(Command("a".into()));
    bus.publish(Command("b".into()));
    bus.publish(Command("c".into()));

    let all = reader.read_all();
    assert_eq!(
        all.len(),
        3,
        "{transport:?}: expected 3 queued messages, got {}",
        all.len()
    );
    assert_eq!(all[0], Command("a".into()), "{transport:?}: first message");
    assert_eq!(all[1], Command("b".into()), "{transport:?}: second message");
    assert_eq!(all[2], Command("c".into()), "{transport:?}: third message");
}

#[test]
fn queued_preserves_all_in_order_inprocess() {
    queued_preserves_all_in_order(InProcessBus::new("t"), Transport::InProcess);
}

#[test]
fn queued_preserves_all_in_order_async() {
    queued_preserves_all_in_order(AsyncBus::new("t"), Transport::Async);
}

fn queued_read_one_at_a_time(bus: impl Bus, transport: Transport) {
    let mut reader = bus.subscribe::<Command>();

    bus.publish(Command("x".into()));
    bus.publish(Command("y".into()));
    bus.publish(Command("z".into()));

    assert_eq!(reader.read(), Some(Command("x".into())), "{transport:?}");
    assert_eq!(reader.read(), Some(Command("y".into())), "{transport:?}");
    assert_eq!(reader.read(), Some(Command("z".into())), "{transport:?}");
    assert_eq!(reader.read(), None, "{transport:?}: should be empty");
}

#[test]
fn queued_read_one_at_a_time_inprocess() {
    queued_read_one_at_a_time(InProcessBus::new("t"), Transport::InProcess);
}

#[test]
fn queued_read_one_at_a_time_async() {
    queued_read_one_at_a_time(AsyncBus::new("t"), Transport::Async);
}

fn multiple_subscribers_all_receive(bus: impl Bus, transport: Transport) {
    let mut r1 = bus.subscribe::<Command>();
    let mut r2 = bus.subscribe::<Command>();

    bus.publish(Command("hello".into()));

    assert_eq!(
        r1.read(),
        Some(Command("hello".into())),
        "{transport:?}: subscriber 1"
    );
    assert_eq!(
        r2.read(),
        Some(Command("hello".into())),
        "{transport:?}: subscriber 2"
    );
}

#[test]
fn multiple_subscribers_all_receive_inprocess() {
    multiple_subscribers_all_receive(InProcessBus::new("t"), Transport::InProcess);
}

#[test]
fn multiple_subscribers_all_receive_async() {
    multiple_subscribers_all_receive(AsyncBus::new("t"), Transport::Async);
}

fn latest_value_multiple_subscribers(bus: impl Bus, transport: Transport) {
    let mut r1 = bus.subscribe::<Counter>();
    let mut r2 = bus.subscribe::<Counter>();

    bus.publish(Counter(1.0));
    bus.publish(Counter(2.0));
    bus.publish(Counter(3.0));

    let v1 = r1
        .read()
        .unwrap_or_else(|| panic!("{transport:?}: subscriber 1 should have a value"));
    let v2 = r2
        .read()
        .unwrap_or_else(|| panic!("{transport:?}: subscriber 2 should have a value"));

    assert!(
        (v1.0 - 3.0).abs() < f64::EPSILON,
        "{transport:?}: subscriber 1 expected 3.0, got {}",
        v1.0
    );
    assert!(
        (v2.0 - 3.0).abs() < f64::EPSILON,
        "{transport:?}: subscriber 2 expected 3.0, got {}",
        v2.0
    );

    assert!(r1.read().is_none(), "{transport:?}: sub 1 no more values");
    assert!(r2.read().is_none(), "{transport:?}: sub 2 no more values");
}

#[test]
fn latest_value_multiple_subscribers_inprocess() {
    latest_value_multiple_subscribers(InProcessBus::new("t"), Transport::InProcess);
}

#[test]
fn latest_value_multiple_subscribers_async() {
    latest_value_multiple_subscribers(AsyncBus::new("t"), Transport::Async);
}

fn read_all_latest_value_returns_at_most_one(bus: impl Bus, transport: Transport) {
    let mut reader = bus.subscribe::<Temperature>();

    bus.publish(Temperature(5.0));
    bus.publish(Temperature(15.0));

    let all = reader.read_all();
    assert_eq!(
        all.len(),
        1,
        "{transport:?}: read_all for LatestValue should return at most 1"
    );
    assert!(
        (all[0].0 - 15.0).abs() < f64::EPSILON,
        "{transport:?}: expected 15.0, got {}",
        all[0].0
    );

    let all2 = reader.read_all();
    assert!(
        all2.is_empty(),
        "{transport:?}: second read_all should be empty"
    );
}

#[test]
fn read_all_latest_value_returns_at_most_one_inprocess() {
    read_all_latest_value_returns_at_most_one(InProcessBus::new("t"), Transport::InProcess);
}

#[test]
fn read_all_latest_value_returns_at_most_one_async() {
    read_all_latest_value_returns_at_most_one(AsyncBus::new("t"), Transport::Async);
}

fn no_messages_before_subscribe(bus: impl Bus, transport: Transport) {
    bus.publish(Command("pre".into()));

    let mut reader = bus.subscribe::<Command>();
    assert!(
        reader.read().is_none(),
        "{transport:?}: subscriber should not see messages published before subscription"
    );
}

#[test]
fn no_messages_before_subscribe_inprocess() {
    no_messages_before_subscribe(InProcessBus::new("t"), Transport::InProcess);
}

#[test]
fn no_messages_before_subscribe_async() {
    no_messages_before_subscribe(AsyncBus::new("t"), Transport::Async);
}

fn buses_are_independent(bus_a: impl Bus, bus_b: impl Bus, transport: Transport) {
    let mut reader_a = bus_a.subscribe::<Command>();
    let mut reader_b = bus_b.subscribe::<Command>();

    bus_a.publish(Command("only-a".into()));

    assert_eq!(
        reader_a.read(),
        Some(Command("only-a".into())),
        "{transport:?}: bus A subscriber"
    );
    assert_eq!(
        reader_b.read(),
        None,
        "{transport:?}: bus B subscriber should not see bus A messages"
    );
}

#[test]
fn buses_are_independent_inprocess() {
    buses_are_independent(
        InProcessBus::new("a"),
        InProcessBus::new("b"),
        Transport::InProcess,
    );
}

#[test]
fn buses_are_independent_async() {
    buses_are_independent(AsyncBus::new("a"), AsyncBus::new("b"), Transport::Async);
}

// --- Compositor-workflow simulation (FPA-035) --------------------------------
//
// NOTE: Full compositor-level parameterized testing (same compositor config
// running under InProcess vs Async) requires the Compositor to accept a generic
// bus type (`Compositor<B: Bus>`) rather than `InProcessBus` directly. The Bus
// trait is not object-safe due to generic methods, so `dyn Bus` is not possible.
// See docs/feedback/FPA-004.md for the detailed analysis and recommendation.
//
// The tests below simulate a compositor-like workflow at the bus level to verify
// that all three bus implementations produce identical results for a multi-tick
// publish/subscribe pattern.

/// Shared context published each compositor tick (LatestValue semantic).
#[derive(Clone, Debug, PartialEq)]
struct SharedContext {
    tick: u32,
    value: f64,
}

impl Message for SharedContext {
    const NAME: &'static str = "SharedContext";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}

/// Event log produced each tick (Queued semantic).
#[derive(Clone, Debug, PartialEq)]
struct TickEvent(u32);

impl Message for TickEvent {
    const NAME: &'static str = "TickEvent";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::Queued;
}

/// Simulates a 3-tick compositor workflow through the bus:
/// - Each tick publishes a SharedContext (LatestValue) and a TickEvent (Queued)
/// - A LatestValue subscriber reads after all ticks: sees only the last context
/// - A Queued subscriber reads after all ticks: sees all 3 events in order
fn compositor_workflow_simulation(bus: impl Bus, label: &str) {
    let mut ctx_reader = bus.subscribe::<SharedContext>();
    let mut evt_reader = bus.subscribe::<TickEvent>();

    // Simulate 3 compositor ticks.
    for tick in 0..3 {
        bus.publish(SharedContext {
            tick,
            value: tick as f64 * 10.0,
        });
        bus.publish(TickEvent(tick));
    }

    // LatestValue: only the final tick's context should be visible.
    let ctx = ctx_reader
        .read()
        .unwrap_or_else(|| panic!("{label}: expected SharedContext from LatestValue read"));
    assert_eq!(
        ctx,
        SharedContext {
            tick: 2,
            value: 20.0
        },
        "{label}: LatestValue should return only the last published context"
    );
    assert!(
        ctx_reader.read().is_none(),
        "{label}: LatestValue should have no more values"
    );

    // Queued: all 3 tick events should be present, in order.
    let events = evt_reader.read_all();
    assert_eq!(
        events,
        vec![TickEvent(0), TickEvent(1), TickEvent(2)],
        "{label}: Queued should preserve all events in order"
    );
}

#[test]
fn compositor_workflow_simulation_inprocess() {
    compositor_workflow_simulation(InProcessBus::new("comp"), "InProcess");
}

#[test]
fn compositor_workflow_simulation_async() {
    compositor_workflow_simulation(AsyncBus::new("comp"), "Async");
}

#[test]
fn compositor_workflow_simulation_network() {
    compositor_workflow_simulation(NetworkBus::new("comp"), "Network");
}

/// Verifies that subscribing mid-workflow only sees messages from that point forward,
/// simulating a partition that joins a running compositor.
fn late_subscriber_workflow(bus: impl Bus, label: &str) {
    // Tick 0 and 1: no subscriber yet.
    bus.publish(SharedContext {
        tick: 0,
        value: 0.0,
    });
    bus.publish(TickEvent(0));
    bus.publish(SharedContext {
        tick: 1,
        value: 10.0,
    });
    bus.publish(TickEvent(1));

    // Subscriber joins after tick 1.
    let mut ctx_reader = bus.subscribe::<SharedContext>();
    let mut evt_reader = bus.subscribe::<TickEvent>();

    // Tick 2: subscriber is now active.
    bus.publish(SharedContext {
        tick: 2,
        value: 20.0,
    });
    bus.publish(TickEvent(2));

    // LatestValue: should see tick 2 only.
    assert_eq!(
        ctx_reader.read(),
        Some(SharedContext {
            tick: 2,
            value: 20.0
        }),
        "{label}: late subscriber should see tick 2 context"
    );

    // Queued: should see only tick 2 event (missed 0 and 1).
    assert_eq!(
        evt_reader.read_all(),
        vec![TickEvent(2)],
        "{label}: late subscriber should see only tick 2 event"
    );
}

#[test]
fn late_subscriber_workflow_inprocess() {
    late_subscriber_workflow(InProcessBus::new("late"), "InProcess");
}

#[test]
fn late_subscriber_workflow_async() {
    late_subscriber_workflow(AsyncBus::new("late"), "Async");
}

#[test]
fn late_subscriber_workflow_network() {
    late_subscriber_workflow(NetworkBus::new("late"), "Network");
}
