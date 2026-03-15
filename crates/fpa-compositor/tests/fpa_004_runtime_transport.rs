//! Tests for FPA-004 Phase 4 M2: Runtime Transport in Compositor.
//!
//! Verifies that the same compositor configuration produces identical results
//! when constructed with different bus implementations (InProcessBus, AsyncBus,
//! NetworkBus). This proves runtime transport selection via dependency injection.

use std::sync::Arc;

use fpa_bus::{AsyncBus, Bus, BusExt, BusReader, InProcessBus, NetworkBus, Transport};
use fpa_compositor::compositor::{Compositor, SharedContext};
use fpa_compositor::state_machine::ExecutionState;
use fpa_contract::test_support::Counter;
use fpa_contract::StateContribution;

/// Helper: build a compositor with the given bus, run N ticks, return the dump.
fn run_compositor_with_bus(bus: Arc<dyn Bus>, n: u64) -> toml::Value {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("alpha")),
        Box::new(Counter::new("beta")),
    ];
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();
    for _ in 0..n {
        compositor.run_tick(1.0).unwrap();
    }
    compositor.shutdown().unwrap();

    compositor.dump().unwrap()
}

/// Same compositor config produces identical state with InProcessBus and AsyncBus.
#[test]
fn same_result_inprocess_and_async() {
    let state_inprocess = run_compositor_with_bus(
        Arc::new(InProcessBus::new("test-inprocess")),
        5,
    );
    let state_async = run_compositor_with_bus(
        Arc::new(AsyncBus::new("test-async")),
        5,
    );
    assert_eq!(state_inprocess, state_async);
}

/// Same compositor config produces identical state with InProcessBus and NetworkBus.
#[test]
fn same_result_inprocess_and_network() {
    let state_inprocess = run_compositor_with_bus(
        Arc::new(InProcessBus::new("test-inprocess")),
        5,
    );
    let state_network = run_compositor_with_bus(
        Arc::new(NetworkBus::new("test-network")),
        5,
    );
    assert_eq!(state_inprocess, state_network);
}

/// Same compositor config produces identical state with all three bus types.
#[test]
fn same_result_all_three_transports() {
    let state_inprocess = run_compositor_with_bus(
        Arc::new(InProcessBus::new("test-inprocess")),
        10,
    );
    let state_async = run_compositor_with_bus(
        Arc::new(AsyncBus::new("test-async")),
        10,
    );
    let state_network = run_compositor_with_bus(
        Arc::new(NetworkBus::new("test-network")),
        10,
    );
    assert_eq!(state_inprocess, state_async);
    assert_eq!(state_async, state_network);
}

/// Bus transport mode is queryable through the compositor.
#[test]
fn compositor_bus_reports_correct_transport() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];

    let compositor = Compositor::new(partitions, Arc::new(InProcessBus::new("ip")));
    assert_eq!(compositor.bus().transport(), Transport::InProcess);

    let partitions2: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let compositor2 = Compositor::new(partitions2, Arc::new(AsyncBus::new("ab")));
    assert_eq!(compositor2.bus().transport(), Transport::Async);

    let partitions3: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let compositor3 = Compositor::new(partitions3, Arc::new(NetworkBus::new("nb")));
    assert_eq!(compositor3.bus().transport(), Transport::Network);
}

/// SharedContext is published on the bus regardless of transport type.
#[test]
fn shared_context_published_on_all_transports() {
    for (label, bus) in [
        ("inprocess", Arc::new(InProcessBus::new("ip")) as Arc<dyn Bus>),
        ("async", Arc::new(AsyncBus::new("ab")) as Arc<dyn Bus>),
        ("network", Arc::new(NetworkBus::new("nb")) as Arc<dyn Bus>),
    ] {
        let mut reader = bus.subscribe::<SharedContext>();

        let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
            Box::new(Counter::new("counter")),
        ];
        let mut compositor = Compositor::new(partitions, bus);

        compositor.init().unwrap();
        compositor.run_tick(1.0).unwrap();

        let ctx = reader.read();
        assert!(
            ctx.is_some(),
            "{}: SharedContext should be published on the bus after run_tick",
            label
        );
        let ctx = ctx.unwrap();
        assert_eq!(ctx.tick, 1, "{}: SharedContext tick should be 1", label);

        let state_table = ctx.state.as_table().unwrap();
        assert!(
            state_table.contains_key("counter"),
            "{}: SharedContext should contain 'counter' partition state",
            label
        );
    }
}

/// new_default convenience constructor creates an InProcessBus internally.
#[test]
fn new_default_creates_inprocess_bus() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("a")),
    ];
    let compositor = Compositor::new_default(partitions, "default-bus");

    assert_eq!(compositor.bus().transport(), Transport::InProcess);
    assert_eq!(compositor.bus().id(), "default-bus");
}

/// Nested compositors can use different bus types at each layer.
#[test]
fn nested_compositors_with_different_transports() {
    // Inner compositor uses AsyncBus
    let inner_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("B1")),
    ];
    let inner = Compositor::new(inner_partitions, Arc::new(AsyncBus::new("inner-async")))
        .with_id("B")
        .with_layer_depth(1);

    // Outer compositor uses NetworkBus
    let outer_partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("A")),
        Box::new(inner),
    ];
    let mut outer = Compositor::new(outer_partitions, Arc::new(NetworkBus::new("outer-network")))
        .with_id("orchestrator");

    outer.init().unwrap();
    for _ in 0..3 {
        outer.run_tick(1.0).unwrap();
    }

    // Verify outer bus type
    assert_eq!(outer.bus().transport(), Transport::Network);

    // Verify state: both layers should have 3 ticks
    let state = outer.dump().unwrap();
    let partitions = state.as_table().unwrap()["partitions"].as_table().unwrap();

    let a_sc = StateContribution::from_toml(&partitions["A"]).unwrap();
    let a_count = a_sc.state.as_table().unwrap()["count"]
        .as_integer().unwrap();
    assert_eq!(a_count, 3);

    let b_sc = StateContribution::from_toml(&partitions["B"]).unwrap();
    let b_tick_count = b_sc.state.as_table().unwrap()["system"]
        .as_table().unwrap()["tick_count"]
        .as_integer().unwrap();
    assert_eq!(b_tick_count, 3);

    outer.shutdown().unwrap();
}

/// Lifecycle (init, run_tick, shutdown) works correctly with all three bus types.
#[test]
fn full_lifecycle_with_each_transport() {
    for (label, bus) in [
        ("inprocess", Arc::new(InProcessBus::new("ip")) as Arc<dyn Bus>),
        ("async", Arc::new(AsyncBus::new("ab")) as Arc<dyn Bus>),
        ("network", Arc::new(NetworkBus::new("nb")) as Arc<dyn Bus>),
    ] {
        let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
            Box::new(Counter::new("a")),
            Box::new(Counter::new("b")),
        ];
        let mut compositor = Compositor::new(partitions, bus);

        assert_eq!(compositor.state(), ExecutionState::Uninitialized,
            "{}: should start Uninitialized", label);

        compositor.init().unwrap();
        assert_eq!(compositor.state(), ExecutionState::Running,
            "{}: should be Running after init", label);

        for _ in 0..3 {
            compositor.run_tick(1.0).unwrap();
        }
        assert_eq!(compositor.tick_count(), 3,
            "{}: should have 3 ticks", label);

        compositor.shutdown().unwrap();
        assert_eq!(compositor.state(), ExecutionState::Terminated,
            "{}: should be Terminated after shutdown", label);
    }
}

/// Dump/load round-trip works with all transport types.
#[test]
fn dump_load_round_trip_with_each_transport() {
    for (label, bus1, bus2) in [
        ("inprocess",
         Arc::new(InProcessBus::new("b1")) as Arc<dyn Bus>,
         Arc::new(InProcessBus::new("b2")) as Arc<dyn Bus>),
        ("async",
         Arc::new(AsyncBus::new("b1")) as Arc<dyn Bus>,
         Arc::new(AsyncBus::new("b2")) as Arc<dyn Bus>),
        ("network",
         Arc::new(NetworkBus::new("b1")) as Arc<dyn Bus>,
         Arc::new(NetworkBus::new("b2")) as Arc<dyn Bus>),
    ] {
        // Run compositor 1 for 5 ticks and dump
        let partitions1: Vec<Box<dyn fpa_contract::Partition>> = vec![
            Box::new(Counter::new("counter")),
        ];
        let mut comp1 = Compositor::new(partitions1, bus1);
        comp1.init().unwrap();
        for _ in 0..5 {
            comp1.run_tick(1.0).unwrap();
        }
        let snapshot = comp1.dump().unwrap();

        // Load into compositor 2 and verify round-trip
        let partitions2: Vec<Box<dyn fpa_contract::Partition>> = vec![
            Box::new(Counter::new("counter")),
        ];
        let mut comp2 = Compositor::new(partitions2, bus2);
        comp2.init().unwrap();
        comp2.pause().unwrap();
        comp2.load(snapshot.clone()).unwrap();
        comp2.resume().unwrap();

        let snapshot2 = comp2.dump().unwrap();
        assert_eq!(snapshot, snapshot2,
            "{}: dump/load round-trip should preserve state", label);
    }
}
