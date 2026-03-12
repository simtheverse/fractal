//! Tests for FPA-013: Direct signals — bypass relay chain within contract crate scope.

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_compositor::direct_signal::DirectSignalRegistry;
use fpa_contract::test_support::Counter;

/// Direct signals must be registered before they can be emitted.
#[test]
fn unregistered_signal_rejected() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut comp = Compositor::new(partitions, bus).with_id("comp");
    comp.init().unwrap();

    let result = comp.emit_direct_signal("unknown_signal", "test reason", "partition_a");
    assert!(result.is_err(), "unregistered signal should be rejected");
}

/// Registered signal can be emitted.
#[test]
fn registered_signal_emitted() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut comp = Compositor::new(partitions, bus).with_id("comp");
    comp.register_direct_signal("emergency_stop");
    comp.init().unwrap();

    let result = comp.emit_direct_signal("emergency_stop", "overtemp", "partition_a");
    assert!(result.is_ok());

    let signals = comp.emitted_signals();
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].signal_id, "emergency_stop");
    assert_eq!(signals[0].reason, "overtemp");
    assert_eq!(signals[0].emitter_identity, "partition_a");
}

/// Direct signal records layer depth of emitter.
#[test]
fn signal_records_layer_depth() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut comp = Compositor::new(partitions, bus)
        .with_id("inner-comp")
        .with_layer_depth(2);
    comp.register_direct_signal("heartbeat");
    comp.init().unwrap();

    comp.emit_direct_signal("heartbeat", "alive", "partition_a").unwrap();

    let signals = comp.emitted_signals();
    assert_eq!(signals[0].layer_depth, 2);
}

/// Direct signal records emitter identity.
#[test]
fn signal_records_emitter_identity() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut comp = Compositor::new(partitions, bus).with_id("comp");
    comp.register_direct_signal("status");
    comp.init().unwrap();

    comp.emit_direct_signal("status", "ok", "sensor_partition").unwrap();

    let signals = comp.emitted_signals();
    assert_eq!(signals[0].emitter_identity, "sensor_partition");
}

/// Direct signal registry scoping: only registered IDs are allowed.
#[test]
fn registry_scoping() {
    let mut registry = DirectSignalRegistry::new();
    assert!(!registry.is_registered("foo"));

    registry.register("foo");
    assert!(registry.is_registered("foo"));
    assert!(!registry.is_registered("bar"));

    // Duplicate registration is idempotent
    registry.register("foo");
    assert_eq!(registry.registered_ids().len(), 1);
}

/// Two-layer scenario: direct signal from inner compositor bypasses relay chain.
/// Even with Suppress relay policy, direct signals are still emitted.
#[test]
fn direct_signal_bypasses_relay_policy() {
    use fpa_compositor::compositor::RelayPolicy;
    use fpa_compositor::state_machine::{ExecutionState, TransitionRequest};

    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("B1")),
    ];
    let inner_bus = InProcessBus::new("inner-bus");
    let mut inner = Compositor::new(inner_partitions, inner_bus)
        .with_id("B")
        .with_layer_depth(1)
        .with_relay_policy(RelayPolicy::Suppress);

    inner.register_direct_signal("emergency");
    inner.init().unwrap();

    // Submit a transition request (will be suppressed)
    inner.submit_inner_request(TransitionRequest {
        requested_by: "B1".to_string(),
        target_state: ExecutionState::Paused,
    });
    let relayed = inner.drain_relayed_requests();
    assert!(relayed.is_empty(), "relay should be suppressed");

    // But emit a direct signal (bypasses relay)
    inner.emit_direct_signal("emergency", "critical fault", "B1").unwrap();
    assert_eq!(
        inner.emitted_signals().len(),
        1,
        "direct signal should be emitted despite suppress relay policy"
    );
    assert_eq!(inner.emitted_signals()[0].signal_id, "emergency");
    assert_eq!(inner.emitted_signals()[0].emitter_identity, "B1");
    assert_eq!(inner.emitted_signals()[0].layer_depth, 1);
}

/// Two-layer scenario: direct signals from inner compositor propagate to outer
/// compositor after a tick. The outer compositor's `run_tick` calls
/// `collect_inner_signals`, which drains signals from inner compositor partitions.
#[test]
fn inner_signals_propagate_to_outer_after_tick() {
    // Build inner compositor with a registered signal.
    // emit_direct_signal only checks the registry, not execution state,
    // so we can emit before nesting (and before the outer init).
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("B1")),
    ];
    let inner_bus = InProcessBus::new("inner-bus");
    let mut inner = Compositor::new(inner_partitions, inner_bus)
        .with_id("B")
        .with_layer_depth(1);

    inner.register_direct_signal("emergency");
    inner.emit_direct_signal("emergency", "overtemp", "B1").unwrap();

    // Nest inner into outer
    let outer_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(inner),
    ];
    let outer_bus = InProcessBus::new("outer-bus");
    let mut outer = Compositor::new(outer_partitions, outer_bus)
        .with_id("orchestrator");

    outer.init().unwrap();

    // Run a tick — this steps inner compositor (which runs its own tick)
    // and then collects inner signals via collect_inner_signals.
    outer.run_tick(1.0).unwrap();

    // The inner compositor's signal should have been collected by the outer compositor.
    let outer_signals = outer.emitted_signals();
    assert!(
        outer_signals.iter().any(|s| s.signal_id == "emergency"),
        "inner compositor's direct signal should propagate to outer compositor"
    );
    let sig = outer_signals.iter().find(|s| s.signal_id == "emergency").unwrap();
    assert_eq!(sig.layer_depth, 1);
    assert_eq!(sig.emitter_identity, "B1");
}

/// Three-layer scenario: signal emitted at layer 2 propagates through layer 1
/// to layer 0 after ticks.
#[test]
fn signal_propagates_through_three_layers() {
    // Layer 2: innermost compositor — emit signal before nesting
    let l2_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("C1")),
    ];
    let l2_bus = InProcessBus::new("layer-2-bus");
    let mut l2 = Compositor::new(l2_partitions, l2_bus)
        .with_id("L2")
        .with_layer_depth(2);
    l2.register_direct_signal("deep_alert");
    l2.emit_direct_signal("deep_alert", "deep issue", "C1").unwrap();

    // Layer 1: middle compositor containing L2
    let l1_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(l2),
    ];
    let l1_bus = InProcessBus::new("layer-1-bus");
    let l1 = Compositor::new(l1_partitions, l1_bus)
        .with_id("L1")
        .with_layer_depth(1);

    // Layer 0: orchestrator
    let l0_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(l1),
    ];
    let l0_bus = InProcessBus::new("layer-0-bus");
    let mut orchestrator = Compositor::new(l0_partitions, l0_bus)
        .with_id("orchestrator");

    orchestrator.init().unwrap();
    orchestrator.run_tick(1.0).unwrap();

    let signals = orchestrator.emitted_signals();
    assert!(
        signals.iter().any(|s| s.signal_id == "deep_alert"),
        "signal from layer 2 should propagate to layer 0 orchestrator"
    );
    let deep_signal = signals.iter().find(|s| s.signal_id == "deep_alert").unwrap();
    assert_eq!(deep_signal.layer_depth, 2, "signal should retain original layer depth");
    assert_eq!(deep_signal.emitter_identity, "C1");
}

/// Emitted signals can be cleared.
#[test]
fn signals_can_be_cleared() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut comp = Compositor::new(partitions, bus).with_id("comp");
    comp.register_direct_signal("sig");
    comp.init().unwrap();

    comp.emit_direct_signal("sig", "r1", "p1").unwrap();
    comp.emit_direct_signal("sig", "r2", "p2").unwrap();
    assert_eq!(comp.emitted_signals().len(), 2);

    comp.clear_emitted_signals();
    assert!(comp.emitted_signals().is_empty());
}
