// FPA-033 — Composition Test: Layer 0 and Layer 1 Compositor Tests
//
// Verifies that compositors correctly assemble and coordinate partitions,
// that inter-partition state is observable through SharedContext, and that
// failure localization distinguishes wiring errors from partition errors.

use std::sync::Arc;

use fpa_bus::{BusExt, BusReader, InProcessBus};
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::{Accumulator, Counter};
use fpa_contract::{Partition, SharedContext, StateContribution};

/// Layer 0: compose Counter + Accumulator, verify inter-partition communication
/// through SharedContext published on the bus. The compositor assembles
/// partition outputs into SharedContext each tick, which is the primary
/// mechanism for inter-partition state observation.
#[test]
fn layer_0_inter_partition_communication_via_shared_context() {
    let bus = Arc::new(InProcessBus::new("layer-0"));

    // Subscribe to SharedContext before creating the compositor so we can
    // observe the compositor's published state.
    let mut ctx_reader = bus.subscribe::<SharedContext>();

    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Counter::new("counter")),
        Box::new(Accumulator::new("accumulator")),
    ];
    let mut compositor = Compositor::new(partitions, bus);

    compositor.init().unwrap();

    let dt = 1.0 / 60.0;
    for _ in 0..10 {
        compositor.run_tick(dt).unwrap();
    }

    // SharedContext is the inter-partition communication mechanism.
    // Verify that both partitions' state is observable through it.
    let ctx = ctx_reader
        .read()
        .expect("SharedContext should be published on the bus after run_tick");
    assert_eq!(ctx.tick, 10, "SharedContext tick should match compositor tick");

    let state_table = ctx.state.as_table().unwrap();

    // Counter's state is visible to other partitions via SharedContext
    let counter_envelope = StateContribution::from_toml(&state_table["counter"]).unwrap();
    let count = counter_envelope.state.as_table().unwrap()["count"]
        .as_integer()
        .unwrap();
    assert_eq!(count, 10);

    // Accumulator's state is visible to other partitions via SharedContext
    let acc_envelope = StateContribution::from_toml(&state_table["accumulator"]).unwrap();
    let total = acc_envelope.state.as_table().unwrap()["total"]
        .as_float()
        .unwrap();
    let expected = 10.0 * dt;
    assert!(
        (total - expected).abs() < 1e-12,
        "accumulator total {} should be close to {}",
        total,
        expected
    );

    compositor.shutdown().unwrap();
}

/// Layer 1: compositor-as-partition with sub-partitions, verifying fractal nesting.
#[test]
fn layer_1_compositor_as_partition() {
    // Inner compositor (layer 1)
    let inner_partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Counter::new("inner-counter")),
    ];
    let inner = Compositor::new(
        inner_partitions,
        Arc::new(InProcessBus::new("inner-bus")),
    )
    .with_id("inner")
    .with_layer_depth(1);

    // Outer compositor (layer 0)
    let outer_partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Counter::new("outer-counter")),
        Box::new(inner),
    ];
    let mut outer = Compositor::new(
        outer_partitions,
        Arc::new(InProcessBus::new("outer-bus")),
    )
    .with_id("outer");

    outer.init().unwrap();
    for _ in 0..5 {
        outer.run_tick(1.0).unwrap();
    }

    let state = outer.dump().unwrap();
    let partitions = state.as_table().unwrap()["partitions"].as_table().unwrap();

    // Outer counter: 5 steps
    let outer_sc = StateContribution::from_toml(&partitions["outer-counter"]).unwrap();
    assert_eq!(
        outer_sc.state.as_table().unwrap()["count"].as_integer().unwrap(),
        5
    );

    // Inner compositor state contains its own partitions
    let inner_sc = StateContribution::from_toml(&partitions["inner"]).unwrap();
    let inner_partitions = inner_sc.state.as_table().unwrap()["partitions"]
        .as_table()
        .unwrap();
    let inner_counter_sc =
        StateContribution::from_toml(&inner_partitions["inner-counter"]).unwrap();
    assert_eq!(
        inner_counter_sc.state.as_table().unwrap()["count"]
            .as_integer()
            .unwrap(),
        5
    );

    outer.shutdown().unwrap();
}

/// Failure localization: wiring error (unknown implementation) is distinguishable
/// from partition runtime error.
#[test]
fn failure_localization_wiring_vs_partition() {
    use fpa_testkit::registry::PartitionRegistry;

    let registry = PartitionRegistry::with_test_partitions();

    // Wiring error: unknown implementation
    let result = registry.create("NonexistentImpl", "test-partition", &toml::Value::Table(Default::default()));
    match result {
        Err(err) => assert!(
            err.message.contains("unknown implementation"),
            "wiring error should mention 'unknown implementation', got: {}",
            err.message
        ),
        Ok(_) => panic!("expected error for unknown implementation"),
    }

    // Partition runtime error: step without init
    let mut counter = Counter::new("test");
    let step_result = counter.step(1.0);
    assert!(step_result.is_err());
    let err = step_result.unwrap_err();
    assert!(
        err.message.contains("not initialized"),
        "runtime error should mention 'not initialized', got: {}",
        err.message
    );
}
