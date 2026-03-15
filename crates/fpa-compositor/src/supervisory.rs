//! Supervisory compositor: partitions run their own processing loops (FPA-009).
//!
//! Unlike the lock-step `Compositor`, the supervisory variant does NOT call
//! `step()` on partitions directly. Instead, partitions are spawned as tokio
//! tasks that run independently and publish their state to a shared store.
//! The compositor monitors heartbeats and reports freshness metadata.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fpa_bus::{Bus, BusExt, InProcessBus};
use fpa_contract::{Partition, PartitionError, SharedContext, StateContribution};
use crate::direct_signal::DirectSignal;
use crate::fault;
use crate::state_machine::{ExecutionState, StateMachine, TransitionRequest};

/// Init result sent by each partition task after init completes.
/// Ok(id) on success, Err((id, operation, message)) on failure.
type InitSignal = Result<String, (String, String, String)>;

/// Run a partition lifecycle call on a blocking thread with deadline
/// monitoring (FPA-011).
///
/// The partition is moved into `spawn_blocking` so the tokio worker thread
/// is free. `tokio::time::timeout` races the call against the deadline.
///
/// **Limitation:** `tokio::time::timeout` + `spawn_blocking` stops *awaiting*
/// the blocking thread on timeout, but cannot actually kill it. The blocking
/// thread continues running (it is abandoned, not terminated). The compositor
/// stops waiting and reports the fault, but the underlying OS thread is only
/// reclaimed when the blocking call eventually returns or the process exits.
///
/// On success: returns the partition and the call's result.
/// On timeout: the partition is abandoned on the blocking thread (it's
///   faulted and won't be used again). Returns a PartitionError.
/// On panic: the partition is lost. Returns a PartitionError.
async fn supervised_lifecycle<T: Send + 'static>(
    partition: Box<dyn Partition>,
    deadline: Duration,
    operation: &'static str,
    f: impl FnOnce(&mut dyn Partition) -> Result<T, PartitionError> + Send + 'static,
) -> Result<(Box<dyn Partition>, T), PartitionError> {
    use std::panic::{self, AssertUnwindSafe};

    // Capture ID before moving partition into spawn_blocking — needed for
    // timeout/join-error cases where the partition is lost.
    let partition_id = partition.id().to_string();

    let result = tokio::time::timeout(
        deadline,
        tokio::task::spawn_blocking(move || {
            let mut p = partition;
            let call_result = panic::catch_unwind(AssertUnwindSafe(|| f(&mut *p)));
            (p, call_result)
        }),
    )
    .await;

    match result {
        // Call completed within deadline
        Ok(Ok((p, Ok(Ok(value))))) => Ok((p, value)),
        // Call returned an error
        Ok(Ok((_p, Ok(Err(e))))) => Err(e),
        // Call panicked — partition is in indeterminate state, abandoned
        Ok(Ok((_p, Err(panic_info)))) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            Err(PartitionError::new(
                &partition_id, operation,
                format!("panic during {}: {}", operation, msg),
            ))
        }
        // spawn_blocking task failed
        Ok(Err(join_err)) => {
            let detail = if join_err.is_panic() {
                format!("{} task panicked: {}", operation, join_err)
            } else {
                format!("{} task was cancelled: {}", operation, join_err)
            };
            Err(PartitionError::new(
                &partition_id, operation, detail,
            ))
        }
        // Timeout — partition abandoned on blocking thread
        Err(_) => {
            Err(PartitionError::new(
                &partition_id, operation,
                format!("{} exceeded timeout of {}ms", operation, deadline.as_millis()),
            ))
        }
    }
}

/// The output of a partition task: either successfully contributed state or a fault.
#[derive(Debug, Clone)]
pub enum PartitionOutput {
    /// The partition's contributed state value from `contribute_state()`.
    State(toml::Value),
    /// A fault recorded during a lifecycle invocation (FPA-011).
    Fault {
        /// The lifecycle operation that faulted (init, step, shutdown, contribute_state).
        operation: String,
        /// The error message from the fault.
        message: String,
    },
}

/// An entry in the output store with freshness tracking.
#[derive(Debug, Clone)]
pub struct FreshnessEntry {
    /// The partition's output — either state or a fault.
    pub output: PartitionOutput,
    /// When this entry was last updated.
    pub updated_at: Instant,
    /// The tick (step count) when this entry was produced.
    pub tick: u64,
}

impl FreshnessEntry {
    /// Returns the state value if this entry contains state (not a fault).
    pub fn state(&self) -> Option<&toml::Value> {
        match &self.output {
            PartitionOutput::State(v) => Some(v),
            PartitionOutput::Fault { .. } => None,
        }
    }

    /// Returns true if this entry contains a fault.
    pub fn is_fault(&self) -> bool {
        matches!(&self.output, PartitionOutput::Fault { .. })
    }
}

/// Handle to a spawned partition task, used for lifecycle management.
pub struct PartitionHandle {
    /// The partition's ID.
    pub id: String,
    /// Join handle for the spawned task.
    pub join_handle: tokio::task::JoinHandle<()>,
    /// One-shot sender to signal shutdown.
    pub shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

/// A supervisory compositor where partitions run their own processing loops.
///
/// The supervisory compositor spawns each partition as an independent tokio
/// task. Each task runs its own `init()` / `step()` / `contribute_state()`
/// loop, writing results to a shared `output_store`. The compositor reads
/// from this store to check heartbeats and publish aggregated state.
pub struct SupervisoryCompositor {
    id: String,
    partition_handles: Vec<PartitionHandle>,
    bus: Arc<dyn Bus>,
    state_machine: StateMachine,
    output_store: Arc<Mutex<HashMap<String, FreshnessEntry>>>,
    heartbeat_timeout: Duration,
    layer_depth: u32,
    /// Partitions waiting to be spawned (consumed during init).
    pending_partitions: Vec<Box<dyn Partition>>,
    /// Fault handling timeout configuration (FPA-011).
    timeout_config: fault::TimeoutConfig,
    /// Default step interval for partition tasks.
    step_interval: Duration,
    /// Per-partition step intervals (overrides `step_interval` for specific partitions).
    partition_intervals: HashMap<String, Duration>,
    /// Tick counter for run_tick.
    tick_count: u64,
    /// Direct signals collected from inner compositor partitions (FPA-013).
    /// Shared with spawned tasks so signals propagate through supervisory nesting.
    emitted_signals: Arc<Mutex<Vec<DirectSignal>>>,
}

impl SupervisoryCompositor {
    /// Create a new supervisory compositor.
    ///
    /// `heartbeat_timeout` controls how long a partition can go without
    /// updating before being considered stale/faulted.
    ///
    /// Accepts any `Bus` implementation via `Arc<dyn Bus>`, enabling runtime
    /// transport selection (FPA-004) and shared ownership.
    /// For convenience with `InProcessBus`,
    /// use `SupervisoryCompositor::new_default`.
    pub fn new(
        id: impl Into<String>,
        partitions: Vec<Box<dyn Partition>>,
        bus: Arc<dyn Bus>,
        heartbeat_timeout: Duration,
    ) -> Self {
        Self {
            id: id.into(),
            partition_handles: Vec::new(),
            bus,
            state_machine: StateMachine::new(),
            output_store: Arc::new(Mutex::new(HashMap::new())),
            heartbeat_timeout,
            layer_depth: 0,
            timeout_config: fault::TimeoutConfig::default(),
            pending_partitions: partitions,
            step_interval: Duration::from_millis(10),
            partition_intervals: HashMap::new(),
            tick_count: 0,
            emitted_signals: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create a new supervisory compositor with a default `InProcessBus`.
    ///
    /// Convenience constructor for the common case where in-process transport
    /// is sufficient.
    pub fn new_default(
        id: impl Into<String>,
        partitions: Vec<Box<dyn Partition>>,
        bus_id: impl Into<String>,
        heartbeat_timeout: Duration,
    ) -> Self {
        Self::new(id, partitions, Arc::new(InProcessBus::new(bus_id)), heartbeat_timeout)
    }

    /// Set the layer depth for this compositor.
    pub fn with_layer_depth(mut self, depth: u32) -> Self {
        self.layer_depth = depth;
        self
    }

    /// Set the fault handling timeout configuration (FPA-011).
    pub fn set_timeout_config(&mut self, config: fault::TimeoutConfig) {
        self.timeout_config = config;
    }

    /// Set the step interval for partition task loops.
    pub fn with_step_interval(mut self, interval: Duration) -> Self {
        self.step_interval = interval;
        self
    }

    /// Set a per-partition step interval, overriding the default for this partition.
    pub fn with_partition_interval(&mut self, partition_id: impl Into<String>, interval: Duration) {
        self.partition_intervals.insert(partition_id.into(), interval);
    }

    /// Get the current execution state.
    pub fn state(&self) -> ExecutionState {
        self.state_machine.state()
    }

    /// Get the current tick count.
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Get a reference to the bus.
    pub fn bus(&self) -> &dyn Bus {
        &*self.bus
    }

    /// Get a reference to the output store.
    pub fn output_store(&self) -> &Arc<Mutex<HashMap<String, FreshnessEntry>>> {
        &self.output_store
    }

    /// Spawn partition tasks and transition to Running.
    ///
    /// Each partition is moved into its own tokio task that runs `init()`,
    /// then loops `step()` / `contribute_state()`. This method returns
    /// immediately after spawning — it does NOT wait for partition init to
    /// complete. This is the synchronous counterpart to [`async_init`],
    /// following the same pattern as `shutdown()` vs `async_shutdown()`:
    /// the sync method signals intent, the async method confirms completion.
    ///
    /// For init fault propagation per FPA-011, use [`async_init`] which
    /// awaits init completion and returns faults from its own call.
    pub fn init(&mut self) -> Result<(), PartitionError> {
        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: self.id.clone(),
                target_state: ExecutionState::Initializing,
            })
            .map_err(|e| self.make_error("compositor", "init", e.to_string()))?;

        // Drop the init receiver — faults will be detected via run_tick/contribute_state.
        let _init_rx = self.spawn_partition_tasks();

        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: self.id.clone(),
                target_state: ExecutionState::Running,
            })
            .map_err(|e| self.make_error("compositor", "init", e.to_string()))?;

        Ok(())
    }

    /// Initialize the supervisory compositor with confirmed init completion.
    ///
    /// Spawns each partition as a tokio task, then awaits each partition's
    /// init result via a dedicated channel. If any partition faults during
    /// init, transitions to Error and returns the fault from this call —
    /// satisfying FPA-011's requirement that init faults propagate from the
    /// compositor's own init invocation.
    ///
    /// This mirrors the `async_shutdown()` pattern: the sync `init()` signals
    /// intent (spawns tasks), while `async_init()` confirms completion (awaits
    /// init results and propagates faults).
    pub async fn async_init(&mut self) -> Result<(), PartitionError> {
        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: self.id.clone(),
                target_state: ExecutionState::Initializing,
            })
            .map_err(|e| self.make_error("compositor", "init", e.to_string()))?;

        let mut init_rx = self.spawn_partition_tasks();
        let partition_count = self.partition_handles.len();

        // Receive init results from each partition task. Each task sends
        // exactly once after safe_init() completes (success or failure).
        for _ in 0..partition_count {
            match init_rx.recv().await {
                Some(Ok(_id)) => { /* init succeeded */ }
                Some(Err((id, operation, message))) => {
                    // Shut down partition tasks that already initialized and
                    // are running their step/contribute loop. Without this,
                    // successfully-initialized tasks would keep running after
                    // async_init returns an error.
                    let handles = std::mem::take(&mut self.partition_handles);
                    for handle in handles {
                        let _ = handle.shutdown_tx.send(());
                        let _ = handle.join_handle.await;
                    }
                    self.state_machine.force_state(ExecutionState::Error);
                    return Err(self.make_error(&id, &operation, message));
                }
                None => {
                    // Channel closed — a task panicked before sending its init result.
                    // Clean up running tasks before returning.
                    let handles = std::mem::take(&mut self.partition_handles);
                    for handle in handles {
                        let _ = handle.shutdown_tx.send(());
                        let _ = handle.join_handle.await;
                    }
                    self.state_machine.force_state(ExecutionState::Error);
                    return Err(self.make_error(
                        "compositor",
                        "init",
                        "partition task terminated before completing init".to_string(),
                    ));
                }
            }
        }

        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: self.id.clone(),
                target_state: ExecutionState::Running,
            })
            .map_err(|e| self.make_error("compositor", "init", e.to_string()))?;

        Ok(())
    }

    /// Spawn partition tasks for the init/step/contribute_state loop.
    ///
    /// Consumes pending partitions and creates a tokio task for each one.
    /// Returns a receiver for init completion signals — each task sends
    /// exactly one signal after `safe_init()` completes.
    /// Called by both `init()` and `async_init()`.
    fn spawn_partition_tasks(
        &mut self,
    ) -> tokio::sync::mpsc::Receiver<InitSignal> {
        let partitions = std::mem::take(&mut self.pending_partitions);
        let (init_tx, init_rx) = tokio::sync::mpsc::channel(partitions.len().max(1));

        for partition in partitions {
            let partition_id = partition.id().to_string();
            let store = Arc::clone(&self.output_store);
            let signals = Arc::clone(&self.emitted_signals);
            let init_tx = init_tx.clone();
            let step_interval = self
                .partition_intervals
                .get(&partition_id)
                .copied()
                .unwrap_or(self.step_interval);
            let lifecycle_timeout = self.timeout_config.lifecycle;
            let step_timeout = self.timeout_config.step;
            let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

            let join_handle = tokio::spawn(async move {
                // Initialize with deadline monitoring (FPA-011).
                // Runs on a blocking thread so the tokio worker is free.
                let partition = match supervised_lifecycle(
                    partition, lifecycle_timeout, "init",
                    |p| { p.init()?; Ok(()) },
                ).await {
                    Ok((p, ())) => {
                        let _ = init_tx.send(Ok(p.id().to_string())).await;
                        drop(init_tx);
                        p
                    }
                    Err(fault) => {
                        let id = fault.partition_id.clone();
                        let operation = fault.operation.clone();
                        let message = fault.message.clone();
                        {
                            let mut s = store.lock().unwrap();
                            s.insert(
                                id.clone(),
                                FreshnessEntry {
                                    output: PartitionOutput::Fault {
                                        operation: operation.clone(),
                                        message: message.clone(),
                                    },
                                    updated_at: Instant::now(),
                                    tick: 0,
                                },
                            );
                        }
                        let _ = init_tx.send(Err((id, operation, message))).await;
                        return;
                    }
                };

                let mut partition = Some(partition);
                let mut tick: u64 = 0;
                let dt = step_interval.as_secs_f64();

                loop {
                    // Check for shutdown signal (non-blocking)
                    match shutdown_rx.try_recv() {
                        Ok(()) | Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                            if let Some(p) = partition.take() {
                                match supervised_lifecycle(
                                    p, lifecycle_timeout, "shutdown",
                                    |p| { p.shutdown()?; Ok(()) },
                                ).await {
                                    Ok((_p, ())) => { /* shutdown succeeded */ }
                                    Err(fault) => {
                                        let mut s = store.lock().unwrap();
                                        s.insert(
                                            fault.partition_id.clone(),
                                            FreshnessEntry {
                                                output: PartitionOutput::Fault {
                                                    operation: fault.operation.clone(),
                                                    message: fault.message.clone(),
                                                },
                                                updated_at: Instant::now(),
                                                tick,
                                            },
                                        );
                                    }
                                }
                            }
                            break;
                        }
                        Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                    }

                    // Step with deadline monitoring (FPA-011).
                    let p = partition.take().unwrap();
                    match supervised_lifecycle(
                        p, step_timeout, "step",
                        move |p| { p.step(dt)?; Ok(()) },
                    ).await {
                        Ok((p, ())) => { partition = Some(p); }
                        Err(fault) => {
                            let mut s = store.lock().unwrap();
                            s.insert(
                                fault.partition_id.clone(),
                                FreshnessEntry {
                                    output: PartitionOutput::Fault {
                                        operation: fault.operation.clone(),
                                        message: fault.message.clone(),
                                    },
                                    updated_at: Instant::now(),
                                    tick,
                                },
                            );
                            break;
                        }
                    }
                    tick += 1;

                    // Collect direct signals from inner compositor partitions (FPA-013)
                    if let Some(any) = partition.as_mut().and_then(|p| p.as_any_mut()) {
                        use crate::compositor::Compositor;
                        let drained = if let Some(inner_comp) = any.downcast_mut::<Compositor>() {
                            inner_comp.drain_emitted_signals()
                        } else if let Some(inner_sup) = any.downcast_mut::<SupervisoryCompositor>() {
                            inner_sup.drain_emitted_signals()
                        } else {
                            Vec::new()
                        };
                        if !drained.is_empty() {
                            signals.lock().unwrap().extend(drained);
                        }
                    }

                    // Contribute state with deadline monitoring (FPA-011).
                    let p = partition.take().unwrap();
                    match supervised_lifecycle(
                        p, step_timeout, "contribute_state",
                        |p| p.contribute_state(),
                    ).await {
                        Ok((p, value)) => {
                            let id = p.id().to_string();
                            partition = Some(p);
                            let mut s = store.lock().unwrap();
                            s.insert(
                                id,
                                FreshnessEntry {
                                    output: PartitionOutput::State(value),
                                    updated_at: Instant::now(),
                                    tick,
                                },
                            );
                        }
                        Err(fault) => {
                            let mut s = store.lock().unwrap();
                            s.insert(
                                fault.partition_id.clone(),
                                FreshnessEntry {
                                    output: PartitionOutput::Fault {
                                        operation: fault.operation.clone(),
                                        message: fault.message.clone(),
                                    },
                                    updated_at: Instant::now(),
                                    tick,
                                },
                            );
                            break;
                        }
                    }

                    tokio::time::sleep(step_interval).await;
                }
            });

            self.partition_handles.push(PartitionHandle {
                id: partition_id,
                join_handle,
                shutdown_tx,
            });
        }

        init_rx
    }

    /// Scan the output store for the first faulted partition.
    /// Returns (id, operation, message) if a fault is found.
    fn find_fault(store: &HashMap<String, FreshnessEntry>) -> Option<(String, String, String)> {
        store.iter().find_map(|(id, entry)| {
            if let PartitionOutput::Fault { operation, message } = &entry.output {
                Some((id.clone(), operation.clone(), message.clone()))
            } else {
                None
            }
        })
    }

    /// Read latest state from the output store, check heartbeat freshness,
    /// and publish aggregated state on the bus.
    ///
    /// Checks for faulted partitions atomically within the same lock
    /// acquisition used to build the snapshot (FPA-011). If any partition
    /// has faulted, transitions to Error state and returns the fault —
    /// propagating it to the outer layer.
    ///
    /// Each partition entry includes freshness metadata:
    /// - `fresh`: whether the partition updated since the last check
    /// - `age_ms`: milliseconds since last update
    ///
    /// The `_dt` parameter is accepted for API compatibility with the lock-step
    /// compositor but is unused — supervisory partitions manage their own timing.
    pub fn run_tick(&mut self, _dt: f64) -> Result<(), PartitionError> {
        if self.state_machine.state() != ExecutionState::Running {
            return Err(self.make_error(
                "compositor",
                "run_tick",
                format!("cannot run tick in state {}", self.state_machine.state()),
            ));
        }

        self.tick_count += 1;
        let now = Instant::now();

        let store = self.output_store.lock().unwrap();

        // Fault check and state snapshot in one lock acquisition
        if let Some((id, operation, message)) = Self::find_fault(&store) {
            drop(store);
            self.state_machine.force_state(ExecutionState::Error);
            return Err(self.make_error(&id, &operation, message));
        }

        let mut table = toml::map::Map::new();
        for (id, entry) in store.iter() {
            if let PartitionOutput::State(value) = &entry.output {
                let age = now.duration_since(entry.updated_at);
                let fresh = age < self.heartbeat_timeout;
                let age_ms = age.as_millis() as u64;

                let envelope = StateContribution {
                    state: value.clone(),
                    fresh,
                    age_ms,
                };
                table.insert(id.clone(), envelope.to_toml());
            }
        }

        drop(store);

        self.bus.publish(SharedContext {
            state: toml::Value::Table(table),
            tick: self.tick_count,
            execution_state: self.state_machine.state(),
        });

        Ok(())
    }

    /// Return the IDs of partitions whose output is stale (exceeds heartbeat timeout)
    /// or that have never produced output.
    pub fn stale_partitions(&self) -> Vec<String> {
        let now = Instant::now();
        let store = self.output_store.lock().unwrap();
        let mut stale_ids = Vec::new();

        for handle in &self.partition_handles {
            if let Some(entry) = store.get(&handle.id) {
                let age = now.duration_since(entry.updated_at);
                if age >= self.heartbeat_timeout {
                    stale_ids.push(handle.id.clone());
                }
            } else {
                // No entry at all - partition never produced output
                stale_ids.push(handle.id.clone());
            }
        }

        stale_ids
    }

    /// Check whether a partition is fresh (has updated within the heartbeat timeout).
    pub fn is_partition_fresh(&self, partition_id: &str) -> Option<bool> {
        let store = self.output_store.lock().unwrap();
        store.get(partition_id).map(|entry| {
            let age = Instant::now().duration_since(entry.updated_at);
            age < self.heartbeat_timeout
        })
    }

    /// Get freshness details for a partition.
    pub fn partition_freshness(&self, partition_id: &str) -> Option<(bool, Duration)> {
        let store = self.output_store.lock().unwrap();
        store.get(partition_id).map(|entry| {
            let age = Instant::now().duration_since(entry.updated_at);
            (age < self.heartbeat_timeout, age)
        })
    }

    /// Shut down all partition tasks gracefully (async) and transition to Terminated.
    ///
    /// This awaits each task's join handle, ensuring all partitions have fully
    /// stopped before returning. After joining, checks the output store for
    /// any faults recorded during shutdown (FPA-011).
    pub async fn async_shutdown(&mut self) -> Result<(), PartitionError> {
        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: self.id.clone(),
                target_state: ExecutionState::ShuttingDown,
            })
            .map_err(|e| self.make_error("compositor", "shutdown", e.to_string()))?;

        // Send shutdown signals and join all tasks
        let handles = std::mem::take(&mut self.partition_handles);
        for handle in handles {
            let _ = handle.shutdown_tx.send(());
            if let Err(join_err) = handle.join_handle.await {
                // Task terminated outside safe_* wrappers — record as fault
                // so find_fault catches it below.
                let message = if join_err.is_panic() {
                    format!("partition '{}' task panicked unexpectedly", handle.id)
                } else {
                    format!("partition '{}' task was cancelled", handle.id)
                };
                let mut s = self.output_store.lock().unwrap();
                s.insert(
                    handle.id.clone(),
                    FreshnessEntry {
                        output: PartitionOutput::Fault {
                            operation: "task".to_string(),
                            message,
                        },
                        updated_at: Instant::now(),
                        tick: 0,
                    },
                );
            }
        }

        // Check for faults recorded during shutdown (FPA-011).
        // Must check before transitioning to Terminated so the error
        // is propagated to the caller.
        {
            let store = self.output_store.lock().unwrap();
            if let Some((id, operation, message)) = Self::find_fault(&store) {
                drop(store);
                self.state_machine.force_state(ExecutionState::Error);
                return Err(self.make_error(&id, &operation, message));
            }
        }

        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: self.id.clone(),
                target_state: ExecutionState::Terminated,
            })
            .map_err(|e| self.make_error("compositor", "shutdown", e.to_string()))?;

        Ok(())
    }

    /// Contribute aggregated state with freshness metadata.
    ///
    /// Checks for faulted partitions before contributing state (FPA-011).
    /// If any partition has faulted, returns an error identifying the faulting
    /// partition and operation.
    pub fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        let now = Instant::now();
        let store = self.output_store.lock().unwrap();
        let mut table = toml::map::Map::new();

        for (id, entry) in store.iter() {
            match &entry.output {
                PartitionOutput::State(value) => {
                    let age = now.duration_since(entry.updated_at);
                    let fresh = age < self.heartbeat_timeout;
                    let age_ms = age.as_millis() as u64;

                    let envelope = StateContribution {
                        state: value.clone(),
                        fresh,
                        age_ms,
                    };
                    table.insert(id.clone(), envelope.to_toml());
                }
                PartitionOutput::Fault { operation, message } => {
                    self.state_machine.force_state(ExecutionState::Error);
                    return Err(self.make_error(id, operation, message.clone()));
                }
            }
        }

        Ok(toml::Value::Table(table))
    }

    /// Drain and return all direct signals collected from inner compositor
    /// partitions (FPA-013). Signals accumulate in the shared store as spawned
    /// tasks step their partitions; this method transfers them out for
    /// propagation to the outer layer.
    pub fn drain_emitted_signals(&mut self) -> Vec<DirectSignal> {
        std::mem::take(&mut *self.emitted_signals.lock().unwrap())
    }

    fn make_error(&self, partition_id: &str, operation: &str, message: String) -> PartitionError {
        PartitionError::new(partition_id, operation, message).with_layer_depth(self.layer_depth)
    }
}

/// Implement `Partition` for `SupervisoryCompositor`, enabling nesting (FPA-001).
///
/// A supervisory compositor can itself be a partition inside an outer compositor,
/// creating fractal nesting. The sync `init()` and `shutdown()` methods signal
/// intent (spawn/stop tasks) but do not confirm completion. Use `async_init()`
/// and `async_shutdown()` for confirmed lifecycle transitions with fault
/// propagation per FPA-011.
impl Partition for SupervisoryCompositor {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        SupervisoryCompositor::init(self)
    }

    fn step(&mut self, dt: f64) -> Result<(), PartitionError> {
        self.run_tick(dt)
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        // Synchronous shutdown signals task termination but does NOT confirm
        // completion. This is intentional — it surfaces a spec finding:
        //
        // FPA-009 says "the compositor controls when [partitions] must stop"
        // but the synchronous Partition trait can only SIGNAL shutdown, not
        // CONFIRM it, when the partition runs async tasks. Use
        // `async_shutdown()` for confirmed shutdown with task join.
        //
        // See FPA-009 in SPECIFICATION.md for the full spec text on this.
        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: self.id.clone(),
                target_state: ExecutionState::ShuttingDown,
            })
            .map_err(|e| self.make_error("compositor", "shutdown", e.to_string()))?;

        // Send shutdown signals to all partition tasks (fire-and-forget).
        let handles = std::mem::take(&mut self.partition_handles);
        for handle in handles {
            let _ = handle.shutdown_tx.send(());
            // JoinHandles are dropped — tasks will complete asynchronously.
        }

        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: self.id.clone(),
                target_state: ExecutionState::Terminated,
            })
            .map_err(|e| self.make_error("compositor", "shutdown", e.to_string()))?;

        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        SupervisoryCompositor::contribute_state(self)
    }

    fn load_state(&mut self, state: toml::Value) -> Result<(), PartitionError> {
        // Restore state to the output store. Note: this cannot restart partition
        // tasks from saved state — live hot-reload requires additional infrastructure.
        if let Some(table) = state.as_table() {
            let mut store = self.output_store.lock().unwrap();
            for (id, value) in table {
                // Unwrap StateContribution envelope if present
                let inner = if let Some(sc) = StateContribution::from_toml(value) {
                    sc.state
                } else {
                    return Err(self.make_error(
                        id,
                        "load_state",
                        format!(
                            "partition '{}' state is not a valid StateContribution envelope \
                             (missing state/fresh/age_ms fields)",
                            id
                        ),
                    ));
                };
                store.insert(
                    id.clone(),
                    FreshnessEntry {
                        output: PartitionOutput::State(inner),
                        updated_at: Instant::now(),
                        tick: 0,
                    },
                );
            }
        }
        Ok(())
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}
