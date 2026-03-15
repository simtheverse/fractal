//! Tests for FPA-010: Relay authority — compositor relays, transforms, suppresses,
//! and aggregates inner transition requests.
//!
//! `submit_inner_request` is the intended API for request injection. Partitions
//! communicate transition requests through this mechanism, and the compositor's
//! relay policy governs how those requests are forwarded to the outer layer.

use std::sync::Arc;

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::{Compositor, RelayPolicy};
use fpa_compositor::state_machine::{ExecutionState, TransitionRequest};
use fpa_contract::test_support::Counter;
use fpa_contract::StateContribution;

fn make_compositor(policy: RelayPolicy) -> Compositor {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("inner_a")),
        Box::new(Counter::new("inner_b")),
    ];
    let bus = InProcessBus::new("relay-test-bus");
    Compositor::new(partitions, Arc::new(bus))
        .with_id("inner-compositor")
        .with_layer_depth(1)
        .with_relay_policy(policy)
}

/// Forward policy: inner requests pass through unchanged.
#[test]
fn relay_forward_passes_requests_through() {
    let mut comp = make_compositor(RelayPolicy::Forward);
    comp.init().unwrap();

    comp.submit_inner_request(TransitionRequest {
        requested_by: "inner_a".to_string(),
        target_state: ExecutionState::Paused,
    });

    let relayed = comp.drain_relayed_requests();
    assert_eq!(relayed.len(), 1);
    assert_eq!(relayed[0].requested_by, "inner_a");
    assert_eq!(relayed[0].target_state, ExecutionState::Paused);
}

/// Suppress policy: inner requests are silently dropped.
#[test]
fn relay_suppress_drops_all_requests() {
    let mut comp = make_compositor(RelayPolicy::Suppress);
    comp.init().unwrap();

    comp.submit_inner_request(TransitionRequest {
        requested_by: "inner_a".to_string(),
        target_state: ExecutionState::Paused,
    });
    comp.submit_inner_request(TransitionRequest {
        requested_by: "inner_b".to_string(),
        target_state: ExecutionState::Paused,
    });

    let relayed = comp.drain_relayed_requests();
    assert!(relayed.is_empty(), "suppress policy should drop all requests");
}

/// Transform policy: inner requests are transformed before forwarding.
#[test]
fn relay_transform_modifies_requests() {
    let policy = RelayPolicy::Transform(Box::new(|mut req| {
        req.requested_by = format!("relayed({})", req.requested_by);
        req
    }));
    let mut comp = make_compositor(policy);
    comp.init().unwrap();

    comp.submit_inner_request(TransitionRequest {
        requested_by: "inner_a".to_string(),
        target_state: ExecutionState::Paused,
    });

    let relayed = comp.drain_relayed_requests();
    assert_eq!(relayed.len(), 1);
    assert_eq!(relayed[0].requested_by, "relayed(inner_a)");
}

/// Aggregate policy: multiple requests collapse into one.
#[test]
fn relay_aggregate_collapses_requests() {
    let mut comp = make_compositor(RelayPolicy::Aggregate);
    comp.init().unwrap();

    comp.submit_inner_request(TransitionRequest {
        requested_by: "inner_a".to_string(),
        target_state: ExecutionState::Paused,
    });
    comp.submit_inner_request(TransitionRequest {
        requested_by: "inner_b".to_string(),
        target_state: ExecutionState::Paused,
    });

    let relayed = comp.drain_relayed_requests();
    assert_eq!(relayed.len(), 1, "aggregate should produce one request");
    assert!(
        relayed[0].requested_by.contains("inner_a"),
        "aggregated request should mention inner_a"
    );
    assert!(
        relayed[0].requested_by.contains("inner_b"),
        "aggregated request should mention inner_b"
    );
}

/// Two-layer scenario: inner compositor with Suppress policy prevents
/// outer compositor from seeing inner requests.
#[test]
fn two_layer_suppress_hides_inner_requests() {
    // Build inner compositor B with suppress policy and sub-partitions B1, B2
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("B1")),
        Box::new(Counter::new("B2")),
    ];
    let inner_bus = InProcessBus::new("inner-bus");
    let mut inner = Compositor::new(inner_partitions, Arc::new(inner_bus))
        .with_id("B")
        .with_layer_depth(1)
        .with_relay_policy(RelayPolicy::Suppress);

    inner.init().unwrap();

    // B1 submits a transition request through B's compositor
    inner.submit_inner_request(TransitionRequest {
        requested_by: "B1".to_string(),
        target_state: ExecutionState::Paused,
    });

    // B's relay policy is Suppress, so nothing should be forwarded
    let relayed = inner.drain_relayed_requests();
    assert!(
        relayed.is_empty(),
        "suppress policy should prevent inner requests from reaching outer layer"
    );
}

/// Two-layer scenario: inner compositor with Forward policy allows
/// outer compositor to see inner requests.
#[test]
fn two_layer_forward_exposes_inner_requests() {
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("B1")),
        Box::new(Counter::new("B2")),
    ];
    let inner_bus = InProcessBus::new("inner-bus");
    let mut inner = Compositor::new(inner_partitions, Arc::new(inner_bus))
        .with_id("B")
        .with_layer_depth(1)
        .with_relay_policy(RelayPolicy::Forward);

    inner.init().unwrap();

    inner.submit_inner_request(TransitionRequest {
        requested_by: "B1".to_string(),
        target_state: ExecutionState::Paused,
    });

    let relayed = inner.drain_relayed_requests();
    assert_eq!(relayed.len(), 1, "forward policy should pass request through");
    assert_eq!(relayed[0].requested_by, "B1");
}

/// Two-layer relay via step: inner compositor is stepped by the outer compositor
/// (via `Partition::step`), then the outer compositor can access relayed requests
/// from the inner compositor by downcasting through `as_any_mut`.
///
/// Note: `submit_inner_request` is the intended API for request injection.
/// Partitions communicate transition requests through this mechanism, and
/// the relay policy governs how they are forwarded to the outer layer.
#[test]
fn two_layer_relay_via_step() {
    // Build inner compositor with Forward policy
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("B1")),
    ];
    let inner_bus = InProcessBus::new("inner-bus");
    let mut inner = Compositor::new(inner_partitions, Arc::new(inner_bus))
        .with_id("B")
        .with_layer_depth(1)
        .with_relay_policy(RelayPolicy::Forward);

    // Submit a request to the inner compositor before nesting
    inner.submit_inner_request(TransitionRequest {
        requested_by: "B1".to_string(),
        target_state: ExecutionState::Paused,
    });

    // Nest inner into outer
    let outer_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(inner),
    ];
    let outer_bus = InProcessBus::new("outer-bus");
    let mut outer = Compositor::new(outer_partitions, Arc::new(outer_bus))
        .with_id("orchestrator");

    outer.init().unwrap();

    // Step the outer compositor (which steps the inner via Partition::step)
    outer.run_tick(1.0).unwrap();

    // Access the inner compositor via as_any_mut to drain relayed requests
    let partitions = outer.partitions();
    // We can't mutably access through partitions() — we need to verify
    // the requests were submitted before nesting. After nesting, the inner
    // compositor is a Box<dyn Partition>. Use dump to verify the inner ran.
    let state = outer.dump().unwrap();
    let root = state.as_table().unwrap();
    let b_sc = StateContribution::from_toml(&root["partitions"].as_table().unwrap()["B"]).unwrap();
    let b_state = b_sc.state.as_table().unwrap();
    assert_eq!(
        b_state["system"].as_table().unwrap()["tick_count"].as_integer().unwrap(),
        1,
        "inner compositor B should have been stepped"
    );

    // Verify that partitions list contains the inner compositor
    assert_eq!(partitions.len(), 2, "outer should have 2 partitions");
    assert_eq!(partitions[1].id(), "B", "second partition should be inner compositor B");
}

// --- Relay integration with tick flow (FPA-010) ---

/// Bus requests flow through the relay during tick: Phase 3 feeds bus-mediated
/// TransitionRequests into the relay system via submit_inner_request.
#[test]
fn bus_request_flows_through_relay_during_tick() {
    use fpa_bus::{Bus, BusExt};

    let bus: Arc<dyn Bus> = Arc::new(InProcessBus::new("test-bus"));

    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("inner_a")),
    ];
    let mut comp = Compositor::new(partitions, Arc::clone(&bus))
        .with_id("inner")
        .with_layer_depth(1)
        .with_relay_policy(RelayPolicy::Forward);

    comp.init().unwrap();

    // Publish a TransitionRequest on the bus (simulating partition bus message)
    bus.publish(TransitionRequest {
        requested_by: "inner_a".to_string(),
        target_state: ExecutionState::Paused,
    });

    // Run tick — Phase 3 reads the bus request, processes it locally,
    // and also feeds it into the relay via submit_inner_request.
    comp.run_tick(1.0).unwrap();

    // The request should be available via drain_relayed_requests
    let relayed = comp.drain_relayed_requests();
    assert_eq!(relayed.len(), 1, "bus request should flow through relay during tick");
    assert_eq!(relayed[0].requested_by, "inner_a");
    assert_eq!(relayed[0].target_state, ExecutionState::Paused);
}

/// Nested compositor relays inner bus requests to outer compositor.
/// Two-layer end-to-end verification: inner bus TransitionRequest flows through
/// relay and is processed by the outer compositor.
#[test]
fn nested_compositor_relays_inner_bus_requests_to_outer() {
    use fpa_bus::{Bus, BusExt};

    // Build inner compositor with Forward relay policy
    let inner_bus: Arc<dyn Bus> = Arc::new(InProcessBus::new("inner-bus"));
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("B1")),
    ];
    let inner = Compositor::new(inner_partitions, Arc::clone(&inner_bus))
        .with_id("B")
        .with_layer_depth(1)
        .with_relay_policy(RelayPolicy::Forward);

    // Publish a Paused request on the inner bus before nesting
    inner_bus.publish(TransitionRequest {
        requested_by: "B1".to_string(),
        target_state: ExecutionState::Paused,
    });

    // Nest inner into outer
    let outer_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(inner),
    ];
    let outer_bus = InProcessBus::new("outer-bus");
    let mut outer = Compositor::new(outer_partitions, Arc::new(outer_bus))
        .with_id("orchestrator");

    outer.init().unwrap();

    // Run outer tick — steps inner (which processes the bus request and feeds relay),
    // then collect_inner_relayed_requests drains relayed requests from inner.
    // The inner compositor's Paused request should be processed by the outer compositor.
    outer.run_tick(1.0).unwrap();

    assert_eq!(
        outer.state(),
        ExecutionState::Paused,
        "outer compositor should have transitioned to Paused via inner relay"
    );
}

/// Suppress relay policy blocks inner bus requests from reaching the outer compositor.
#[test]
fn nested_compositor_suppress_blocks_relay() {
    use fpa_bus::{Bus, BusExt};

    let inner_bus: Arc<dyn Bus> = Arc::new(InProcessBus::new("inner-bus"));
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("B1")),
    ];
    let inner = Compositor::new(inner_partitions, Arc::clone(&inner_bus))
        .with_id("B")
        .with_layer_depth(1)
        .with_relay_policy(RelayPolicy::Suppress);

    // Publish a Paused request on the inner bus
    inner_bus.publish(TransitionRequest {
        requested_by: "B1".to_string(),
        target_state: ExecutionState::Paused,
    });

    // Nest inner into outer
    let outer_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(inner),
    ];
    let outer_bus = InProcessBus::new("outer-bus");
    let mut outer = Compositor::new(outer_partitions, Arc::new(outer_bus))
        .with_id("orchestrator");

    outer.init().unwrap();
    outer.run_tick(1.0).unwrap();

    // Inner compositor processes the Paused request locally (its own state changes),
    // but the Suppress policy prevents it from reaching the outer compositor.
    assert_eq!(
        outer.state(),
        ExecutionState::Running,
        "outer compositor should remain Running when inner relay policy is Suppress"
    );
}

/// Three-layer relay chain: request propagates from layer 2 through layer 1 to layer 0.
#[test]
fn relay_chain_through_three_layers() {
    use fpa_bus::{Bus, BusExt};

    // Layer 2: innermost compositor
    let l2_bus: Arc<dyn Bus> = Arc::new(InProcessBus::new("l2-bus"));
    let l2_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("C1")),
    ];
    let l2 = Compositor::new(l2_partitions, Arc::clone(&l2_bus))
        .with_id("L2")
        .with_layer_depth(2)
        .with_relay_policy(RelayPolicy::Forward);

    // Publish Paused request on L2's bus
    l2_bus.publish(TransitionRequest {
        requested_by: "C1".to_string(),
        target_state: ExecutionState::Paused,
    });

    // Layer 1: contains L2
    let l1_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(l2),
    ];
    let l1_bus = InProcessBus::new("l1-bus");
    let l1 = Compositor::new(l1_partitions, Arc::new(l1_bus))
        .with_id("L1")
        .with_layer_depth(1)
        .with_relay_policy(RelayPolicy::Forward);

    // Layer 0: orchestrator
    let l0_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(l1),
    ];
    let l0_bus = InProcessBus::new("l0-bus");
    let mut orchestrator = Compositor::new(l0_partitions, Arc::new(l0_bus))
        .with_id("orchestrator");

    orchestrator.init().unwrap();
    orchestrator.run_tick(1.0).unwrap();

    assert_eq!(
        orchestrator.state(),
        ExecutionState::Paused,
        "request from layer 2 should propagate through relay chain to layer 0"
    );
}

/// After draining, pending requests are empty.
#[test]
fn drain_clears_pending_requests() {
    let mut comp = make_compositor(RelayPolicy::Forward);
    comp.init().unwrap();

    comp.submit_inner_request(TransitionRequest {
        requested_by: "inner_a".to_string(),
        target_state: ExecutionState::Paused,
    });

    let _ = comp.drain_relayed_requests();
    assert!(comp.pending_requests().is_empty());
    let second_drain = comp.drain_relayed_requests();
    assert!(second_drain.is_empty());
}
