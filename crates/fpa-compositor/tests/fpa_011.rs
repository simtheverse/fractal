//! Tests for FPA-011: Fault Handling.

use std::sync::Arc;

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::{Compositor, LifecycleOp};
use fpa_compositor::state_machine::ExecutionState;
use fpa_contract::{Partition, PartitionError};

// --- Test partition implementations ---

/// A partition that returns Err on a specified operation.
struct FailingPartition {
    id: String,
    fail_on: String,
}

impl FailingPartition {
    fn new(id: &str, fail_on: &str) -> Self {
        Self {
            id: id.to_string(),
            fail_on: fail_on.to_string(),
        }
    }
}

impl Partition for FailingPartition {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        if self.fail_on == "init" {
            return Err(PartitionError::new(&self.id, "init", "deliberate init failure"));
        }
        Ok(())
    }

    fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
        if self.fail_on == "step" {
            return Err(PartitionError::new(&self.id, "step", "deliberate step failure"));
        }
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        if self.fail_on == "shutdown" {
            return Err(PartitionError::new(
                &self.id,
                "shutdown",
                "deliberate shutdown failure",
            ));
        }
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        Ok(toml::Value::Table(toml::map::Map::new()))
    }

    fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> {
        Ok(())
    }
}

/// A partition that panics during a specified operation.
struct PanickingPartition {
    id: String,
    panic_on: String,
}

impl PanickingPartition {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            panic_on: "step".to_string(),
        }
    }

    fn on(id: &str, operation: &str) -> Self {
        Self {
            id: id.to_string(),
            panic_on: operation.to_string(),
        }
    }
}

impl Partition for PanickingPartition {
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
        if self.panic_on == "shutdown" {
            panic!("partition panicked during shutdown");
        }
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        if self.panic_on == "contribute_state" {
            panic!("partition panicked during contribute_state");
        }
        Ok(toml::Value::Table(toml::map::Map::new()))
    }

    fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> {
        if self.panic_on == "load_state" {
            panic!("partition panicked during load_state");
        }
        Ok(())
    }
}

/// A partition that sleeps during a specified operation to test timeout detection.
struct SlowPartition {
    id: String,
    delay_ms: u64,
    slow_on: String,
}

impl SlowPartition {
    fn new(id: &str, delay_ms: u64) -> Self {
        Self {
            id: id.to_string(),
            delay_ms,
            slow_on: "step".to_string(),
        }
    }

    fn on(id: &str, delay_ms: u64, operation: &str) -> Self {
        Self {
            id: id.to_string(),
            delay_ms,
            slow_on: operation.to_string(),
        }
    }

    fn maybe_sleep(&self, operation: &str) {
        if self.slow_on == operation {
            std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));
        }
    }
}

impl Partition for SlowPartition {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        self.maybe_sleep("init");
        Ok(())
    }

    fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
        self.maybe_sleep("step");
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        self.maybe_sleep("shutdown");
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        self.maybe_sleep("contribute_state");
        Ok(toml::Value::Table(toml::map::Map::new()))
    }

    fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> {
        self.maybe_sleep("load_state");
        Ok(())
    }
}

// --- Tests ---

/// Partition returning error from step() -> compositor catches and returns error with context.
#[test]
fn step_error_is_caught_with_context() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(FailingPartition::new("failer", "step")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    let result = compositor.run_tick(1.0);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.partition_id, "failer");
    assert_eq!(err.operation, "step");
    assert!(
        err.message.contains("deliberate step failure"),
        "error message should contain original error: {}",
        err.message
    );
}

/// Partition panicking during step() -> compositor catches panic and returns error.
#[test]
fn panic_during_step_is_caught() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(PanickingPartition::new("panicker")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    let result = compositor.run_tick(1.0);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.partition_id, "panicker");
    assert_eq!(err.operation, "step");
    assert!(
        err.message.contains("panic"),
        "error message should mention panic: {}",
        err.message
    );
}

/// Error includes partition ID and operation name.
#[test]
fn error_includes_partition_id_and_operation() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(FailingPartition::new("my-partition-42", "step")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    let err = compositor.run_tick(1.0).unwrap_err();

    assert_eq!(err.partition_id, "my-partition-42");
    assert_eq!(err.operation, "step");
}

/// Timeout: partition step exceeding 50ms is treated as a fault.
///
/// This test uses a partition that sleeps for 100ms, exceeding the 50ms timeout.
/// The timeout is detected post-hoc (after the operation completes).
#[test]
fn slow_partition_detected_as_timeout() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(SlowPartition::new("slowpoke", 100)),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    let result = compositor.run_tick(1.0);

    assert!(result.is_err(), "slow partition should be detected as timeout fault");
    let err = result.unwrap_err();
    assert_eq!(err.partition_id, "slowpoke");
    assert_eq!(err.operation, "step");
    assert!(
        err.message.contains("timeout") || err.message.contains("exceeded"),
        "error should mention timeout: {}",
        err.message
    );
}

/// Multiple partitions: one failing, one healthy. Compositor reports the failure.
#[test]
fn mixed_healthy_and_failing_partitions() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(fpa_contract::test_support::Counter::new("healthy")),
        Box::new(FailingPartition::new("failer", "step")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    let result = compositor.run_tick(1.0);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.partition_id, "failer");
}

/// Init failure transitions state machine to Error.
#[test]
fn init_failure_transitions_to_error() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(FailingPartition::new("failer", "init")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    let result = compositor.init();
    assert!(result.is_err());
    assert_eq!(compositor.state(), ExecutionState::Error);
}

// --- New tests: panics across all lifecycle methods (FPA-011 audit gap) ---

/// Panic during init() is caught and returned as error with correct context.
#[test]
fn panic_during_init_is_caught() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(PanickingPartition::on("panicker", "init")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    let result = compositor.init();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.partition_id, "panicker");
    assert_eq!(err.operation, "init");
    assert!(
        err.message.contains("panic"),
        "error message should mention panic: {}",
        err.message
    );
}

/// Panic during shutdown() is caught and returned as error.
#[test]
fn panic_during_shutdown_is_caught() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(PanickingPartition::on("panicker", "shutdown")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    let result = compositor.shutdown();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.partition_id, "panicker");
    assert!(
        err.message.contains("panic"),
        "error message should mention panic: {}",
        err.message
    );
}

/// Panic during contribute_state() is caught (via dump).
#[test]
fn panic_during_contribute_state_is_caught() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(PanickingPartition::on("panicker", "contribute_state")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    let result = compositor.dump();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.partition_id, "panicker");
    assert!(
        err.message.contains("panic"),
        "error message should mention panic: {}",
        err.message
    );
}

/// Panic during load_state() is caught.
#[test]
fn panic_during_load_state_is_caught() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(PanickingPartition::on("panicker", "load_state")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.pause().unwrap();

    let state: toml::Value = toml::from_str(
        r#"
        [system]
        tick_count = 0
        [partitions.panicker]
        fresh = true
        age_ms = 0
        [partitions.panicker.state]
        "#,
    )
    .unwrap();

    let result = compositor.load(state);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.partition_id, "panicker");
    assert!(
        err.message.contains("panic"),
        "error message should mention panic: {}",
        err.message
    );
}

// --- New tests: timeouts for init, shutdown, contribute_state (FPA-011 audit gap) ---

/// Init exceeding 500ms timeout is detected as fault.
#[test]
fn slow_init_detected_as_timeout() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(SlowPartition::on("slowpoke", 600, "init")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    let result = compositor.init();
    assert!(result.is_err(), "slow init should be detected as timeout fault");
    let err = result.unwrap_err();
    assert_eq!(err.partition_id, "slowpoke");
    assert_eq!(err.operation, "init");
    assert!(
        err.message.contains("timeout") || err.message.contains("exceeded"),
        "error should mention timeout: {}",
        err.message
    );
}

/// Shutdown exceeding 500ms timeout is detected as fault.
#[test]
fn slow_shutdown_detected_as_timeout() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(SlowPartition::on("slowpoke", 600, "shutdown")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    let result = compositor.shutdown();
    assert!(result.is_err(), "slow shutdown should be detected as timeout fault");
    let err = result.unwrap_err();
    assert_eq!(err.partition_id, "slowpoke");
    assert!(
        err.message.contains("timeout") || err.message.contains("exceeded"),
        "error should mention timeout: {}",
        err.message
    );
}

/// contribute_state() exceeding 50ms timeout is detected as fault (via dump).
#[test]
fn slow_contribute_state_detected_as_timeout() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(SlowPartition::on("slowpoke", 100, "contribute_state")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    let result = compositor.dump();
    assert!(result.is_err(), "slow contribute_state should be detected as timeout fault");
    let err = result.unwrap_err();
    assert_eq!(err.partition_id, "slowpoke");
    assert!(
        err.message.contains("timeout") || err.message.contains("exceeded"),
        "error should mention timeout: {}",
        err.message
    );
}

/// Init error includes correct operation context (not "step").
#[test]
fn error_during_init_includes_operation_context() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(FailingPartition::new("failer", "init")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    let err = compositor.init().unwrap_err();
    assert_eq!(err.partition_id, "failer");
    assert_eq!(err.operation, "init");
    assert!(
        err.message.contains("deliberate init failure"),
        "error should contain original message: {}",
        err.message
    );
}

// --- Despawn shutdown warning tests (FPA-011 despawn exception) ---

/// Despawn of a partition that fails shutdown records a lifecycle warning
/// rather than propagating the error.
#[test]
fn despawn_shutdown_error_recorded_as_warning() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(FailingPartition::new("failer", "shutdown")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.request_lifecycle_op(LifecycleOp::Despawn("failer".to_string()));

    // Tick should succeed — despawn shutdown errors don't propagate
    let result = compositor.run_tick(1.0);
    assert!(result.is_ok(), "despawn shutdown failure should not fail the tick");

    // The error should be available as a lifecycle warning
    let warnings = compositor.drain_lifecycle_warnings();
    assert_eq!(warnings.len(), 1, "should have one lifecycle warning");
    assert_eq!(warnings[0].partition_id, "failer");
    assert_eq!(warnings[0].operation, "shutdown");
}

/// Despawn of a partition that panics during shutdown records a lifecycle
/// warning rather than crashing the compositor.
#[test]
fn despawn_shutdown_panic_recorded_as_warning() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(PanickingPartition::on("panicker", "shutdown")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.request_lifecycle_op(LifecycleOp::Despawn("panicker".to_string()));

    let result = compositor.run_tick(1.0);
    assert!(result.is_ok(), "despawn shutdown panic should not fail the tick");

    let warnings = compositor.drain_lifecycle_warnings();
    assert_eq!(warnings.len(), 1, "should have one lifecycle warning");
    assert_eq!(warnings[0].partition_id, "panicker");
    assert!(
        warnings[0].message.contains("panic"),
        "warning should mention panic: {}",
        warnings[0].message
    );
}

/// Successful despawn produces no lifecycle warnings.
#[test]
fn despawn_clean_shutdown_no_warnings() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(fpa_contract::test_support::Counter::new("clean")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Arc::new(bus));

    compositor.init().unwrap();
    compositor.request_lifecycle_op(LifecycleOp::Despawn("clean".to_string()));
    compositor.run_tick(1.0).unwrap();

    let warnings = compositor.drain_lifecycle_warnings();
    assert!(warnings.is_empty(), "clean despawn should produce no warnings");
}

