//! Compositor runtime: assembles partitions, bus, state machine, and double buffer (FPA-009).
//!
//! The compositor owns all partitions and orchestrates their lifecycle. It
//! integrates fault handling (FPA-011) and the shared state machine (FPA-006).

use std::collections::HashMap;

use fpa_bus::{BusExt, InProcessBus};
use fpa_contract::{Partition, PartitionError};
use fpa_events::EventEngine;

// Re-export SharedContext so downstream code importing from compositor::SharedContext still works.
pub use fpa_contract::SharedContext;

use crate::direct_signal::{DirectSignal, DirectSignalRegistry};
use crate::double_buffer::DoubleBuffer;
use crate::fault;
use crate::multi_rate::RateConfig;
use crate::state_machine::{ExecutionState, StateMachine, TransitionRequest};

/// Relay policy controlling how inner transition requests are forwarded
/// to the outer layer (FPA-010).
pub enum RelayPolicy {
    /// Forward the request unchanged.
    Forward,
    /// Transform the request before forwarding.
    Transform(Box<dyn Fn(TransitionRequest) -> TransitionRequest + Send>),
    /// Suppress the request (do not forward).
    Suppress,
    /// Aggregate: collect requests and forward a single summary.
    Aggregate,
}

/// The compositor assembles and manages a set of partitions.
///
/// It owns the bus, state machine, and double buffer. Each tick follows
/// the three-phase lifecycle: swap buffers, step partitions, collect outputs.
/// Fault handling wraps every partition call.
pub struct Compositor {
    /// Unique identifier for this compositor instance.
    id: String,
    partitions: Vec<Box<dyn Partition>>,
    bus: InProcessBus,
    state_machine: StateMachine,
    double_buffer: DoubleBuffer,
    /// Fallback partitions keyed by the ID of the partition they replace.
    fallbacks: HashMap<String, Box<dyn Partition>>,
    tick_count: u64,
    /// Accumulated simulation time (sum of all dt values passed to run_tick).
    elapsed_time: f64,
    /// Optional event engine for evaluating system-level events.
    event_engine: Option<EventEngine>,
    /// Action IDs triggered during the last tick's event evaluation.
    last_triggered_actions: Vec<String>,
    /// The layer depth of this compositor in a nested hierarchy.
    layer_depth: u32,
    /// Multi-rate scheduling configuration.
    rate_config: RateConfig,
    /// Relay policy for inter-layer transition request forwarding (FPA-010).
    relay_policy: RelayPolicy,
    /// Transition requests collected from inner partitions during the current tick.
    pending_requests: Vec<TransitionRequest>,
    /// Registry of allowed direct signal IDs (FPA-013).
    direct_signal_registry: DirectSignalRegistry,
    /// Direct signals emitted during operation (FPA-013).
    emitted_signals: Vec<DirectSignal>,
}

impl Compositor {
    /// Create a new compositor with the given partitions and bus.
    pub fn new(partitions: Vec<Box<dyn Partition>>, bus: InProcessBus) -> Self {
        Self {
            id: "compositor".to_string(),
            partitions,
            bus,
            state_machine: StateMachine::new(),
            double_buffer: DoubleBuffer::new(),
            fallbacks: HashMap::new(),
            tick_count: 0,
            elapsed_time: 0.0,
            event_engine: None,
            last_triggered_actions: Vec::new(),
            layer_depth: 0,
            rate_config: RateConfig::new(),
            relay_policy: RelayPolicy::Forward,
            pending_requests: Vec::new(),
            direct_signal_registry: DirectSignalRegistry::new(),
            emitted_signals: Vec::new(),
        }
    }

    /// Set the compositor ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Get the compositor ID.
    pub fn compositor_id(&self) -> &str {
        &self.id
    }

    /// Set the layer depth for this compositor.
    ///
    /// Layer depth is attached to any `PartitionError` produced by this
    /// compositor, enabling callers to identify which layer faulted.
    pub fn with_layer_depth(mut self, depth: u32) -> Self {
        self.layer_depth = depth;
        self
    }

    /// Get the layer depth of this compositor.
    pub fn layer_depth(&self) -> u32 {
        self.layer_depth
    }

    /// Set the multi-rate scheduling configuration.
    ///
    /// Partitions with a rate multiplier > 1 will be stepped multiple times
    /// per outer tick with proportionally smaller dt.
    pub fn set_rate_config(&mut self, config: RateConfig) {
        self.rate_config = config;
    }

    /// Set the relay policy for this compositor (FPA-010).
    pub fn with_relay_policy(mut self, policy: RelayPolicy) -> Self {
        self.relay_policy = policy;
        self
    }

    /// Register a direct signal ID as allowed (FPA-013).
    pub fn register_direct_signal(&mut self, signal_id: impl Into<String>) {
        self.direct_signal_registry.register(signal_id);
    }

    /// Get a reference to the direct signal registry.
    pub fn direct_signal_registry(&self) -> &DirectSignalRegistry {
        &self.direct_signal_registry
    }

    /// Emit a direct signal, bypassing the relay chain (FPA-013).
    ///
    /// The signal must be registered in the direct signal registry.
    /// Returns an error if the signal ID is not registered.
    pub fn emit_direct_signal(
        &mut self,
        signal_id: impl Into<String>,
        reason: impl Into<String>,
        emitter_identity: impl Into<String>,
    ) -> Result<(), PartitionError> {
        let signal_id = signal_id.into();
        let id_clone = self.id.clone();
        if !self.direct_signal_registry.is_registered(&signal_id) {
            return Err(self.make_error(
                &id_clone,
                "emit_direct_signal",
                format!("signal '{}' is not registered", signal_id),
            ));
        }
        let signal = DirectSignal::new(
            signal_id,
            reason,
            emitter_identity,
            self.layer_depth,
        );
        self.emitted_signals.push(signal);
        Ok(())
    }

    /// Get the list of emitted direct signals.
    pub fn emitted_signals(&self) -> &[DirectSignal] {
        &self.emitted_signals
    }

    /// Clear emitted direct signals (typically called after they've been consumed).
    pub fn clear_emitted_signals(&mut self) {
        self.emitted_signals.clear();
    }

    /// Submit a transition request from an inner partition (FPA-010).
    ///
    /// The request is subject to the compositor's relay policy before
    /// being forwarded to the outer layer.
    pub fn submit_inner_request(&mut self, request: TransitionRequest) {
        self.pending_requests.push(request);
    }

    /// Drain pending transition requests, applying the relay policy (FPA-010).
    ///
    /// Returns the requests that should be forwarded to the outer layer.
    pub fn drain_relayed_requests(&mut self) -> Vec<TransitionRequest> {
        let requests = std::mem::take(&mut self.pending_requests);
        match &self.relay_policy {
            RelayPolicy::Forward => requests,
            RelayPolicy::Transform(f) => requests.into_iter().map(|r| f(r)).collect(),
            RelayPolicy::Suppress => Vec::new(),
            RelayPolicy::Aggregate => {
                if requests.is_empty() {
                    Vec::new()
                } else {
                    let requesters: Vec<String> =
                        requests.iter().map(|r| r.requested_by.clone()).collect();
                    let target = requests.last().unwrap().target_state;
                    vec![TransitionRequest {
                        requested_by: format!("aggregated({})", requesters.join(",")),
                        target_state: target,
                    }]
                }
            }
        }
    }

    /// Get the pending (not yet relayed) inner requests.
    pub fn pending_requests(&self) -> &[TransitionRequest] {
        &self.pending_requests
    }

    /// Register a fallback partition for the given partition ID.
    ///
    /// If the primary partition faults during step(), the fallback will be
    /// activated in its place and the compositor will continue without error.
    ///
    /// # Panics
    /// Panics if the fallback's `id()` does not match `partition_id`.
    pub fn register_fallback(&mut self, partition_id: impl Into<String>, fallback: Box<dyn Partition>) {
        let partition_id = partition_id.into();
        assert_eq!(
            fallback.id(),
            partition_id,
            "fallback id '{}' must match partition id '{}'",
            fallback.id(),
            partition_id
        );
        self.fallbacks.insert(partition_id, fallback);
    }

    /// Get the current execution state.
    pub fn state(&self) -> ExecutionState {
        self.state_machine.state()
    }

    /// Get a reference to the state machine.
    pub fn state_machine(&self) -> &StateMachine {
        &self.state_machine
    }

    /// Get the current tick count.
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Get a reference to the bus.
    pub fn bus(&self) -> &InProcessBus {
        &self.bus
    }

    /// Get a reference to the double buffer.
    pub fn buffer(&self) -> &DoubleBuffer {
        &self.double_buffer
    }

    /// Initialize all partitions and transition to Running.
    ///
    /// Transitions: Uninitialized -> Initializing -> Running.
    /// If any partition fails init, transitions to Error.
    pub fn init(&mut self) -> Result<(), PartitionError> {
        // Transition to Initializing
        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: "compositor".to_string(),
                target_state: ExecutionState::Initializing,
            })
            .map_err(|e| self.make_error("compositor", "init", e.to_string()))?;

        // Initialize all partitions with fault handling
        for partition in &mut self.partitions {
            let result = fault::safe_init(partition.as_mut());
            if let Err(e) = result.into_result() {
                self.state_machine.force_state(ExecutionState::Error);
                return Err(e.with_layer_depth(self.layer_depth));
            }
        }

        // Initialize fallbacks too
        for fallback in self.fallbacks.values_mut() {
            let result = fault::safe_init(fallback.as_mut());
            if let Err(e) = result.into_result() {
                self.state_machine.force_state(ExecutionState::Error);
                return Err(e.with_layer_depth(self.layer_depth));
            }
        }

        // Transition to Running
        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: "compositor".to_string(),
                target_state: ExecutionState::Running,
            })
            .map_err(|e| self.make_error("compositor", "init", e.to_string()))?;

        Ok(())
    }

    /// Run one tick of the compositor lifecycle.
    ///
    /// Three phases:
    /// 1. Swap buffers (write -> read, clear new write)
    /// 2. Step each partition with fault handling
    /// 3. Collect outputs via contribute_state
    pub fn run_tick(&mut self, dt: f64) -> Result<(), PartitionError> {
        if self.state_machine.state() != ExecutionState::Running {
            return Err(self.make_error(
                "compositor",
                "run_tick",
                format!(
                    "cannot run tick in state {}",
                    self.state_machine.state()
                ),
            ));
        }

        self.tick_count += 1;
        self.elapsed_time += dt;

        // Phase 1: swap buffers
        self.double_buffer.swap();

        // Phase 2 & 3: step each partition and collect outputs.
        // For multi-rate partitions, each partition steps `rate` times per outer
        // tick with `dt / rate` per sub-step. If a partition faults mid-cycle and
        // a fallback is registered, the fallback completes the remaining sub-steps.
        let mut i = 0;
        while i < self.partitions.len() {
            let partition_id = self.partitions[i].id().to_string();
            let rate = self.rate_config.get_rate(&partition_id);
            let sub_dt = dt / rate as f64;

            for sub in 0..rate {
                let step_result = fault::safe_step(self.partitions[i].as_mut(), sub_dt);

                if let Err(step_err) = step_result.into_result() {
                    // Check for fallback
                    if let Some(mut fallback) = self.fallbacks.remove(&partition_id) {
                        // Step the fallback for the failed sub-step
                        let fallback_result = fault::safe_step(fallback.as_mut(), sub_dt);
                        if let Err(fallback_err) = fallback_result.into_result() {
                            self.state_machine.force_state(ExecutionState::Error);
                            return Err(fallback_err.with_layer_depth(self.layer_depth));
                        }

                        // Replace the partition with the fallback
                        self.partitions[i] = fallback;

                        // Complete remaining sub-steps with the fallback
                        for _remaining in (sub + 1)..rate {
                            let r = fault::safe_step(self.partitions[i].as_mut(), sub_dt);
                            if let Err(e) = r.into_result() {
                                self.state_machine.force_state(ExecutionState::Error);
                                return Err(e.with_layer_depth(self.layer_depth));
                            }
                        }
                        break;
                    }

                    // No fallback - transition to Error and propagate
                    self.state_machine.force_state(ExecutionState::Error);
                    return Err(step_err.with_layer_depth(self.layer_depth));
                }
            }

            // Collect output after all sub-steps (including fallback's remaining steps)
            let state = fault::safe_contribute_state(self.partitions[i].as_ref())?;
            self.double_buffer.write(&partition_id, state);

            i += 1;
        }

        // Publish aggregated shared context on the bus.
        // Collects all partition states from the write buffer into a single
        // TOML table and publishes it as a SharedContext message.
        //
        // Design note (FPA-009 multi-rate): SharedContext is published once per
        // outer tick, not after each sub-step. During multi-rate sub-stepping the
        // double buffer write slot is overwritten, but a bus publication only
        // occurs here — after all partitions have completed all their sub-steps.
        // Publishing per sub-step would expose intermediate states that don't
        // correspond to a consistent system snapshot.
        {
            let mut table = toml::map::Map::new();
            for (id, value) in self.double_buffer.write_all() {
                table.insert(id.clone(), value.clone());
            }
            self.bus.publish(SharedContext {
                state: toml::Value::Table(table),
                tick: self.tick_count,
            });
        }

        // Phase 3: evaluate events against pre-step state (snapshot semantics)
        // The read buffer contains the previous tick's state (set during Phase 1 swap),
        // which is the pre-step snapshot. Events are evaluated against this snapshot
        // so that event conditions see a consistent, pre-mutation view.
        self.last_triggered_actions.clear();
        if let Some(ref engine) = self.event_engine {
            let signals = self.build_signals();
            let current_time = self.elapsed_time;
            let triggered = engine.evaluate(current_time, &signals);
            self.last_triggered_actions = triggered
                .iter()
                .map(|action| action.action_id.clone())
                .collect();
        }

        // Collect direct signals from any inner compositor partitions (FPA-013).
        self.collect_inner_signals();

        Ok(())
    }

    /// Shut down all partitions and transition to Terminated.
    ///
    /// Transitions: Running (or Paused) -> ShuttingDown -> Terminated.
    pub fn shutdown(&mut self) -> Result<(), PartitionError> {
        // Transition to ShuttingDown
        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: "compositor".to_string(),
                target_state: ExecutionState::ShuttingDown,
            })
            .map_err(|e| self.make_error("compositor", "shutdown", e.to_string()))?;

        // Shut down all partitions
        let mut last_error = None;
        for partition in &mut self.partitions {
            let result = fault::safe_shutdown(partition.as_mut());
            if let Err(e) = result.into_result() {
                last_error = Some(e.with_layer_depth(self.layer_depth));
            }
        }

        if let Some(e) = last_error {
            self.state_machine.force_state(ExecutionState::Error);
            return Err(e);
        }

        // Transition to Terminated
        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: "compositor".to_string(),
                target_state: ExecutionState::Terminated,
            })
            .map_err(|e| self.make_error("compositor", "shutdown", e.to_string()))?;

        Ok(())
    }

    /// Process a transition request (e.g., received via bus).
    pub fn process_transition_request(
        &mut self,
        request: TransitionRequest,
    ) -> Result<ExecutionState, PartitionError> {
        self.state_machine
            .request_transition(request)
            .map_err(|e| self.make_error("compositor", "transition", e.to_string()))
    }

    /// Get a reference to the partition list.
    pub fn partitions(&self) -> &[Box<dyn Partition>] {
        &self.partitions
    }

    /// Set the event engine for system-level event evaluation.
    pub fn set_event_engine(&mut self, engine: EventEngine) {
        self.event_engine = Some(engine);
    }

    /// Get the action IDs triggered during the last tick's event evaluation.
    pub fn last_triggered_actions(&self) -> &[String] {
        &self.last_triggered_actions
    }

    /// Create a `PartitionError` with this compositor's layer depth attached.
    fn make_error(&self, partition_id: &str, operation: &str, message: String) -> PartitionError {
        PartitionError::new(partition_id, operation, message)
            .with_layer_depth(self.layer_depth)
    }

    /// Collect direct signals from inner compositor partitions (FPA-013).
    ///
    /// After stepping, any partition that is itself a `Compositor` may have
    /// emitted direct signals. This method drains those signals and extends
    /// the current compositor's `emitted_signals`, enabling signal propagation
    /// through nested layers.
    fn collect_inner_signals(&mut self) {
        for partition in &mut self.partitions {
            if let Some(any) = partition.as_any_mut() {
                if let Some(inner_comp) = any.downcast_mut::<Compositor>() {
                    let signals = std::mem::take(&mut inner_comp.emitted_signals);
                    self.emitted_signals.extend(signals);
                }
            }
        }
    }

    /// Build a signal map from the read buffer (pre-step snapshot).
    ///
    /// Extracts numeric values from partition state tables, using the format
    /// `partition_id.key` as the signal name.
    fn build_signals(&self) -> HashMap<String, f64> {
        let mut signals = HashMap::new();
        for (id, value) in self.double_buffer.read_all() {
            if let Some(table) = value.as_table() {
                for (key, val) in table {
                    if let Some(f) = val.as_float() {
                        signals.insert(format!("{}.{}", id, key), f);
                    } else if let Some(i) = val.as_integer() {
                        signals.insert(format!("{}.{}", id, key), i as f64);
                    }
                }
            }
        }
        signals
    }

    /// Dump the current state as a TOML composition fragment (FPA-022, FPA-023).
    pub fn dump(&self) -> Result<toml::Value, PartitionError> {
        let mut partitions = toml::map::Map::new();
        for partition in &self.partitions {
            let state = fault::safe_contribute_state(partition.as_ref())?;
            partitions.insert(partition.id().to_string(), state);
        }
        let mut root = toml::map::Map::new();
        root.insert("partitions".to_string(), toml::Value::Table(partitions));
        // Include system state
        let mut system = toml::map::Map::new();
        let tick_count = i64::try_from(self.tick_count).map_err(|_| {
            self.make_error("compositor", "dump", "tick_count exceeds i64::MAX".to_string())
        })?;
        system.insert(
            "tick_count".to_string(),
            toml::Value::Integer(tick_count),
        );
        root.insert("system".to_string(), toml::Value::Table(system));
        Ok(toml::Value::Table(root))
    }

    /// Load state from a TOML composition fragment (FPA-022, FPA-023).
    pub fn load(&mut self, fragment: toml::Value) -> Result<(), PartitionError> {
        // Extract partitions section
        if let Some(partitions) = fragment.get("partitions").and_then(|v| v.as_table()) {
            for partition in &mut self.partitions {
                if let Some(state) = partitions.get(partition.id()) {
                    fault::safe_load_state(partition.as_mut(), state.clone())
                        .into_result()
                        .map_err(|e| e.with_layer_depth(self.layer_depth))?;
                }
            }
        }
        // Restore system state
        if let Some(system) = fragment.get("system").and_then(|v| v.as_table()) {
            if let Some(tc) = system.get("tick_count").and_then(|v| v.as_integer()) {
                self.tick_count = u64::try_from(tc).map_err(|_| {
                    self.make_error("compositor", "load", "tick_count is negative".to_string())
                })?;
            }
        }
        Ok(())
    }
}

/// Implement `Partition` for `Compositor`, enabling vertical composition (FPA-001).
///
/// A compositor can itself be a partition inside an outer compositor,
/// creating the fractal nesting that gives FPA its name.
impl Partition for Compositor {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        Compositor::init(self)
    }

    fn step(&mut self, dt: f64) -> Result<(), PartitionError> {
        self.run_tick(dt)
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        Compositor::shutdown(self)
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        self.dump()
    }

    fn load_state(&mut self, state: toml::Value) -> Result<(), PartitionError> {
        self.load(state)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}
