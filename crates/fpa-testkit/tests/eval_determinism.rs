//! Phase 6 Track Q — Determinism & Transport evaluation.
//!
//! 6Q.1: Transport mode comparison (InProcess, Async, Network)
//! 6Q.2: Tick-lifecycle determinism (ordering independence, strategy comparison)
//! 6Q.3: Cross-strategy composition (LS/LS, LS/SV, SV/LS, SV/SV, freshness)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fpa_bus::{AsyncBus, Bus, DeferredBus, InProcessBus, NetworkBus};
use fpa_compositor::compositor::Compositor;
use fpa_compositor::supervisory::{FreshnessEntry, SupervisoryCompositor};
use fpa_contract::test_support::{Counter, SensorReading, TestCommand};
use fpa_contract::{Partition, StateContribution};

use fpa_testkit::test_partitions::{Follower, Recorder, Sensor};

// ---------------------------------------------------------------------------
// Helper: tolerance-based TOML comparison
// ---------------------------------------------------------------------------

fn states_equal_within_tolerance(a: &toml::Value, b: &toml::Value, tol: f64) -> bool {
    match (a, b) {
        (toml::Value::Table(ta), toml::Value::Table(tb)) => {
            ta.keys().all(|k| tb.contains_key(k))
                && tb.keys().all(|k| ta.contains_key(k))
                && ta.iter().all(|(k, va)| {
                    tb.get(k)
                        .is_some_and(|vb| states_equal_within_tolerance(va, vb, tol))
                })
        }
        (toml::Value::Float(fa), toml::Value::Float(fb)) => (fa - fb).abs() <= tol,
        (toml::Value::Array(aa), toml::Value::Array(ab)) => {
            aa.len() == ab.len()
                && aa
                    .iter()
                    .zip(ab.iter())
                    .all(|(va, vb)| states_equal_within_tolerance(va, vb, tol))
        }
        _ => a == b,
    }
}

// ---------------------------------------------------------------------------
// Helper: wait for supervisory partition output
// ---------------------------------------------------------------------------

async fn wait_for_output(
    store: &Arc<Mutex<HashMap<String, FreshnessEntry>>>,
    id: &str,
    timeout: Duration,
) {
    let deadline = Instant::now() + timeout;
    loop {
        {
            let s = store.lock().unwrap();
            if s.contains_key(id) {
                return;
            }
        }
        if Instant::now() > deadline {
            panic!("timed out waiting for partition '{}' output", id);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

// ===========================================================================
// 6Q.1 — Transport mode comparison
// ===========================================================================

/// InProcessBus and AsyncBus produce identical states for non-bus-communicating
/// Counter partitions over 100 ticks.
#[test]
fn system_identical_across_inprocess_and_async() {
    let ids = ["c1", "c2", "c3"];
    let ticks = 100;
    let dt = 1.0;
    let tol = 1e-12;

    let state_ip = {
        let partitions: Vec<Box<dyn Partition>> = ids
            .iter()
            .map(|id| Box::new(Counter::new(*id)) as Box<dyn Partition>)
            .collect();
        let mut c = Compositor::new(partitions, Arc::new(InProcessBus::new("ip")));
        c.init().unwrap();
        for _ in 0..ticks {
            c.run_tick(dt).unwrap();
        }
        let s = c.dump().unwrap();
        c.shutdown().unwrap();
        s
    };

    let state_async = {
        let partitions: Vec<Box<dyn Partition>> = ids
            .iter()
            .map(|id| Box::new(Counter::new(*id)) as Box<dyn Partition>)
            .collect();
        let mut c = Compositor::new(partitions, Arc::new(AsyncBus::new("ab")));
        c.init().unwrap();
        for _ in 0..ticks {
            c.run_tick(dt).unwrap();
        }
        let s = c.dump().unwrap();
        c.shutdown().unwrap();
        s
    };

    assert!(
        states_equal_within_tolerance(&state_ip, &state_async, tol),
        "InProcessBus and AsyncBus should produce identical state for non-bus partitions"
    );
}

/// All three transports (InProcess, Async, Network) produce identical states
/// for non-bus-communicating Counter partitions over 50 ticks.
#[test]
fn transport_comparison_all_three() {
    let ids = ["c1", "c2", "c3"];
    let ticks = 50;
    let dt = 1.0;
    let tol = 1e-12;

    let build = |bus: Arc<dyn fpa_bus::Bus>| {
        let partitions: Vec<Box<dyn Partition>> = ids
            .iter()
            .map(|id| Box::new(Counter::new(*id)) as Box<dyn Partition>)
            .collect();
        let mut c = Compositor::new(partitions, bus);
        c.init().unwrap();
        for _ in 0..ticks {
            c.run_tick(dt).unwrap();
        }
        let s = c.dump().unwrap();
        c.shutdown().unwrap();
        s
    };

    let state_ip = build(Arc::new(InProcessBus::new("ip")));
    let state_async = build(Arc::new(AsyncBus::new("ab")));
    let state_net = build(Arc::new(NetworkBus::new("nb")));

    assert!(
        states_equal_within_tolerance(&state_ip, &state_async, tol),
        "InProcess vs Async mismatch"
    );
    assert!(
        states_equal_within_tolerance(&state_async, &state_net, tol),
        "Async vs Network mismatch"
    );
}

/// All three transports produce identical state for bus-communicating
/// Sensor/Follower/Recorder partitions with DeferredBus over 10 ticks.
///
/// Unlike the Counter-only tests above, this exercises actual bus
/// publish/subscribe across InProcess, Async, and Network transports.
#[test]
fn transport_equivalence_with_bus_communication() {
    let tol = 1e-12;
    let ticks = 10;

    fn run_bus_pipeline(bus: Arc<dyn Bus>, ticks: u64) -> toml::Value {
        let deferred = Arc::new(DeferredBus::new(bus));
        let layer_bus: Arc<dyn Bus> = deferred.clone();
        let partitions: Vec<Box<dyn Partition>> = vec![
            Box::new(Sensor::new("sensor", layer_bus.clone(), 1.5, 0.0)),
            Box::new(Follower::new("follower", layer_bus.clone(), 5.0)),
            Box::new(Recorder::new("recorder", layer_bus.clone())),
        ];
        let mut compositor = Compositor::from_deferred_bus(partitions, deferred);
        compositor.init().unwrap();
        for _ in 0..ticks {
            compositor.run_tick(1.0).unwrap();
        }
        let state = compositor.dump().unwrap();
        compositor.shutdown().unwrap();
        state
    }

    fn make_network_bus(id: &str) -> NetworkBus {
        let bus = NetworkBus::new(id).with_framework_codecs();
        bus.register_codec::<SensorReading>();
        bus.register_codec::<TestCommand>();
        bus
    }

    let state_ip = run_bus_pipeline(Arc::new(InProcessBus::new("ip")), ticks);
    let state_async = run_bus_pipeline(Arc::new(AsyncBus::new("ab")), ticks);
    let state_net = run_bus_pipeline(Arc::new(make_network_bus("nb")), ticks);

    assert!(
        states_equal_within_tolerance(&state_ip, &state_async, tol),
        "InProcess vs Async mismatch with bus-communicating partitions"
    );
    assert!(
        states_equal_within_tolerance(&state_async, &state_net, tol),
        "Async vs Network mismatch with bus-communicating partitions"
    );
}

// ===========================================================================
// 6Q.2 — Tick-lifecycle determinism
// ===========================================================================

/// All 6 permutations of bus-communicating [sensor, follower, recorder]
/// produce identical partition state over 100 ticks with DeferredBus.
///
/// This is a stronger ordering-independence test than the Counter-only case
/// because partitions actively publish and subscribe through the bus.
/// DeferredBus ensures intra-tick isolation (FPA-014), making results
/// identical regardless of stepping order.
#[test]
fn ordering_independence_bus_communicating_6_permutations() {
    let orders: [(usize, usize, usize); 6] = [
        (0, 1, 2), // sensor, follower, recorder
        (0, 2, 1), // sensor, recorder, follower
        (1, 0, 2), // follower, sensor, recorder
        (1, 2, 0), // follower, recorder, sensor
        (2, 0, 1), // recorder, sensor, follower
        (2, 1, 0), // recorder, follower, sensor
    ];
    let names = ["sensor", "follower", "recorder"];

    let mut reference_state: Option<toml::Value> = None;

    for (pi, &(a, b, c)) in orders.iter().enumerate() {
        let inner = Arc::new(InProcessBus::new("test"));
        let deferred = Arc::new(DeferredBus::new(inner));
        let bus: Arc<dyn Bus> = deferred.clone();

        let make = |idx: usize| -> Box<dyn Partition> {
            match idx {
                0 => Box::new(Sensor::new("sensor", bus.clone(), 1.5, 0.0)),
                1 => Box::new(Follower::new("follower", bus.clone(), 5.0)),
                2 => Box::new(Recorder::new("recorder", bus.clone())),
                _ => unreachable!(),
            }
        };
        let partitions: Vec<Box<dyn Partition>> = vec![make(a), make(b), make(c)];
        let mut compositor = Compositor::from_deferred_bus(partitions, deferred);

        compositor.init().unwrap();
        for _ in 0..100 {
            compositor.run_tick(1.0).unwrap();
        }

        let state = compositor.dump().unwrap();
        compositor.shutdown().unwrap();

        let partitions_table = state.as_table().unwrap()["partitions"].as_table().unwrap();

        if let Some(ref expected) = reference_state {
            let expected_table = expected.as_table().unwrap()["partitions"].as_table().unwrap();
            for name in &names {
                let expected_sc = StateContribution::from_toml(&expected_table[*name]).unwrap();
                let actual_sc = StateContribution::from_toml(&partitions_table[*name]).unwrap();
                assert_eq!(
                    expected_sc.state, actual_sc.state,
                    "permutation {} [{}, {}, {}]: partition '{}' state differs from reference",
                    pi, names[a], names[b], names[c], name,
                );
            }
        } else {
            reference_state = Some(state);
        }
    }
}

/// Sequential (lock-step) and supervisory compositors produce structurally
/// compatible state: same keys, valid TOML tables, non-negative numerics.
/// Exact values differ because supervisory timing is non-deterministic.
#[tokio::test]
async fn sequential_vs_supervisory_comparison() {
    // Lock-step: deterministic 50 ticks
    let ls_state = {
        let partitions: Vec<Box<dyn Partition>> = vec![
            Box::new(Counter::new("p1")),
            Box::new(Counter::new("p2")),
        ];
        let mut c = Compositor::new(partitions, Arc::new(InProcessBus::new("ls")));
        c.init().unwrap();
        for _ in 0..50 {
            c.run_tick(1.0).unwrap();
        }
        let s = c.dump().unwrap();
        c.shutdown().unwrap();
        s
    };

    // Supervisory: run until partitions have output, then read state
    let sv_state = {
        let partitions: Vec<Box<dyn Partition>> = vec![
            Box::new(Counter::new("p1")),
            Box::new(Counter::new("p2")),
        ];
        let mut sv = SupervisoryCompositor::new(
            "sv",
            partitions,
            Arc::new(InProcessBus::new("sv")),
            Duration::from_secs(1),
        )
        .with_step_interval(Duration::from_millis(5));

        sv.init().unwrap();
        let store = sv.output_store().clone();
        wait_for_output(&store, "p1", Duration::from_secs(2)).await;
        wait_for_output(&store, "p2", Duration::from_secs(2)).await;

        // Let partitions accumulate some steps
        tokio::time::sleep(Duration::from_millis(100)).await;

        sv.run_tick(0.0).unwrap();
        let state = Partition::contribute_state(&sv).unwrap();
        sv.async_shutdown().await.unwrap();
        state
    };

    // Structural comparison: same top-level keys
    let ls_table = ls_state.as_table().unwrap();
    let ls_parts = ls_table["partitions"].as_table().unwrap();

    let sv_table = sv_state.as_table().unwrap();

    // Both should have p1 and p2
    assert!(ls_parts.contains_key("p1"), "lock-step should have p1");
    assert!(ls_parts.contains_key("p2"), "lock-step should have p2");
    assert!(sv_table.contains_key("p1"), "supervisory should have p1");
    assert!(sv_table.contains_key("p2"), "supervisory should have p2");

    // Supervisory wraps in StateContribution envelope; verify structure
    for key in ["p1", "p2"] {
        let sv_entry = sv_table[key].as_table().unwrap();
        assert!(
            sv_entry.contains_key("state"),
            "supervisory {} should have state key",
            key
        );
        assert!(
            sv_entry.contains_key("fresh"),
            "supervisory {} should have fresh key",
            key
        );
        assert!(
            sv_entry.contains_key("age_ms"),
            "supervisory {} should have age_ms key",
            key
        );

        // Inner state should be a valid table with non-negative count
        let inner_state = sv_entry["state"].as_table().unwrap();
        let count = inner_state["count"].as_integer().unwrap();
        assert!(count > 0, "supervisory {} count should be positive", key);
    }

    // Lock-step partitions also wrapped in StateContribution
    for key in ["p1", "p2"] {
        let sc = StateContribution::from_toml(&ls_parts[key]).unwrap();
        let count = sc.state.as_table().unwrap()["count"].as_integer().unwrap();
        assert!(count > 0, "lock-step {} count should be positive", key);
    }
}

// ===========================================================================
// 6Q.3 — Cross-strategy composition
// ===========================================================================

/// Exercise all 4 outer/inner strategy boundary combinations:
/// LS/LS, LS/SV, SV/LS, SV/SV. Each must produce valid state contributions
/// with correct nesting structure.
#[tokio::test]
async fn all_strategy_boundary_combinations() {
    // LS/LS: lock-step outer with lock-step inner
    {
        let inner = Compositor::new(
            vec![Box::new(Counter::new("inner-c"))],
            Arc::new(InProcessBus::new("inner")),
        )
        .with_id("inner");

        let mut outer = Compositor::new(
            vec![Box::new(Counter::new("outer-c")), Box::new(inner)],
            Arc::new(InProcessBus::new("outer")),
        );
        outer.init().unwrap();
        for _ in 0..10 {
            outer.run_tick(1.0).unwrap();
        }
        let state = outer.dump().unwrap();
        let parts = state.as_table().unwrap()["partitions"].as_table().unwrap();
        assert!(parts.contains_key("outer-c"), "LS/LS: outer-c present");
        assert!(parts.contains_key("inner"), "LS/LS: inner present");

        // Inner should have nested partitions structure
        let inner_sc = StateContribution::from_toml(&parts["inner"]).unwrap();
        let inner_state = inner_sc.state.as_table().unwrap();
        assert!(
            inner_state.contains_key("partitions"),
            "LS/LS: inner should have partitions key"
        );
        outer.shutdown().unwrap();
    }

    // LS/SV: lock-step outer with supervisory inner
    {
        let inner = SupervisoryCompositor::new(
            "sv-inner",
            vec![Box::new(Counter::new("inner-c"))],
            Arc::new(InProcessBus::new("inner")),
            Duration::from_secs(1),
        )
        .with_step_interval(Duration::from_millis(5));

        let inner_store = inner.output_store().clone();

        let mut outer = Compositor::new(
            vec![Box::new(Counter::new("outer-c")), Box::new(inner)],
            Arc::new(InProcessBus::new("outer")),
        );
        outer.init().unwrap();
        wait_for_output(&inner_store, "inner-c", Duration::from_secs(2)).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        for _ in 0..5 {
            outer.run_tick(1.0).unwrap();
        }

        let state = outer.dump().unwrap();
        let parts = state.as_table().unwrap()["partitions"].as_table().unwrap();
        assert!(parts.contains_key("outer-c"), "LS/SV: outer-c present");
        assert!(parts.contains_key("sv-inner"), "LS/SV: sv-inner present");

        let inner_sc = StateContribution::from_toml(&parts["sv-inner"]).unwrap();
        let inner_state = inner_sc.state.as_table().unwrap();
        assert!(
            inner_state.contains_key("inner-c"),
            "LS/SV: supervisory inner should have inner-c"
        );
        outer.shutdown().unwrap();
    }

    // SV/LS: supervisory outer with lock-step inner
    {
        let inner = Compositor::new(
            vec![Box::new(Counter::new("inner-c"))],
            Arc::new(InProcessBus::new("inner")),
        )
        .with_id("ls-inner");

        let mut outer = SupervisoryCompositor::new(
            "sv-outer",
            vec![Box::new(Counter::new("outer-c")), Box::new(inner)],
            Arc::new(InProcessBus::new("outer")),
            Duration::from_secs(1),
        )
        .with_step_interval(Duration::from_millis(5));

        outer.init().unwrap();
        let store = outer.output_store().clone();
        wait_for_output(&store, "outer-c", Duration::from_secs(2)).await;
        wait_for_output(&store, "ls-inner", Duration::from_secs(2)).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        outer.run_tick(0.0).unwrap();

        let state = Partition::contribute_state(&outer).unwrap();
        let table = state.as_table().unwrap();
        assert!(table.contains_key("outer-c"), "SV/LS: outer-c present");
        assert!(table.contains_key("ls-inner"), "SV/LS: ls-inner present");

        // ls-inner wrapped in freshness envelope
        let inner_entry = table["ls-inner"].as_table().unwrap();
        assert!(inner_entry.contains_key("fresh"), "SV/LS: freshness metadata");
        assert!(inner_entry.contains_key("state"), "SV/LS: state key");

        outer.async_shutdown().await.unwrap();
    }

    // SV/SV: supervisory outer with supervisory inner
    {
        let inner = SupervisoryCompositor::new(
            "sv-inner",
            vec![Box::new(Counter::new("inner-c"))],
            Arc::new(InProcessBus::new("inner")),
            Duration::from_secs(1),
        )
        .with_step_interval(Duration::from_millis(5));

        let mut outer = SupervisoryCompositor::new(
            "sv-outer",
            vec![Box::new(Counter::new("outer-c")), Box::new(inner)],
            Arc::new(InProcessBus::new("outer")),
            Duration::from_secs(1),
        )
        .with_step_interval(Duration::from_millis(10));

        outer.init().unwrap();
        let store = outer.output_store().clone();
        wait_for_output(&store, "outer-c", Duration::from_secs(2)).await;
        wait_for_output(&store, "sv-inner", Duration::from_secs(2)).await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        outer.run_tick(0.0).unwrap();

        let state = Partition::contribute_state(&outer).unwrap();
        let table = state.as_table().unwrap();
        assert!(table.contains_key("outer-c"), "SV/SV: outer-c present");
        assert!(table.contains_key("sv-inner"), "SV/SV: sv-inner present");

        // sv-inner wrapped in freshness envelope by the outer supervisory
        let inner_entry = table["sv-inner"].as_table().unwrap();
        assert!(inner_entry.contains_key("fresh"), "SV/SV: freshness metadata");
        assert!(inner_entry.contains_key("state"), "SV/SV: state key");

        outer.async_shutdown().await.unwrap();
    }
}

/// After running a supervisory compositor, freshness metadata should show
/// fresh == true and age_ms within a reasonable bound (< 5000ms).
#[tokio::test]
async fn freshness_metadata_accuracy() {
    let mut sv = SupervisoryCompositor::new(
        "test",
        vec![
            Box::new(Counter::new("p1")),
            Box::new(Counter::new("p2")),
        ],
        Arc::new(InProcessBus::new("bus")),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    sv.init().unwrap();
    let store = sv.output_store().clone();
    wait_for_output(&store, "p1", Duration::from_secs(2)).await;
    wait_for_output(&store, "p2", Duration::from_secs(2)).await;

    // Let partitions accumulate some steps
    tokio::time::sleep(Duration::from_millis(50)).await;

    sv.run_tick(0.0).unwrap();

    let state = Partition::contribute_state(&sv).unwrap();
    let table = state.as_table().unwrap();

    for key in ["p1", "p2"] {
        let sc = StateContribution::from_toml(&table[key]).unwrap();
        assert!(
            sc.fresh,
            "{}: should be fresh while partitions are running",
            key
        );
        assert!(
            sc.age_ms < 5000,
            "{}: age_ms should be < 5000ms, got {}",
            key,
            sc.age_ms
        );
    }

    sv.async_shutdown().await.unwrap();
}
