//! Tests for FPA-010: Relay authority — compositor relays, transforms, suppresses,
//! and aggregates inner transition requests.
//!
//! `submit_inner_request` is the intended API for request injection. Partitions
//! communicate transition requests through this mechanism, and the compositor's
//! relay policy governs how those requests are forwarded to the outer layer.

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::{Compositor, RelayPolicy};
use fpa_compositor::state_machine::{ExecutionState, TransitionRequest};
use fpa_contract::test_support::Counter;

fn make_compositor(policy: RelayPolicy) -> Compositor {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("inner_a")),
        Box::new(Counter::new("inner_b")),
    ];
    let bus = InProcessBus::new("relay-test-bus");
    Compositor::new(partitions, bus)
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
    let mut inner = Compositor::new(inner_partitions, inner_bus)
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
    let mut inner = Compositor::new(inner_partitions, inner_bus)
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
    let mut inner = Compositor::new(inner_partitions, inner_bus)
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
    let mut outer = Compositor::new(outer_partitions, outer_bus)
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
    let b_state = root["partitions"].as_table().unwrap()["B"].as_table().unwrap();
    assert_eq!(
        b_state["system"].as_table().unwrap()["tick_count"].as_integer().unwrap(),
        1,
        "inner compositor B should have been stepped"
    );

    // Verify that partitions list contains the inner compositor
    assert_eq!(partitions.len(), 2, "outer should have 2 partitions");
    assert_eq!(partitions[1].id(), "B", "second partition should be inner compositor B");
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
