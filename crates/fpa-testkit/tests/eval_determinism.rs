//! Phase 6 Track Q — Determinism & Transport evaluation.
//!
//! 6Q.1: Transport mode comparison (InProcess, Async, Network)
//! 6Q.2: Tick-lifecycle determinism (ordering independence, strategy comparison)
//! 6Q.3: Cross-strategy composition (LS/LS, LS/SV, SV/LS, SV/SV, freshness)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fpa_bus::{AsyncBus, InProcessBus, NetworkBus};
use fpa_compositor::compositor::Compositor;
use fpa_compositor::supervisory::{FreshnessEntry, SupervisoryCompositor};
use fpa_contract::test_support::Counter;
use fpa_contract::{Partition, StateContribution};

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
                        .map_or(false, |vb| states_equal_within_tolerance(va, vb, tol))
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

// ---------------------------------------------------------------------------
// Helper: build a lock-step compositor with counters in a given order
// ---------------------------------------------------------------------------

fn make_compositor(ids: &[&str]) -> Compositor {
    let partitions: Vec<Box<dyn Partition>> = ids
        .iter()
        .map(|id| Box::new(Counter::new(*id)) as Box<dyn Partition>)
        .collect();
    Compositor::new(partitions, Arc::new(InProcessBus::new("test")))
}

fn run_and_dump(ids: &[&str], ticks: u64, dt: f64) -> toml::Value {
    let mut c = make_compositor(ids);
    c.init().unwrap();
    for _ in 0..ticks {
        c.run_tick(dt).unwrap();
    }
    let state = c.dump().unwrap();
    c.shutdown().unwrap();
    state
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

// ===========================================================================
// 6Q.2 — Tick-lifecycle determinism
// ===========================================================================

/// 1000 ticks with 10 different partition orderings all produce identical state.
/// Counter partitions are order-independent: each steps its own count.
#[test]
fn determinism_1000_ticks_10_orderings() {
    let orderings: Vec<Vec<&str>> = vec![
        vec!["a", "b", "c"],
        vec!["a", "c", "b"],
        vec!["b", "a", "c"],
        vec!["b", "c", "a"],
        vec!["c", "a", "b"],
        vec!["c", "b", "a"],
        // Repeat some for 10 total
        vec!["a", "b", "c"],
        vec!["c", "a", "b"],
        vec!["b", "c", "a"],
        vec!["a", "c", "b"],
    ];

    let reference = run_and_dump(&orderings[0], 1000, 1.0);

    for (i, ordering) in orderings.iter().enumerate().skip(1) {
        let state = run_and_dump(ordering, 1000, 1.0);
        assert_eq!(
            reference, state,
            "Ordering {} ({:?}) produced different state than ordering 0",
            i, ordering
        );
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
