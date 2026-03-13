//! Fault handling for partition operations (FPA-011).
//!
//! Provides safe execution wrappers that catch errors and panics from
//! partition trait calls, enforce timeouts, and produce enriched error context.

use fpa_contract::{Partition, PartitionError};
use std::panic::{self, AssertUnwindSafe};
use std::time::{Duration, Instant};

/// Default timeout for step operations (50ms).
pub const STEP_TIMEOUT: Duration = Duration::from_millis(50);

/// Default timeout for init operations (500ms).
pub const INIT_TIMEOUT: Duration = Duration::from_millis(500);

/// Result of a safe partition call, including fault information.
#[derive(Debug)]
pub enum FaultResult {
    /// Operation succeeded.
    Ok,
    /// Partition returned an error.
    Error(PartitionError),
    /// Partition panicked.
    Panic(PartitionError),
    /// Operation exceeded the timeout (detected post-hoc).
    Timeout(PartitionError),
}

impl FaultResult {
    /// Convert to a standard Result.
    pub fn into_result(self) -> Result<(), PartitionError> {
        match self {
            FaultResult::Ok => Ok(()),
            FaultResult::Error(e) | FaultResult::Panic(e) | FaultResult::Timeout(e) => Err(e),
        }
    }

    /// Returns true if the operation succeeded.
    pub fn is_ok(&self) -> bool {
        matches!(self, FaultResult::Ok)
    }
}

/// Execute a partition's init() with panic catching and timeout detection.
pub fn safe_init(partition: &mut dyn Partition) -> FaultResult {
    let id = partition.id().to_string();
    safe_call(&id, "init", Some(INIT_TIMEOUT), || partition.init())
}

/// Execute a partition's step() with panic catching and timeout detection.
pub fn safe_step(partition: &mut dyn Partition, dt: f64) -> FaultResult {
    let id = partition.id().to_string();
    safe_call(&id, "step", Some(STEP_TIMEOUT), || partition.step(dt))
}

/// Execute a partition's shutdown() with panic catching.
pub fn safe_shutdown(partition: &mut dyn Partition) -> FaultResult {
    let id = partition.id().to_string();
    safe_call(&id, "shutdown", None, || partition.shutdown())
}

/// Execute a partition's contribute_state() with panic catching.
pub fn safe_contribute_state(partition: &dyn Partition) -> Result<toml::Value, PartitionError> {
    let id = partition.id().to_string();
    let result = panic::catch_unwind(AssertUnwindSafe(|| partition.contribute_state()));
    match result {
        Ok(inner) => inner,
        Err(panic_info) => Err(make_panic_error(&id, "contribute_state", panic_info)),
    }
}

/// Core safe-call wrapper: catches panics and detects timeout (post-hoc).
///
/// Runs the closure on the current thread. If a timeout duration is provided,
/// checks whether the operation exceeded it after completion.
///
/// **Known limitation**: Timeout detection is post-hoc — the operation runs to
/// completion (or panic) on the current thread, and the elapsed time is checked
/// afterward. This does **not** preempt a long-running or stuck operation.
/// True preemptive timeout enforcement would require spawning the operation on
/// a separate thread and aborting it, which is left as a future enhancement.
///
/// Uses AssertUnwindSafe to wrap the closure since partition references are
/// not inherently UnwindSafe.
fn safe_call<F>(
    partition_id: &str,
    operation: &str,
    timeout: Option<Duration>,
    f: F,
) -> FaultResult
where
    F: FnOnce() -> Result<(), PartitionError>,
{
    let start = Instant::now();
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    let elapsed = start.elapsed();

    // Check timeout first (even if operation succeeded, exceeding timeout is a fault)
    if let Some(limit) = timeout {
        if elapsed > limit {
            return FaultResult::Timeout(PartitionError::new(
                partition_id,
                operation,
                format!(
                    "operation exceeded timeout of {}ms (took {}ms)",
                    limit.as_millis(),
                    elapsed.as_millis()
                ),
            ));
        }
    }

    match result {
        Ok(Ok(())) => FaultResult::Ok,
        Ok(Err(e)) => FaultResult::Error(e),
        Err(panic_info) => FaultResult::Panic(make_panic_error(partition_id, operation, panic_info)),
    }
}

/// Extract a message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// Construct a PartitionError from a panic.
fn make_panic_error(
    partition_id: &str,
    operation: &str,
    panic_info: Box<dyn std::any::Any + Send>,
) -> PartitionError {
    PartitionError::new(
        partition_id,
        operation,
        format!("panic during {}: {}", operation, panic_message(&panic_info)),
    )
}
