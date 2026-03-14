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
use crate::state_machine::{ExecutionState, StateMachine, TransitionRequest};

/// An entry in the output store with freshness tracking.
#[derive(Debug, Clone)]
pub struct FreshnessEntry {
    /// The partition's contributed state value.
    pub value: toml::Value,
    /// When this entry was last updated.
    pub updated_at: Instant,
    /// The tick (step count) when this entry was produced.
    pub tick: u64,
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
    bus: Box<dyn Bus>,
    state_machine: StateMachine,
    output_store: Arc<Mutex<HashMap<String, FreshnessEntry>>>,
    heartbeat_timeout: Duration,
    layer_depth: u32,
    /// Partitions waiting to be spawned (consumed during init).
    pending_partitions: Vec<Box<dyn Partition>>,
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
    /// Accepts any `Bus` implementation via `Box<dyn Bus>`, enabling runtime
    /// transport selection (FPA-004). For convenience with `InProcessBus`,
    /// use `SupervisoryCompositor::new_default`.
    pub fn new(
        id: impl Into<String>,
        partitions: Vec<Box<dyn Partition>>,
        bus: Box<dyn Bus>,
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
        Self::new(id, partitions, Box::new(InProcessBus::new(bus_id)), heartbeat_timeout)
    }

    /// Set the layer depth for this compositor.
    pub fn with_layer_depth(mut self, depth: u32) -> Self {
        self.layer_depth = depth;
        self
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

    /// Initialize the supervisory compositor: spawn each partition as a task.
    ///
    /// Each partition is moved into its own tokio task that runs:
    /// 1. `partition.init()`
    /// 2. Loop: `partition.step(dt)`, `partition.contribute_state()`, write to store
    /// 3. On shutdown signal: `partition.shutdown()`
    pub fn init(&mut self) -> Result<(), PartitionError> {
        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: self.id.clone(),
                target_state: ExecutionState::Initializing,
            })
            .map_err(|e| self.make_error("compositor", "init", e.to_string()))?;

        let partitions = std::mem::take(&mut self.pending_partitions);

        for mut partition in partitions {
            let partition_id = partition.id().to_string();
            let store = Arc::clone(&self.output_store);
            let signals = Arc::clone(&self.emitted_signals);
            let step_interval = self
                .partition_intervals
                .get(&partition_id)
                .copied()
                .unwrap_or(self.step_interval);
            let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

            let join_handle = tokio::spawn(async move {
                // Initialize
                if let Err(e) = partition.init() {
                    // Report init error to the output store
                    let mut s = store.lock().unwrap();
                    let mut error_table = toml::map::Map::new();
                    error_table.insert(
                        "error".to_string(),
                        toml::Value::String(e.message.clone()),
                    );
                    error_table.insert(
                        "operation".to_string(),
                        toml::Value::String("init".to_string()),
                    );
                    s.insert(
                        partition.id().to_string(),
                        FreshnessEntry {
                            value: toml::Value::Table(error_table),
                            updated_at: Instant::now(),
                            tick: 0,
                        },
                    );
                    return;
                }

                let mut tick: u64 = 0;
                let dt = step_interval.as_secs_f64();

                loop {
                    // Check for shutdown signal (non-blocking)
                    match shutdown_rx.try_recv() {
                        Ok(()) | Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                            let _ = partition.shutdown();
                            break;
                        }
                        Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                    }

                    // Step
                    if let Err(e) = partition.step(dt) {
                        // Report step error to the output store
                        let mut s = store.lock().unwrap();
                        let mut error_table = toml::map::Map::new();
                        error_table.insert(
                            "error".to_string(),
                            toml::Value::String(e.message.clone()),
                        );
                        error_table.insert(
                            "operation".to_string(),
                            toml::Value::String("step".to_string()),
                        );
                        s.insert(
                            partition.id().to_string(),
                            FreshnessEntry {
                                value: toml::Value::Table(error_table),
                                updated_at: Instant::now(),
                                tick,
                            },
                        );
                        break;
                    }
                    tick += 1;

                    // Collect direct signals from inner compositor partitions (FPA-013)
                    if let Some(any) = partition.as_any_mut() {
                        use crate::compositor::Compositor;
                        if let Some(inner_comp) = any.downcast_mut::<Compositor>() {
                            let drained = inner_comp.drain_emitted_signals();
                            if !drained.is_empty() {
                                signals.lock().unwrap().extend(drained);
                            }
                        }
                    }

                    // Contribute state
                    if let Ok(value) = partition.contribute_state() {
                        let mut s = store.lock().unwrap();
                        s.insert(
                            partition.id().to_string(),
                            FreshnessEntry {
                                value,
                                updated_at: Instant::now(),
                                tick,
                            },
                        );
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

        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: self.id.clone(),
                target_state: ExecutionState::Running,
            })
            .map_err(|e| self.make_error("compositor", "init", e.to_string()))?;

        Ok(())
    }

    /// Read latest state from the output store, check heartbeat freshness,
    /// and publish aggregated state on the bus.
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
        let mut table = toml::map::Map::new();

        for (id, entry) in store.iter() {
            let age = now.duration_since(entry.updated_at);
            let fresh = age < self.heartbeat_timeout;
            let age_ms = age.as_millis() as u64;

            let envelope = StateContribution {
                state: entry.value.clone(),
                fresh,
                age_ms,
            };
            table.insert(id.clone(), envelope.to_toml());
        }

        drop(store);

        self.bus.publish(SharedContext {
            state: toml::Value::Table(table),
            tick: self.tick_count,
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
    /// stopped before returning. For a sync-compatible version, use the
    /// `Partition::shutdown()` trait implementation which sends signals without
    /// awaiting completion.
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
            let _ = handle.join_handle.await;
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
    pub fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        let now = Instant::now();
        let store = self.output_store.lock().unwrap();
        let mut table = toml::map::Map::new();

        for (id, entry) in store.iter() {
            let age = now.duration_since(entry.updated_at);
            let fresh = age < self.heartbeat_timeout;
            let age_ms = age.as_millis() as u64;

            let envelope = StateContribution {
                state: entry.value.clone(),
                fresh,
                age_ms,
            };
            table.insert(id.clone(), envelope.to_toml());
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
/// creating fractal nesting. Note that `shutdown()` is synchronous here: it sends
/// shutdown signals but does not await task completion. Use `async_shutdown()` for
/// graceful async shutdown with join.
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
        // See docs/feedback/phase4analysis.md F5 for the full spec finding.
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
                    value.clone()
                };
                store.insert(
                    id.clone(),
                    FreshnessEntry {
                        value: inner,
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
