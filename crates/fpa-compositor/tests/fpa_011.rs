//! Tests for FPA-011: Fault Handling.

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
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

/// A partition that panics during step().
struct PanickingPartition {
    id: String,
}

impl PanickingPartition {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
        }
    }
}

impl Partition for PanickingPartition {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        Ok(())
    }

    fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
        panic!("partition panicked during step");
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

/// A partition that sleeps during step() to test timeout detection.
struct SlowPartition {
    id: String,
    delay_ms: u64,
}

impl SlowPartition {
    fn new(id: &str, delay_ms: u64) -> Self {
        Self {
            id: id.to_string(),
            delay_ms,
        }
    }
}

impl Partition for SlowPartition {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        Ok(())
    }

    fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
        std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));
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

/// A simple fallback partition that always succeeds.
struct FallbackPartition {
    id: String,
    step_count: u64,
}

impl FallbackPartition {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            step_count: 0,
        }
    }
}

impl Partition for FallbackPartition {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        Ok(())
    }

    fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
        self.step_count += 1;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        let mut table = toml::map::Map::new();
        table.insert(
            "fallback_steps".to_string(),
            toml::Value::Integer(self.step_count as i64),
        );
        Ok(toml::Value::Table(table))
    }

    fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> {
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
    let mut compositor = Compositor::new(partitions, Box::new(bus));

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
    let mut compositor = Compositor::new(partitions, Box::new(bus));

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
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.init().unwrap();
    let err = compositor.run_tick(1.0).unwrap_err();

    assert_eq!(err.partition_id, "my-partition-42");
    assert_eq!(err.operation, "step");
}

/// With fallback configured: partition faults, fallback activated, compositor continues.
#[test]
fn fallback_activated_on_fault() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(FailingPartition::new("primary", "step")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    // Register a fallback for "primary"
    compositor.register_fallback("primary", Box::new(FallbackPartition::new("primary")));

    compositor.init().unwrap();

    // Tick should succeed because fallback takes over
    let result = compositor.run_tick(1.0);
    assert!(result.is_ok(), "compositor should continue with fallback");

    // The fallback's state should be in the write buffer
    let state = compositor.buffer().write_all().get("primary").unwrap();
    let table = state.as_table().unwrap();
    assert!(
        table.contains_key("fallback_steps"),
        "fallback partition state should be in the buffer"
    );

    // Subsequent ticks should also work (fallback replaced the primary)
    let result = compositor.run_tick(1.0);
    assert!(result.is_ok(), "subsequent ticks should succeed with fallback");
}

/// Fallback is also initialized during compositor init.
#[test]
fn fallback_with_panic_partition() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(PanickingPartition::new("panicker")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.register_fallback("panicker", Box::new(FallbackPartition::new("panicker")));

    compositor.init().unwrap();

    // Panicking partition should be caught and fallback activated
    let result = compositor.run_tick(1.0);
    assert!(result.is_ok(), "compositor should recover from panic via fallback");
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
    let mut compositor = Compositor::new(partitions, Box::new(bus));

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

/// Timeout with fallback: slow partition faults, fallback takes over.
#[test]
fn slow_partition_with_fallback() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(SlowPartition::new("slowpoke", 100)),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    compositor.register_fallback("slowpoke", Box::new(FallbackPartition::new("slowpoke")));

    compositor.init().unwrap();
    let result = compositor.run_tick(1.0);
    assert!(result.is_ok(), "fallback should handle timeout fault");
}

/// Multiple partitions: one failing, one healthy. Compositor reports the failure.
#[test]
fn mixed_healthy_and_failing_partitions() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(fpa_contract::test_support::Counter::new("healthy")),
        Box::new(FailingPartition::new("failer", "step")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

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
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    let result = compositor.init();
    assert!(result.is_err());
    assert_eq!(compositor.state(), ExecutionState::Error);
}
