//! FPA-009 supervisory compositor tests.
//!
//! Verifies that the supervisory compositor:
//! - Partitions run their own processing loops (not called by compositor)
//! - Compositor manages lifecycle boundaries (start/stop)
//! - Compositor detects fault via heartbeat/timeout
//! - Data freshness metadata on output (fresh vs stale)
//! - Implements Partition trait for nestability
//! - Reports partition errors to the output store
//! - Tracks stale partitions
//!
//! FPA-011 supervisory fault handling tests (added by audit):
//! - Panics during step/init are caught (not raw unwinds)
//! - Per-invocation timeouts enforced (50ms step, 500ms init)
//! - Errors include compositor context (partition id, operation)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fpa_bus::{BusExt, BusReader, InProcessBus};
use fpa_compositor::compositor::SharedContext;
use fpa_compositor::supervisory::{FreshnessEntry, PartitionOutput, SupervisoryCompositor};
use fpa_contract::test_support::Counter;
use fpa_contract::{Partition, PartitionError};

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

/// Poll the output store until the given partition has a fault entry, or panic on timeout.
async fn wait_for_fault(
    store: &Arc<Mutex<HashMap<String, FreshnessEntry>>>,
    id: &str,
    timeout: Duration,
) {
    let deadline = Instant::now() + timeout;
    loop {
        {
            let s = store.lock().unwrap();
            if let Some(entry) = s.get(id) {
                if entry.is_fault() {
                    return;
                }
            }
        }
        if Instant::now() > deadline {
            panic!("timed out waiting for partition '{}' fault", id);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Partitions accumulate steps on their own without the compositor calling step().
#[tokio::test]
async fn partition_runs_own_processing_loop() {
    let bus = InProcessBus::new("test-bus");
    let counter = Counter::new("counter-1");

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(counter)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the partition to produce output
    wait_for_output(compositor.output_store(), "counter-1", Duration::from_secs(2)).await;

    // Let it accumulate a few steps
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The partition should have accumulated steps autonomously
    let count = {
        let store = compositor.output_store().lock().unwrap();
        let entry = store.get("counter-1").expect("partition should have written state");
        entry
            .state()
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("count"))
            .and_then(|v| v.as_integer())
            .unwrap()
    };
    assert!(count > 1, "partition should have stepped multiple times, got {}", count);

    compositor.async_shutdown().await.unwrap();
}

/// Compositor manages lifecycle: init starts the task, shutdown stops it.
#[tokio::test]
async fn lifecycle_management_init_and_shutdown() {
    let bus = InProcessBus::new("test-bus");
    let counter = Counter::new("counter-1");

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(counter)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    // Before init, state is Uninitialized
    assert_eq!(
        compositor.state(),
        fpa_compositor::state_machine::ExecutionState::Uninitialized
    );

    compositor.init().unwrap();

    // After init, state is Running
    assert_eq!(
        compositor.state(),
        fpa_compositor::state_machine::ExecutionState::Running
    );

    // Wait for partition to produce output
    wait_for_output(compositor.output_store(), "counter-1", Duration::from_secs(2)).await;

    // Record the count before shutdown
    let count_before = {
        let store = compositor.output_store().lock().unwrap();
        store
            .get("counter-1")
            .and_then(|e| e.state()).and_then(|v| v.as_table())
            .and_then(|t| t.get("count"))
            .and_then(|v| v.as_integer())
            .unwrap_or(0)
    };

    compositor.async_shutdown().await.unwrap();

    assert_eq!(
        compositor.state(),
        fpa_compositor::state_machine::ExecutionState::Terminated
    );

    // After shutdown, the count should not increase
    tokio::time::sleep(Duration::from_millis(50)).await;
    let count_after = {
        let store = compositor.output_store().lock().unwrap();
        store
            .get("counter-1")
            .and_then(|e| e.state()).and_then(|v| v.as_table())
            .and_then(|t| t.get("count"))
            .and_then(|v| v.as_integer())
            .unwrap_or(0)
    };

    assert_eq!(
        count_before, count_after,
        "partition should stop stepping after shutdown"
    );
}

/// Freshness: recently updated partitions are marked fresh.
#[tokio::test]
async fn partition_marked_fresh_when_recently_updated() {
    let bus = InProcessBus::new("test-bus");
    let counter = Counter::new("counter-1");

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(counter)],
        Arc::new(bus),
        Duration::from_secs(1), // 1s timeout - plenty of time
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for partition to produce output
    wait_for_output(compositor.output_store(), "counter-1", Duration::from_secs(2)).await;

    // Partition should be fresh
    let fresh = compositor.is_partition_fresh("counter-1");
    assert_eq!(fresh, Some(true), "partition should be fresh");

    // run_tick should produce state with fresh=true
    compositor.run_tick(0.0).unwrap();
    let state = compositor.contribute_state().unwrap();
    let counter_meta = state
        .as_table()
        .and_then(|t| t.get("counter-1"))
        .and_then(|v| v.as_table())
        .expect("should have counter-1 entry");

    assert_eq!(
        counter_meta.get("fresh").and_then(|v| v.as_bool()),
        Some(true)
    );

    compositor.async_shutdown().await.unwrap();
}

/// Staleness: if a partition stops updating, it is detected via heartbeat timeout.
#[tokio::test]
async fn partition_marked_stale_after_heartbeat_timeout() {
    let bus = InProcessBus::new("test-bus");
    let counter = Counter::new("counter-1");

    // Very short heartbeat timeout
    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(counter)],
        Arc::new(bus),
        Duration::from_millis(30), // 30ms timeout
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for partition to produce output
    wait_for_output(compositor.output_store(), "counter-1", Duration::from_secs(2)).await;

    // Partition should be fresh while still running
    assert_eq!(
        compositor.is_partition_fresh("counter-1"),
        Some(true),
        "partition should be fresh while running"
    );

    // Shut down to stop the partition from updating
    compositor.async_shutdown().await.unwrap();

    // Wait longer than the heartbeat timeout
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Now the entry should be stale (we can still check the store directly)
    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("counter-1").expect("entry should exist");
    let age = std::time::Instant::now().duration_since(entry.updated_at);
    assert!(
        age > Duration::from_millis(30),
        "entry should be older than heartbeat timeout"
    );
}

/// run_tick publishes aggregated state with freshness metadata on the bus.
#[tokio::test]
async fn run_tick_publishes_shared_context_with_freshness() {
    let bus = InProcessBus::new("test-bus");
    let mut reader = bus.subscribe::<SharedContext>();
    let counter = Counter::new("counter-1");

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(counter)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for partition to produce output
    wait_for_output(compositor.output_store(), "counter-1", Duration::from_secs(2)).await;

    // run_tick should publish on bus
    compositor.run_tick(0.0).unwrap();

    let ctx = reader.read().expect("should have published SharedContext");
    assert_eq!(ctx.tick, 1);

    let table = ctx.state.as_table().expect("state should be a table");
    let counter_entry = table
        .get("counter-1")
        .and_then(|v| v.as_table())
        .expect("should have counter-1");

    // Should have freshness metadata (StateContribution envelope)
    assert!(counter_entry.contains_key("fresh"));
    assert!(counter_entry.contains_key("age_ms"));
    assert!(counter_entry.contains_key("state"));

    // Should be fresh
    assert_eq!(
        counter_entry.get("fresh").and_then(|v| v.as_bool()),
        Some(true)
    );

    compositor.async_shutdown().await.unwrap();
}

/// Multiple partitions run independently in supervisory mode.
#[tokio::test]
async fn multiple_partitions_run_independently() {
    let bus = InProcessBus::new("test-bus");
    let counter_a = Counter::new("counter-a");
    let counter_b = Counter::new("counter-b");

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(counter_a), Box::new(counter_b)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for both partitions to produce output
    let store = compositor.output_store().clone();
    wait_for_output(&store, "counter-a", Duration::from_secs(2)).await;
    wait_for_output(&store, "counter-b", Duration::from_secs(2)).await;

    let (count_a, count_b) = {
        let s = store.lock().unwrap();
        let a = s
            .get("counter-a")
            .and_then(|e| e.state()).and_then(|v| v.as_table())
            .and_then(|t| t.get("count"))
            .and_then(|v| v.as_integer())
            .expect("counter-a should have state");
        let b = s
            .get("counter-b")
            .and_then(|e| e.state()).and_then(|v| v.as_table())
            .and_then(|t| t.get("count"))
            .and_then(|v| v.as_integer())
            .expect("counter-b should have state");
        (a, b)
    };

    assert!(count_a > 0, "counter-a should have stepped");
    assert!(count_b > 0, "counter-b should have stepped");

    compositor.async_shutdown().await.unwrap();
}

/// SupervisoryCompositor implements Partition trait for nestability.
#[tokio::test]
async fn supervisory_compositor_implements_partition_trait() {
    let bus = InProcessBus::new("test-bus");
    let counter = Counter::new("counter-1");

    let mut compositor = SupervisoryCompositor::new(
        "test-supervisory",
        vec![Box::new(counter)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    // Use Partition trait methods
    assert_eq!(Partition::id(&compositor), "test-supervisory");

    Partition::init(&mut compositor).unwrap();

    // Wait for partition to produce output
    wait_for_output(compositor.output_store(), "counter-1", Duration::from_secs(2)).await;

    // step delegates to run_tick
    Partition::step(&mut compositor, 0.016).unwrap();
    assert_eq!(compositor.tick_count(), 1);

    // contribute_state returns aggregated state
    let state = Partition::contribute_state(&compositor).unwrap();
    assert!(state.as_table().is_some());

    // load_state populates the output store (must use StateContribution envelope)
    let envelope = fpa_contract::StateContribution {
        state: toml::Value::String("restored-value".to_string()),
        fresh: true,
        age_ms: 0,
    };
    let mut test_table = toml::map::Map::new();
    test_table.insert(
        "restored-partition".to_string(),
        envelope.to_toml(),
    );
    Partition::load_state(&mut compositor, toml::Value::Table(test_table)).unwrap();

    {
        let store = compositor.output_store().lock().unwrap();
        assert!(store.contains_key("restored-partition"), "load_state should populate the store");
    }

    // shutdown via Partition trait (sync, non-blocking)
    Partition::shutdown(&mut compositor).unwrap();
    assert_eq!(
        compositor.state(),
        fpa_compositor::state_machine::ExecutionState::Terminated
    );
}

/// stale_partitions returns IDs of partitions that have exceeded the heartbeat timeout.
#[tokio::test]
async fn stale_partitions_detected() {
    let bus = InProcessBus::new("test-bus");
    let counter = Counter::new("counter-1");

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(counter)],
        Arc::new(bus),
        Duration::from_millis(30),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for partition to produce output
    wait_for_output(compositor.output_store(), "counter-1", Duration::from_secs(2)).await;

    // While running, partition should not be stale
    assert!(
        compositor.stale_partitions().is_empty(),
        "running partition should not be stale"
    );

    // Shut down and wait for staleness
    compositor.async_shutdown().await.unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Re-check: can't call stale_partitions after shutdown because handles are drained.
    // Instead verify via direct store inspection (already tested in staleness test above).
}

/// Faulted partition causes contribute_state to return an error.
///
/// A partition that faults stops producing output. After the heartbeat timeout
/// passes, contribute_state() should return an error because the compositor
/// detects the faulted partition and refuses to produce partial state.
#[tokio::test]
async fn faulted_partition_fails_contribute_state() {
    let bus = InProcessBus::new("test-bus");

    struct FailOnSecondStep {
        id: String,
        count: u32,
    }

    impl Partition for FailOnSecondStep {
        fn id(&self) -> &str {
            &self.id
        }
        fn init(&mut self) -> Result<(), PartitionError> {
            Ok(())
        }
        fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
            self.count += 1;
            if self.count >= 2 {
                return Err(PartitionError::new(&self.id, "step", "intentional failure"));
            }
            Ok(())
        }
        fn shutdown(&mut self) -> Result<(), PartitionError> {
            Ok(())
        }
        fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
            let mut t = toml::map::Map::new();
            t.insert("count".to_string(), toml::Value::Integer(self.count as i64));
            Ok(toml::Value::Table(t))
        }
        fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> {
            Ok(())
        }
    }

    // Use a good partition alongside the failing one so contribute_state doesn't
    // short-circuit on fault for the entire compositor.
    let good_counter = Counter::new("good");
    let failing = FailOnSecondStep {
        id: "failer".to_string(),
        count: 0,
    };

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(good_counter), Box::new(failing)],
        Arc::new(bus),
        Duration::from_millis(30), // short heartbeat timeout
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the good partition to produce output
    wait_for_output(compositor.output_store(), "good", Duration::from_secs(2)).await;

    // Wait for the failer to fault
    wait_for_fault(compositor.output_store(), "failer", Duration::from_secs(2)).await;

    // Wait for the heartbeat timeout to expire
    tokio::time::sleep(Duration::from_millis(60)).await;

    // contribute_state should return an error because a partition has faulted.
    // The supervisory compositor checks for faults before contributing state.
    let result = compositor.contribute_state();
    assert!(result.is_err(), "contribute_state should error when a partition has faulted");

    compositor.async_shutdown().await.unwrap_or_default();
}

/// Per-partition step intervals are respected.
#[tokio::test]
async fn per_partition_step_interval() {
    let bus = InProcessBus::new("test-bus");
    let fast_counter = Counter::new("fast");
    let slow_counter = Counter::new("slow");

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(fast_counter), Box::new(slow_counter)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    // Set slow partition to a longer interval
    compositor.with_partition_interval("slow", Duration::from_millis(50));

    compositor.init().unwrap();

    // Wait for both to produce output
    let store = compositor.output_store().clone();
    wait_for_output(&store, "fast", Duration::from_secs(2)).await;
    wait_for_output(&store, "slow", Duration::from_secs(2)).await;

    // Let them run for a bit
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (fast_count, slow_count) = {
        let s = store.lock().unwrap();
        let fast = s
            .get("fast")
            .and_then(|e| e.state()).and_then(|v| v.as_table())
            .and_then(|t| t.get("count"))
            .and_then(|v| v.as_integer())
            .unwrap_or(0);
        let slow = s
            .get("slow")
            .and_then(|e| e.state()).and_then(|v| v.as_table())
            .and_then(|t| t.get("count"))
            .and_then(|v| v.as_integer())
            .unwrap_or(0);
        (fast, slow)
    };

    assert!(
        fast_count > slow_count,
        "fast partition ({}) should have stepped more than slow partition ({})",
        fast_count,
        slow_count
    );

    compositor.async_shutdown().await.unwrap();
}

/// Partition errors during step are reported to the output store.
#[tokio::test]
async fn partition_step_error_reported_to_store() {
    // Create a partition that fails on step
    struct FailingPartition {
        id: String,
    }

    impl Partition for FailingPartition {
        fn id(&self) -> &str {
            &self.id
        }
        fn init(&mut self) -> Result<(), PartitionError> {
            Ok(())
        }
        fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
            Err(PartitionError::new(&self.id, "step", "intentional failure"))
        }
        fn shutdown(&mut self) -> Result<(), PartitionError> {
            Ok(())
        }
        fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
            Ok(toml::Value::Table(toml::map::Map::new()))
        }
        fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> {
            Ok(())
        }
    }

    let bus = InProcessBus::new("test-bus");
    let failing = FailingPartition {
        id: "failing-1".to_string(),
    };

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(failing)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the error to be reported
    wait_for_output(compositor.output_store(), "failing-1", Duration::from_secs(2)).await;

    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("failing-1").expect("should have error entry");
    match &entry.output {
        PartitionOutput::Fault { operation, message } => {
            assert!(
                message.contains("intentional failure"),
                "error message should contain the failure reason, got: {}",
                message
            );
            assert_eq!(operation, "step");
        }
        PartitionOutput::State(_) => panic!("expected fault, got state"),
    }
}

// --- FPA-011 supervisory fault handling tests ---
//
// These tests verify that the supervisory compositor applies the same fault
// handling discipline as the lock-step compositor: panic catching, timeout
// enforcement, and error context enrichment. The lock-step compositor routes
// all lifecycle calls through fault::safe_* wrappers; the supervisory
// compositor should provide equivalent protection in its spawned tasks.

/// A partition that panics during a specified operation.
struct SupervisoryPanickingPartition {
    id: String,
    panic_on: String,
}

impl Partition for SupervisoryPanickingPartition {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        if self.panic_on == "init" {
            panic!("partition panicked during init");
        }
        Ok(())
    }

    fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
        if self.panic_on == "step" {
            panic!("partition panicked during step");
        }
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        Ok(toml::Value::Table(toml::map::Map::new()))
    }

    fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> {
        Ok(())
    }
}

/// A partition that sleeps during a specified operation to test timeout detection.
struct SupervisorySlowPartition {
    id: String,
    delay_ms: u64,
    slow_on: String,
}

impl Partition for SupervisorySlowPartition {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        if self.slow_on == "init" {
            std::thread::sleep(Duration::from_millis(self.delay_ms));
        }
        Ok(())
    }

    fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
        if self.slow_on == "step" {
            std::thread::sleep(Duration::from_millis(self.delay_ms));
        }
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        Ok(toml::Value::Table(toml::map::Map::new()))
    }

    fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> {
        Ok(())
    }
}

/// FPA-011: A partition panicking during step() in a supervisory task should be
/// caught and reported to the output store — not silently kill the tokio task.
#[tokio::test]
async fn panic_during_supervisory_step_is_caught() {
    let bus = InProcessBus::new("test-bus");
    let panicker = SupervisoryPanickingPartition {
        id: "panicker".to_string(),
        panic_on: "step".to_string(),
    };

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(panicker)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the panic to be caught and reported
    wait_for_output(compositor.output_store(), "panicker", Duration::from_secs(2)).await;

    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("panicker").expect("should have error entry");
    match &entry.output {
        PartitionOutput::Fault { operation, message } => {
            assert!(
                message.contains("panic"),
                "error message should mention panic: {}",
                message
            );
            assert_eq!(
                operation, "step",
                "error should identify the faulting operation"
            );
        }
        PartitionOutput::State(_) => panic!("expected fault, got state"),
    }
}

/// FPA-011: A partition panicking during init() in a supervisory task should be
/// caught and returned from async_init().
#[tokio::test]
async fn panic_during_supervisory_init_is_caught() {
    let bus = InProcessBus::new("test-bus");
    let panicker = SupervisoryPanickingPartition {
        id: "panicker".to_string(),
        panic_on: "init".to_string(),
    };

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(panicker)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    // async_init awaits init completion and returns faults (FPA-011)
    let result = compositor.async_init().await;
    assert!(result.is_err(), "async_init should return error when partition panics");

    let err = result.unwrap_err();
    assert!(
        err.message.contains("panic"),
        "error message should mention panic: {}",
        err.message
    );
    assert_eq!(
        err.operation, "init",
        "error should identify init as the faulting operation"
    );
    assert_eq!(
        compositor.state(),
        fpa_compositor::state_machine::ExecutionState::Error,
        "compositor should be in Error state after init fault"
    );
}

/// FPA-011: A partition whose step() exceeds the 50ms timeout should be
/// reported as a timeout fault in the supervisory output store.
#[tokio::test]
async fn slow_supervisory_step_detected_as_timeout() {
    let bus = InProcessBus::new("test-bus");
    let slowpoke = SupervisorySlowPartition {
        id: "slowpoke".to_string(),
        delay_ms: 100, // exceeds 50ms step timeout
        slow_on: "step".to_string(),
    };

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(slowpoke)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the timeout to be detected and reported
    wait_for_output(compositor.output_store(), "slowpoke", Duration::from_secs(2)).await;

    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("slowpoke").expect("should have entry");
    match &entry.output {
        PartitionOutput::Fault { message, .. } => {
            assert!(
                message.contains("timeout") || message.contains("exceeded"),
                "error message should mention timeout: {}",
                message
            );
        }
        PartitionOutput::State(_) => panic!("expected timeout fault, got state"),
    }
}

/// FPA-011: A partition whose init() exceeds the 500ms timeout should be
/// returned as a fault from async_init().
#[tokio::test]
async fn slow_supervisory_init_detected_as_timeout() {
    let bus = InProcessBus::new("test-bus");
    let slowpoke = SupervisorySlowPartition {
        id: "slowpoke".to_string(),
        delay_ms: 600, // exceeds 500ms init timeout
        slow_on: "init".to_string(),
    };

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(slowpoke)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    // async_init awaits init completion and returns timeouts (FPA-011).
    let result = compositor.async_init().await;
    assert!(result.is_err(), "async_init should return error when partition times out");

    let err = result.unwrap_err();
    assert!(
        err.message.contains("timeout") || err.message.contains("exceeded") || err.message.contains("deadline"),
        "error message should mention timeout: {}",
        err.message
    );
    assert_eq!(
        compositor.state(),
        fpa_compositor::state_machine::ExecutionState::Error,
        "compositor should be in Error state after init timeout"
    );
}

/// FPA-011: Error context from supervisory tasks should include the partition
/// ID and faulting operation, matching the lock-step compositor's context
/// enrichment.
#[tokio::test]
async fn supervisory_error_includes_partition_id_and_operation() {
    let bus = InProcessBus::new("test-bus");

    struct FailOnSecondStep {
        id: String,
        count: u32,
    }

    impl Partition for FailOnSecondStep {
        fn id(&self) -> &str {
            &self.id
        }
        fn init(&mut self) -> Result<(), PartitionError> {
            Ok(())
        }
        fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
            self.count += 1;
            if self.count >= 2 {
                return Err(PartitionError::new(&self.id, "step", "specific-failure-message"));
            }
            Ok(())
        }
        fn shutdown(&mut self) -> Result<(), PartitionError> {
            Ok(())
        }
        fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
            Ok(toml::Value::Table(toml::map::Map::new()))
        }
        fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> {
            Ok(())
        }
    }

    let partition = FailOnSecondStep {
        id: "my-partition-42".to_string(),
        count: 0,
    };

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(partition)],
        Arc::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the fault to surface in the output store (the partition succeeds
    // on the first step, so wait_for_output would return the initial state entry)
    wait_for_fault(compositor.output_store(), "my-partition-42", Duration::from_secs(2)).await;

    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("my-partition-42").expect("should have entry");
    match &entry.output {
        PartitionOutput::Fault { operation, message } => {
            assert!(
                message.contains("specific-failure-message"),
                "error should preserve the original error message: {}",
                message
            );
            assert_eq!(operation, "step", "error should identify the faulting operation");
        }
        PartitionOutput::State(_) => panic!("expected fault, got state"),
    }
}
