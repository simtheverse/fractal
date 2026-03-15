//! Tests for FPA-023: Bus-mediated dump/load requests.
//!
//! Verifies that partitions can publish DumpRequest/LoadRequest on the bus
//! and the compositor processes them during Phase 1 of run_tick.

use std::sync::Arc;

use fpa_bus::{BusExt, InProcessBus};
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::Counter;
use fpa_contract::{DumpRequest, LoadRequest, Partition, StateContribution};

/// Publishing DumpRequest on bus → dump result available after tick.
#[test]
fn bus_mediated_dump_request() {
    let bus: Arc<dyn fpa_bus::Bus> = Arc::new(InProcessBus::new("test-bus"));
    let partitions: Vec<Box<dyn Partition>> = vec![Box::new(Counter::new("a"))];
    let mut compositor = Compositor::new(partitions, Arc::clone(&bus));

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    // Publish dump request on bus (simulating a partition)
    bus.publish(DumpRequest);

    // Next tick processes the dump request in Phase 1
    compositor.run_tick(1.0).unwrap();

    let dump = compositor.take_dump_result();
    assert!(dump.is_some(), "dump result should be available after bus-mediated request");

    let dump = dump.unwrap();
    let partitions_table = dump.get("partitions").unwrap().as_table().unwrap();
    let a_sc = StateContribution::from_toml(partitions_table.get("a").unwrap()).unwrap();
    let count = a_sc.state.get("count").unwrap().as_integer().unwrap();
    // Dump happens in Phase 1 of tick 2, before stepping — so it captures
    // the partition's internal state after 1 completed step (count=1).
    // Then tick 2's stepping advances count to 2, but the dump already ran.
    assert_eq!(count, 1, "dump should capture partition state at Phase 1 (before tick 2 stepping)");
}

/// Publishing LoadRequest on bus → state fragment applied.
#[test]
fn bus_mediated_load_request() {
    let bus: Arc<dyn fpa_bus::Bus> = Arc::new(InProcessBus::new("test-bus"));
    let partitions: Vec<Box<dyn Partition>> = vec![Box::new(Counter::new("counter"))];
    let mut compositor = Compositor::new(partitions, Arc::clone(&bus));

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    // Publish load request on bus with a state fragment
    let fragment: toml::Value = toml::from_str(
        r#"
        [system]
        tick_count = 42

        [partitions.counter]
        fresh = true
        age_ms = 0
        [partitions.counter.state]
        count = 99
        "#,
    )
    .unwrap();

    bus.publish(LoadRequest { fragment });

    // Next tick processes the load request in Phase 1
    compositor.run_tick(1.0).unwrap();

    assert_eq!(
        compositor.tick_count(),
        // tick_count was set to 42 by load, then incremented by run_tick
        43,
        "tick count should reflect loaded value + 1 tick"
    );

    // Verify the partition state was loaded and then stepped once more
    let dump = compositor.dump().unwrap();
    let counter_sc = StateContribution::from_toml(
        dump.get("partitions").unwrap().get("counter").unwrap(),
    )
    .unwrap();
    let count = counter_sc.state.get("count").unwrap().as_integer().unwrap();
    // Counter was loaded with count=99, then stepped once (count=100)
    assert_eq!(count, 100, "counter should be loaded value + 1 step");
}
