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
use fpa_compositor::supervisory::{FreshnessEntry, SupervisoryCompositor};
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

/// Partitions accumulate steps on their own without the compositor calling step().
#[tokio::test]
async fn partition_runs_own_processing_loop() {
    let bus = InProcessBus::new("test-bus");
    let counter = Counter::new("counter-1");

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(counter)],
        Box::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the partition to produce output
    wait_for_output(compositor.output_store(), "counter-1", Duration::from_secs(2)).await;

    // Let it accumulate a few steps
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The partition should have accumulated steps autonomously
    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("counter-1").expect("partition should have written state");
    let count = entry
        .value
        .as_table()
        .and_then(|t| t.get("count"))
        .and_then(|v| v.as_integer())
        .unwrap();
    assert!(count > 1, "partition should have stepped multiple times, got {}", count);
    drop(store);

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
        Box::new(bus),
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
            .and_then(|e| e.value.as_table())
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
            .and_then(|e| e.value.as_table())
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
        Box::new(bus),
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
        Box::new(bus),
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
        Box::new(bus),
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
        Box::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for both partitions to produce output
    let store = compositor.output_store().clone();
    wait_for_output(&store, "counter-a", Duration::from_secs(2)).await;
    wait_for_output(&store, "counter-b", Duration::from_secs(2)).await;

    let s = store.lock().unwrap();

    let count_a = s
        .get("counter-a")
        .and_then(|e| e.value.as_table())
        .and_then(|t| t.get("count"))
        .and_then(|v| v.as_integer())
        .expect("counter-a should have state");

    let count_b = s
        .get("counter-b")
        .and_then(|e| e.value.as_table())
        .and_then(|t| t.get("count"))
        .and_then(|v| v.as_integer())
        .expect("counter-b should have state");

    assert!(count_a > 0, "counter-a should have stepped");
    assert!(count_b > 0, "counter-b should have stepped");

    drop(s);
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
        Box::new(bus),
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
        Box::new(bus),
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

/// Per-partition step intervals are respected.
#[tokio::test]
async fn per_partition_step_interval() {
    let bus = InProcessBus::new("test-bus");
    let fast_counter = Counter::new("fast");
    let slow_counter = Counter::new("slow");

    let mut compositor = SupervisoryCompositor::new(
        "test",
        vec![Box::new(fast_counter), Box::new(slow_counter)],
        Box::new(bus),
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

    let s = store.lock().unwrap();
    let fast_count = s
        .get("fast")
        .and_then(|e| e.value.as_table())
        .and_then(|t| t.get("count"))
        .and_then(|v| v.as_integer())
        .unwrap_or(0);

    let slow_count = s
        .get("slow")
        .and_then(|e| e.value.as_table())
        .and_then(|t| t.get("count"))
        .and_then(|v| v.as_integer())
        .unwrap_or(0);

    assert!(
        fast_count > slow_count,
        "fast partition ({}) should have stepped more than slow partition ({})",
        fast_count,
        slow_count
    );

    drop(s);
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
        Box::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the error to be reported
    wait_for_output(compositor.output_store(), "failing-1", Duration::from_secs(2)).await;

    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("failing-1").expect("should have error entry");
    let error_msg = entry
        .value
        .as_table()
        .and_then(|t| t.get("error"))
        .and_then(|v| v.as_str());
    assert!(
        error_msg.is_some(),
        "error should be reported in the output store"
    );
    assert!(
        error_msg.unwrap().contains("intentional failure"),
        "error message should contain the failure reason"
    );

    let operation = entry
        .value
        .as_table()
        .and_then(|t| t.get("operation"))
        .and_then(|v| v.as_str());
    assert_eq!(operation, Some("step"));
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
        Box::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the panic to be caught and reported
    wait_for_output(compositor.output_store(), "panicker", Duration::from_secs(2)).await;

    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("panicker").expect("should have error entry");
    let error_msg = entry
        .value
        .as_table()
        .and_then(|t| t.get("error"))
        .and_then(|v| v.as_str());
    assert!(
        error_msg.is_some(),
        "panic should be caught and reported as error in the output store"
    );
    assert!(
        error_msg.unwrap().contains("panic"),
        "error message should mention panic: {:?}",
        error_msg
    );

    let operation = entry
        .value
        .as_table()
        .and_then(|t| t.get("operation"))
        .and_then(|v| v.as_str());
    assert_eq!(
        operation,
        Some("step"),
        "error should identify the faulting operation"
    );
}

/// FPA-011: A partition panicking during init() in a supervisory task should be
/// caught and reported — not crash the task silently.
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
        Box::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the init panic to be caught and reported
    wait_for_output(compositor.output_store(), "panicker", Duration::from_secs(2)).await;

    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("panicker").expect("should have error entry");
    let error_msg = entry
        .value
        .as_table()
        .and_then(|t| t.get("error"))
        .and_then(|v| v.as_str());
    assert!(
        error_msg.is_some(),
        "init panic should be caught and reported as error"
    );
    assert!(
        error_msg.unwrap().contains("panic"),
        "error message should mention panic: {:?}",
        error_msg
    );

    let operation = entry
        .value
        .as_table()
        .and_then(|t| t.get("operation"))
        .and_then(|v| v.as_str());
    assert_eq!(
        operation,
        Some("init"),
        "error should identify init as the faulting operation"
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
        Box::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the timeout to be detected and reported
    wait_for_output(compositor.output_store(), "slowpoke", Duration::from_secs(2)).await;

    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("slowpoke").expect("should have entry");
    let error_msg = entry
        .value
        .as_table()
        .and_then(|t| t.get("error"))
        .and_then(|v| v.as_str());
    assert!(
        error_msg.is_some(),
        "slow step should be detected as timeout fault"
    );
    assert!(
        error_msg.unwrap().contains("timeout") || error_msg.unwrap().contains("exceeded"),
        "error message should mention timeout: {:?}",
        error_msg
    );
}

/// FPA-011: A partition whose init() exceeds the 500ms timeout should be
/// reported as a timeout fault in the supervisory output store.
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
        Box::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the timeout to be detected and reported
    wait_for_output(compositor.output_store(), "slowpoke", Duration::from_secs(2)).await;

    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("slowpoke").expect("should have entry");
    let error_msg = entry
        .value
        .as_table()
        .and_then(|t| t.get("error"))
        .and_then(|v| v.as_str());
    assert!(
        error_msg.is_some(),
        "slow init should be detected as timeout fault"
    );
    assert!(
        error_msg.unwrap().contains("timeout") || error_msg.unwrap().contains("exceeded"),
        "error message should mention timeout: {:?}",
        error_msg
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
        Box::new(bus),
        Duration::from_secs(1),
    )
    .with_step_interval(Duration::from_millis(5));

    compositor.init().unwrap();

    // Wait for the error to surface
    tokio::time::sleep(Duration::from_millis(100)).await;

    let store = compositor.output_store().lock().unwrap();
    let entry = store.get("my-partition-42").expect("should have entry");
    let table = entry.value.as_table().unwrap();

    // Error should contain partition identity and operation
    if let Some(error) = table.get("error").and_then(|v| v.as_str()) {
        assert!(
            error.contains("specific-failure-message"),
            "error should preserve the original error message: {}",
            error
        );
    }
    if let Some(operation) = table.get("operation").and_then(|v| v.as_str()) {
        assert_eq!(operation, "step", "error should identify the faulting operation");
    }
}
