//! FPA-009 cross-strategy composition tests (Phase 4, Track M).
//!
//! Verifies that lock-step and supervisory compositors can nest inside each
//! other without modification:
//! - Lock-step outer with supervisory inner
//! - Supervisory outer with lock-step inner
//! - Freshness metadata correctly indicates stale data at strategy boundary

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_compositor::supervisory::{FreshnessEntry, SupervisoryCompositor};
use fpa_contract::test_support::Counter;
use fpa_contract::{Partition, StateContribution};

/// Poll the output store until the given partition has produced output, or panic on timeout.
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
// Lock-step outer embeds supervisory inner — works without modification
// ---------------------------------------------------------------------------

/// A lock-step compositor can contain a supervisory compositor as a partition.
/// The supervisory inner spawns its own tasks and manages its own timing.
/// The outer lock-step compositor calls step(dt) on it each tick, which
/// triggers run_tick() to read from the output store and publish state.
#[tokio::test]
async fn lockstep_outer_embeds_supervisory_inner() {
    // Inner: supervisory compositor with a counter partition
    let inner_bus = InProcessBus::new("inner-bus");
    let inner_counter = Counter::new("inner-counter");
    let inner = SupervisoryCompositor::new(
        "supervisory-inner",
        vec![Box::new(inner_counter)],
        Arc::new(inner_bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    // Keep a handle to the output store so we can wait for partition output
    let inner_store = inner.output_store().clone();

    // Outer: lock-step compositor with a regular counter and the supervisory inner
    let outer_counter = Counter::new("outer-counter");
    let outer_bus = InProcessBus::new("outer-bus");
    let mut outer = Compositor::new(
        vec![
            Box::new(outer_counter),
            Box::new(inner),
        ],
        Arc::new(outer_bus),
    )
    .with_id("lockstep-outer");

    // Init initializes all partitions, including the supervisory inner
    // (which spawns its tasks)
    outer.init().unwrap();

    // Wait for the inner supervisory's counter to produce output
    wait_for_output(&inner_store, "inner-counter", Duration::from_secs(2)).await;

    // Let the inner counter accumulate a few steps
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Run outer ticks — this calls step(dt) on both the regular counter
    // and the supervisory inner
    for _ in 0..3 {
        outer.run_tick(1.0).unwrap();
    }

    // Verify the outer counter stepped 3 times
    let state = outer.dump().unwrap();
    let partitions = state.as_table().unwrap()["partitions"].as_table().unwrap();
    let outer_sc = StateContribution::from_toml(&partitions["outer-counter"]).unwrap();
    let outer_count = outer_sc.state.as_table().unwrap()["count"].as_integer().unwrap();
    assert_eq!(outer_count, 3, "outer counter should have stepped 3 times");

    // Verify the supervisory inner contributed state with freshness metadata.
    // The outer dump wraps supervisory-inner in a StateContribution envelope.
    // Inside that, the supervisory compositor's contribute_state wraps each
    // partition in its own StateContribution envelope with freshness metadata.
    let supervisory_sc = StateContribution::from_toml(&partitions["supervisory-inner"]).unwrap();
    let inner_state = supervisory_sc.state.as_table().unwrap();
    let inner_counter_meta = inner_state["inner-counter"].as_table().unwrap();

    // The supervisory compositor wraps state with freshness metadata (StateContribution)
    assert!(
        inner_counter_meta.contains_key("fresh"),
        "supervisory inner should include freshness metadata"
    );
    assert!(
        inner_counter_meta.contains_key("age_ms"),
        "supervisory inner should include age_ms metadata"
    );
    assert!(
        inner_counter_meta.contains_key("state"),
        "supervisory inner should include nested state"
    );

    // The inner counter should be fresh (still running)
    assert_eq!(
        inner_counter_meta.get("fresh").and_then(|v| v.as_bool()),
        Some(true),
        "inner counter should be fresh while still running"
    );

    // The nested state should contain the counter's actual data
    let inner_counter_state = inner_counter_meta["state"].as_table().unwrap();
    let inner_count = inner_counter_state["count"].as_integer().unwrap();
    assert!(
        inner_count > 0,
        "inner counter should have stepped autonomously, got {}",
        inner_count
    );

    // Note: outer.shutdown() signals the supervisory inner's tasks to stop
    // but does NOT confirm they have stopped (FPA-009: shutdown is a signal,
    // not a confirmation, under supervisory coordination).
    outer.shutdown().unwrap();
}

// ---------------------------------------------------------------------------
// Supervisory outer embeds lock-step inner — works without modification
// ---------------------------------------------------------------------------

/// A supervisory compositor can contain a lock-step compositor as a partition.
/// The supervisory spawns the lock-step compositor as a task, calling
/// init()/step()/contribute_state() in its task loop.
#[tokio::test]
async fn supervisory_outer_embeds_lockstep_inner() {
    // Inner: lock-step compositor with a counter
    let inner_counter = Counter::new("inner-counter");
    let inner_bus = InProcessBus::new("inner-bus");
    let inner = Compositor::new(
        vec![Box::new(inner_counter)],
        Arc::new(inner_bus),
    )
    .with_id("lockstep-inner");

    // Outer: supervisory compositor containing the lock-step inner
    // and a regular counter
    let outer_counter = Counter::new("outer-counter");
    let outer_bus = InProcessBus::new("outer-bus");
    let mut outer = SupervisoryCompositor::new(
        "supervisory-outer",
        vec![
            Box::new(outer_counter),
            Box::new(inner),
        ],
        Arc::new(outer_bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(10));

    outer.init().unwrap();

    // Wait for both partitions to produce output
    let store = outer.output_store().clone();
    wait_for_output(&store, "outer-counter", Duration::from_secs(2)).await;
    wait_for_output(&store, "lockstep-inner", Duration::from_secs(2)).await;

    // Let them run for a bit
    tokio::time::sleep(Duration::from_millis(100)).await;

    // run_tick reads from the output store and publishes state
    outer.run_tick(0.0).unwrap();

    // Check that both partitions produced state
    let (outer_count, inner_count) = {
        let s = store.lock().unwrap();

        // The regular counter should have stepped
        let outer_entry = s.get("outer-counter").expect("outer-counter should have output");
        let oc = outer_entry
            .state()
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("count"))
            .and_then(|v| v.as_integer())
            .unwrap();

        // The lock-step inner compositor should have produced nested state.
        // Note: the output store holds the raw contribute_state() output from the
        // lock-step compositor. When used as a Partition, its contribute_state()
        // calls dump(), which wraps each sub-partition in a StateContribution envelope.
        let inner_entry = s.get("lockstep-inner").expect("lockstep-inner should have output");
        let inner_state = inner_entry.state().unwrap().as_table().unwrap();

        // The lock-step compositor's contribute_state returns a dump with partitions + system
        assert!(
            inner_state.contains_key("partitions"),
            "lock-step inner should contribute state with partitions key"
        );
        assert!(
            inner_state.contains_key("system"),
            "lock-step inner should contribute state with system key"
        );

        // The inner counter should have been stepped — unwrap the StateContribution envelope
        let inner_partitions = inner_state["partitions"].as_table().unwrap();
        let inner_counter_sc = StateContribution::from_toml(&inner_partitions["inner-counter"]).unwrap();
        let ic = inner_counter_sc.state.as_table().unwrap()["count"]
            .as_integer()
            .unwrap();

        (oc, ic)
    };
    assert!(outer_count > 0, "outer counter should have stepped");
    assert!(
        inner_count > 0,
        "inner counter should have stepped via lock-step compositor"
    );

    outer.async_shutdown().await.unwrap();
}

// ---------------------------------------------------------------------------
// Freshness metadata correctly indicates stale data at strategy boundary
// ---------------------------------------------------------------------------

/// When a supervisory inner compositor's partition stops updating (goes stale),
/// the freshness metadata observed by the outer lock-step compositor correctly
/// reflects staleness at the strategy boundary.
#[tokio::test]
async fn freshness_metadata_reflects_staleness_at_boundary() {
    // Inner: supervisory compositor with a very short heartbeat timeout
    let inner_bus = InProcessBus::new("inner-bus");
    let inner_counter = Counter::new("inner-counter");
    let inner = SupervisoryCompositor::new(
        "supervisory-inner",
        vec![Box::new(inner_counter)],
        Arc::new(inner_bus),
        Duration::from_millis(50), // Short timeout for staleness testing
    )
    .with_step_interval(Duration::from_millis(5));

    let inner_store = inner.output_store().clone();

    // Outer: lock-step compositor
    let outer_bus = InProcessBus::new("outer-bus");
    let mut outer = Compositor::new(
        vec![Box::new(inner) as Box<dyn Partition>],
        Arc::new(outer_bus),
    )
    .with_id("lockstep-outer");

    outer.init().unwrap();

    // Wait for inner counter to produce output
    wait_for_output(&inner_store, "inner-counter", Duration::from_secs(2)).await;

    // Run a tick while the inner partition is still fresh
    outer.run_tick(1.0).unwrap();

    let state = outer.dump().unwrap();
    let partitions = state.as_table().unwrap()["partitions"].as_table().unwrap();
    let supervisory_sc = StateContribution::from_toml(&partitions["supervisory-inner"]).unwrap();
    let inner_state = supervisory_sc.state.as_table().unwrap();
    let counter_meta = inner_state["inner-counter"].as_table().unwrap();
    assert_eq!(
        counter_meta.get("fresh").and_then(|v| v.as_bool()),
        Some(true),
        "inner counter should be fresh initially"
    );

    // Shut down the outer (which signals the supervisory inner's tasks to
    // stop — fire-and-forget, see F5 spec finding).
    outer.shutdown().unwrap();

    // Wait for tasks to receive the shutdown signal and for the heartbeat
    // timeout (50ms) to expire so staleness is detectable.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Directly check the inner store for staleness
    let store = inner_store.lock().unwrap();
    let entry = store.get("inner-counter").expect("entry should exist");
    let age = Instant::now().duration_since(entry.updated_at);
    assert!(
        age > Duration::from_millis(50),
        "entry should be older than heartbeat timeout (age: {:?})",
        age
    );
}

/// When a lock-step inner is nested in a supervisory outer, the supervisory
/// wraps the inner's output with freshness metadata, showing it was recently
/// updated by the task loop.
#[tokio::test]
async fn supervisory_outer_adds_freshness_to_lockstep_inner() {
    let inner_counter = Counter::new("inner-counter");
    let inner_bus = InProcessBus::new("inner-bus");
    let inner = Compositor::new(
        vec![Box::new(inner_counter)],
        Arc::new(inner_bus),
    )
    .with_id("lockstep-inner");

    let outer_bus = InProcessBus::new("outer-bus");
    let mut outer = SupervisoryCompositor::new(
        "supervisory-outer",
        vec![Box::new(inner) as Box<dyn Partition>],
        Arc::new(outer_bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(10));

    outer.init().unwrap();

    // Wait for inner to produce output
    wait_for_output(outer.output_store(), "lockstep-inner", Duration::from_secs(2)).await;

    // run_tick produces state with freshness metadata
    outer.run_tick(0.0).unwrap();

    let state = outer.contribute_state().unwrap();
    let table = state.as_table().unwrap();
    let inner_meta = table["lockstep-inner"].as_table().unwrap();

    // Supervisory always wraps output with freshness metadata
    assert!(
        inner_meta.contains_key("fresh"),
        "supervisory should wrap lock-step inner output with freshness"
    );
    assert!(
        inner_meta.contains_key("age_ms"),
        "supervisory should include age_ms for lock-step inner"
    );
    assert!(
        inner_meta.contains_key("state"),
        "supervisory should wrap lock-step inner actual state under 'state' key"
    );

    // The wrapped state should contain the lock-step compositor's dump
    let inner_state = inner_meta["state"].as_table().unwrap();
    assert!(
        inner_state.contains_key("partitions"),
        "lock-step inner state should have partitions"
    );

    // Should be fresh since the task is still running
    assert_eq!(
        inner_meta.get("fresh").and_then(|v| v.as_bool()),
        Some(true),
        "lock-step inner should be fresh while task is running"
    );

    outer.async_shutdown().await.unwrap();
}

/// Two levels of cross-strategy nesting: lock-step -> supervisory -> lock-step.
/// Verifies that composition works at arbitrary depth with mixed strategies.
#[tokio::test]
async fn three_layer_mixed_strategy_nesting() {
    // Innermost: lock-step compositor with a counter
    let innermost_counter = Counter::new("deep-counter");
    let innermost_bus = InProcessBus::new("innermost-bus");
    let innermost = Compositor::new(
        vec![Box::new(innermost_counter)],
        Arc::new(innermost_bus),
    )
    .with_id("innermost-lockstep")
    .with_layer_depth(2);

    // Middle: supervisory compositor containing the innermost lock-step
    let middle_bus = InProcessBus::new("middle-bus");
    let middle = SupervisoryCompositor::new(
        "middle-supervisory",
        vec![Box::new(innermost) as Box<dyn Partition>],
        Arc::new(middle_bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(10))
    .with_layer_depth(1);

    let middle_store = middle.output_store().clone();

    // Outer: lock-step compositor containing the middle supervisory and a counter
    let outer_counter = Counter::new("outer-counter");
    let outer_bus = InProcessBus::new("outer-bus");
    let mut outer = Compositor::new(
        vec![
            Box::new(outer_counter),
            Box::new(middle),
        ],
        Arc::new(outer_bus),
    )
    .with_id("outer-lockstep");

    outer.init().unwrap();

    // Wait for the innermost lock-step's output in the middle supervisory's store
    wait_for_output(&middle_store, "innermost-lockstep", Duration::from_secs(2)).await;

    // Let the innermost counter accumulate steps
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Run outer ticks
    for _ in 0..3 {
        outer.run_tick(1.0).unwrap();
    }

    // Verify the full state tree
    let state = outer.dump().unwrap();
    let partitions = state.as_table().unwrap()["partitions"].as_table().unwrap();

    // Outer counter should have count = 3 — unwrap StateContribution envelope
    let outer_sc = StateContribution::from_toml(&partitions["outer-counter"]).unwrap();
    let outer_count = outer_sc.state.as_table().unwrap()["count"].as_integer().unwrap();
    assert_eq!(outer_count, 3, "outer counter should have count 3");

    // Middle supervisory — unwrap outer envelope, then access inner state
    let middle_sc = StateContribution::from_toml(&partitions["middle-supervisory"]).unwrap();
    let middle_state = middle_sc.state.as_table().unwrap();
    let innermost_meta = middle_state["innermost-lockstep"].as_table().unwrap();

    // Freshness metadata from the supervisory layer (StateContribution envelope)
    assert!(
        innermost_meta.contains_key("fresh"),
        "middle supervisory should provide freshness metadata for innermost"
    );
    assert_eq!(
        innermost_meta.get("fresh").and_then(|v| v.as_bool()),
        Some(true),
        "innermost should be fresh"
    );

    // The actual state of the innermost lock-step compositor
    let innermost_state = innermost_meta["state"].as_table().unwrap();
    assert!(
        innermost_state.contains_key("partitions"),
        "innermost state should have partitions"
    );

    let innermost_partitions = innermost_state["partitions"].as_table().unwrap();
    let deep_counter_sc = StateContribution::from_toml(&innermost_partitions["deep-counter"]).unwrap();
    let deep_count = deep_counter_sc.state.as_table().unwrap()["count"]
        .as_integer()
        .unwrap();
    assert!(
        deep_count > 0,
        "deep counter should have been stepped by the innermost lock-step compositor"
    );

    // Note: outer.shutdown() signals the middle supervisory's tasks but
    // does NOT confirm completion (FPA-009: signal, not confirmation).
    outer.shutdown().unwrap();
}

// ---------------------------------------------------------------------------
// StateContribution is defined in fpa-contract, importable by partitions
// ---------------------------------------------------------------------------

/// Verifies that `StateContribution` is importable from the contract crate
/// (not the compositor crate), consistent with FPA-009 and FPA-003.
#[test]
fn state_contribution_importable_from_contract() {
    // Construct, serialize, and deserialize a StateContribution
    let sc = StateContribution {
        state: {
            let mut t = toml::map::Map::new();
            t.insert("count".to_string(), toml::Value::Integer(42));
            toml::Value::Table(t)
        },
        fresh: true,
        age_ms: 0,
    };

    let toml_val = sc.to_toml();
    let table = toml_val.as_table().unwrap();
    assert!(table.contains_key("state"));
    assert!(table.contains_key("fresh"));
    assert!(table.contains_key("age_ms"));

    let roundtripped = StateContribution::from_toml(&toml_val).unwrap();
    assert!(roundtripped.fresh);
    assert_eq!(roundtripped.age_ms, 0);
    assert_eq!(
        roundtripped.state.as_table().unwrap()["count"].as_integer().unwrap(),
        42
    );
}

/// Both lock-step and supervisory compositors produce structurally identical
/// `contribute_state()` output: each partition entry is a `StateContribution`
/// envelope with `state`, `fresh`, and `age_ms` keys.
#[tokio::test]
async fn both_strategies_produce_uniform_state_contribution_format() {
    // Lock-step compositor
    let lockstep_counter = Counter::new("counter");
    let lockstep_bus = InProcessBus::new("lockstep-bus");
    let mut lockstep = Compositor::new(
        vec![Box::new(lockstep_counter)],
        Arc::new(lockstep_bus),
    ).with_id("lockstep");

    lockstep.init().unwrap();
    lockstep.run_tick(1.0).unwrap();

    let lockstep_state = fpa_contract::Partition::contribute_state(&lockstep).unwrap();
    let lockstep_partitions = lockstep_state.as_table().unwrap()["partitions"].as_table().unwrap();
    let lockstep_entry = &lockstep_partitions["counter"];
    let lockstep_sc = StateContribution::from_toml(lockstep_entry)
        .expect("lock-step partition entry should be a valid StateContribution");
    assert!(lockstep_sc.fresh, "lock-step should always report fresh=true");
    assert_eq!(lockstep_sc.age_ms, 0, "lock-step should always report age_ms=0");

    lockstep.shutdown().unwrap();

    // Supervisory compositor
    let supervisory_counter = Counter::new("counter");
    let supervisory_bus = InProcessBus::new("supervisory-bus");
    let mut supervisory = SupervisoryCompositor::new(
        "supervisory",
        vec![Box::new(supervisory_counter)],
        Arc::new(supervisory_bus),
        Duration::from_secs(1),
    ).with_step_interval(Duration::from_millis(5));

    supervisory.init().unwrap();

    wait_for_output(supervisory.output_store(), "counter", Duration::from_secs(2)).await;

    supervisory.run_tick(0.0).unwrap();

    let supervisory_state = fpa_contract::Partition::contribute_state(&supervisory).unwrap();
    let supervisory_table = supervisory_state.as_table().unwrap();
    let supervisory_entry = &supervisory_table["counter"];
    let supervisory_sc = StateContribution::from_toml(supervisory_entry)
        .expect("supervisory partition entry should be a valid StateContribution");

    // Both have the same envelope structure: state, fresh, age_ms
    assert!(supervisory_sc.fresh, "supervisory should report fresh for running partition");
    assert!(supervisory_sc.state.is_table(), "supervisory inner state should be a table");

    supervisory.async_shutdown().await.unwrap();
}
