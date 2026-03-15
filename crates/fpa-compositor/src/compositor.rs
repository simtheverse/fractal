//! Compositor runtime: assembles partitions, bus, state machine, and double buffer (FPA-009).
//!
//! The compositor owns all partitions and orchestrates their lifecycle. It
//! integrates fault handling (FPA-011) and the shared state machine (FPA-006).

use std::collections::HashMap;
use std::sync::Arc;

use fpa_bus::{Bus, BusExt, BusReader, DeferredBus, InProcessBus, TypedReader};
use fpa_contract::{DumpRequest, LoadRequest, Partition, PartitionError, StateContribution};
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

/// A pending lifecycle operation to be processed in Phase 1 (FPA-014).
pub enum LifecycleOp {
    /// Add a new partition to the compositor.
    Spawn(Box<dyn Partition>),
    /// Remove a partition by ID.
    Despawn(String),
}

/// The compositor assembles and manages a set of partitions.
///
/// It owns the bus, state machine, and double buffer. Each tick follows
/// the three-phase lifecycle defined in FPA-014:
/// - Phase 1: direct signal check, lifecycle ops, dump/load, buffer swap
/// - Phase 2: step partitions with direct signal checks between each;
///   shared context assembled after tick barrier
/// - Phase 3: event evaluation, request processing, final signal check
/// Fault handling wraps every partition call.
pub struct Compositor {
    /// Unique identifier for this compositor instance.
    id: String,
    partitions: Vec<Box<dyn Partition>>,
    bus: Arc<DeferredBus>,
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
    /// Fault handling timeout configuration (FPA-011).
    timeout_config: fault::TimeoutConfig,
    /// Relay policy for inter-layer transition request forwarding (FPA-010).
    relay_policy: RelayPolicy,
    /// Transition requests collected from inner partitions during the current tick.
    pending_requests: Vec<TransitionRequest>,
    /// Registry of allowed direct signal IDs (FPA-013).
    direct_signal_registry: DirectSignalRegistry,
    /// Direct signals emitted during operation (FPA-013).
    emitted_signals: Vec<DirectSignal>,
    /// Pending lifecycle operations queued for Phase 1 processing (FPA-014).
    pending_lifecycle_ops: Vec<LifecycleOp>,
    /// Pending dump request flag for Phase 1 processing (FPA-014, FPA-023).
    pending_dump: bool,
    /// Last dump result produced during Phase 1 processing.
    last_dump_result: Option<toml::Value>,
    /// Pending load request for Phase 1 processing (FPA-014, FPA-023).
    pending_load: Option<toml::Value>,
    /// Bus reader for transition requests (FPA-006).
    transition_reader: TypedReader<TransitionRequest>,
    /// Bus reader for dump requests (FPA-023).
    dump_reader: TypedReader<DumpRequest>,
    /// Bus reader for load requests (FPA-023).
    load_reader: TypedReader<LoadRequest>,
}

impl Compositor {
    /// Create a new compositor with the given partitions and bus.
    ///
    /// Wraps the bus in a `DeferredBus` internally. Partitions that were
    /// constructed with the original `bus` will publish directly to it,
    /// bypassing deferred mode — this is fine for partitions that don't
    /// publish bus messages during `step()` (Counter, Accumulator, Doubler).
    ///
    /// For partitions that publish during `step()`, use `compose()` or
    /// `from_deferred_bus()` so partitions hold the `DeferredBus` and
    /// FPA-014 intra-tick isolation is enforced.
    pub fn new(partitions: Vec<Box<dyn Partition>>, bus: Arc<dyn Bus>) -> Self {
        Self::from_deferred_bus(partitions, Arc::new(DeferredBus::new(bus)))
    }

    /// Create a new compositor with a pre-constructed `DeferredBus`.
    ///
    /// Use this when partitions need to hold the same `DeferredBus` instance
    /// (e.g., when created via `compose()` or in tests where partitions
    /// publish bus messages during `step()`).
    pub fn from_deferred_bus(partitions: Vec<Box<dyn Partition>>, bus: Arc<DeferredBus>) -> Self {
        let transition_reader = bus.subscribe::<TransitionRequest>();
        let dump_reader = bus.subscribe::<DumpRequest>();
        let load_reader = bus.subscribe::<LoadRequest>();
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
            timeout_config: fault::TimeoutConfig::default(),
            relay_policy: RelayPolicy::Forward,
            pending_requests: Vec::new(),
            direct_signal_registry: DirectSignalRegistry::new(),
            emitted_signals: Vec::new(),
            pending_lifecycle_ops: Vec::new(),
            pending_dump: false,
            last_dump_result: None,
            pending_load: None,
            transition_reader,
            dump_reader,
            load_reader,
        }
    }

    /// Create a new compositor with a default `InProcessBus`.
    ///
    /// Convenience constructor for the common case where in-process transport
    /// is sufficient.
    pub fn new_default(partitions: Vec<Box<dyn Partition>>, bus_id: impl Into<String>) -> Self {
        Self::new(partitions, Arc::new(InProcessBus::new(bus_id)))
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

    /// Set the fault handling timeout configuration (FPA-011).
    ///
    /// Each domain sets timeouts appropriate to its timing constraints.
    /// Defaults: 50ms for step/contribute_state, 500ms for init/shutdown/load.
    pub fn set_timeout_config(&mut self, config: fault::TimeoutConfig) {
        self.timeout_config = config;
    }

    /// Get the current timeout configuration.
    pub fn timeout_config(&self) -> &fault::TimeoutConfig {
        &self.timeout_config
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

    /// Drain and return all emitted direct signals, leaving the internal list empty.
    pub fn drain_emitted_signals(&mut self) -> Vec<DirectSignal> {
        std::mem::take(&mut self.emitted_signals)
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
    /// Returns an error if the fallback's `id()` does not match `partition_id`.
    pub fn register_fallback(
        &mut self,
        partition_id: impl Into<String>,
        fallback: Box<dyn Partition>,
    ) -> Result<(), PartitionError> {
        let partition_id = partition_id.into();
        if fallback.id() != partition_id {
            return Err(self.make_error(
                &partition_id,
                "register_fallback",
                format!(
                    "fallback id '{}' must match partition id '{}'",
                    fallback.id(),
                    partition_id,
                ),
            ));
        }
        self.fallbacks.insert(partition_id, fallback);
        Ok(())
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
    pub fn bus(&self) -> &dyn Bus {
        &*self.bus
    }

    /// Get a shared reference to the deferred bus.
    pub fn bus_arc(&self) -> &Arc<DeferredBus> {
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
            let result = fault::safe_init(partition.as_mut(), &self.timeout_config);
            if let Err(e) = result.into_result() {
                self.state_machine.force_state(ExecutionState::Error);
                return Err(e.with_layer_depth(self.layer_depth));
            }
        }

        // Initialize fallbacks too
        for fallback in self.fallbacks.values_mut() {
            let result = fault::safe_init(fallback.as_mut(), &self.timeout_config);
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

    /// Queue a lifecycle operation for processing in the next tick's Phase 1 (FPA-014).
    pub fn request_lifecycle_op(&mut self, op: LifecycleOp) {
        self.pending_lifecycle_ops.push(op);
    }

    /// Queue a dump request for processing in the next tick's Phase 1 (FPA-014, FPA-023).
    pub fn request_dump(&mut self) {
        self.pending_dump = true;
    }

    /// Retrieve the last dump result produced during Phase 1 processing.
    pub fn take_dump_result(&mut self) -> Option<toml::Value> {
        self.last_dump_result.take()
    }

    /// Queue a load request for processing in the next tick's Phase 1 (FPA-014, FPA-023).
    pub fn request_load(&mut self, fragment: toml::Value) {
        self.pending_load = Some(fragment);
    }

    /// Run one tick of the compositor lifecycle (FPA-014).
    ///
    /// Three phases per tick:
    /// - Phase 1: Check direct signals, process lifecycle ops, process
    ///   dump/load requests, swap buffers
    /// - Phase 2: Step each partition with fault handling; check direct
    ///   signals between each partition step; assemble shared context
    ///   after all partitions complete (tick barrier)
    /// - Phase 3: Evaluate events against pre-step state, collect outputs,
    ///   process bus requests, check direct signals
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

        // === Phase 1: Pre-tick processing (FPA-014) ===
        // Tick counters are incremented after Phase 1 so that dump requests
        // produce snapshots with metadata matching the last completed tick.

        // Step 1: Check for pending direct signals and process them.
        self.collect_inner_signals();

        // Step 2: Process pending lifecycle operations (spawn/despawn).
        self.process_lifecycle_ops()?;

        // Step 3: Process pending dump and load requests.
        self.process_pending_dump_load()?;

        // Advance tick counters now that Phase 1 is complete.
        self.tick_count += 1;
        self.elapsed_time += dt;

        // Step 4: Swap the read/write buffers.
        self.double_buffer.swap();

        // === Phase 2: Partition stepping (FPA-014) ===
        // Enable deferred mode so bus messages published during step() are
        // queued rather than immediately delivered. This ensures no partition
        // sees another partition's current-tick bus messages — the same
        // isolation guarantee the double buffer provides for SharedContext.
        //
        // Deferred mode is always restored on exit from Phase 2 — including
        // fault paths — so the bus is never left in deferred state.
        self.bus.set_deferred(true);
        let phase2_result = self.run_phase2_stepping(dt);
        self.bus.set_deferred(false);
        self.bus.flush();
        phase2_result?;

        // Assemble shared context from current tick's partition outputs
        // and publish on the bus (FPA-014: after the tick barrier, before
        // Phase 3). SharedContext reflects the complete, consistent state
        // of all partitions after stepping. Published with deferred mode
        // off, so it goes directly to the inner bus.
        {
            let mut table = toml::map::Map::new();
            for (id, value) in self.double_buffer.write_all() {
                table.insert(id.clone(), value.clone());
            }
            self.bus.publish(SharedContext {
                state: toml::Value::Table(table),
                tick: self.tick_count,
                execution_state: self.state_machine.state(),
            });
        }

        // === Phase 3: Post-tick processing (FPA-014) ===

        // Step 1: Evaluate events against pre-step state (snapshot semantics).
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

        // Step 2: Process bus-mediated transition requests (FPA-006).
        for request in self.transition_reader.read_all() {
            self.process_transition_request(request)?;
        }

        // Step 3: Check for pending direct signals.
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
            let result = fault::safe_shutdown(partition.as_mut(), &self.timeout_config);
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

    /// Pause the compositor. Transitions Running -> Paused.
    ///
    /// Load operations require the compositor to be in Paused state (FPA-023).
    pub fn pause(&mut self) -> Result<(), PartitionError> {
        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: "compositor".to_string(),
                target_state: ExecutionState::Paused,
            })
            .map_err(|e| self.make_error("compositor", "pause", e.to_string()))?;
        Ok(())
    }

    /// Resume the compositor. Transitions Paused -> Running.
    pub fn resume(&mut self) -> Result<(), PartitionError> {
        self.state_machine
            .request_transition(TransitionRequest {
                requested_by: "compositor".to_string(),
                target_state: ExecutionState::Running,
            })
            .map_err(|e| self.make_error("compositor", "resume", e.to_string()))?;
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

    /// Process pending lifecycle operations (FPA-014 Phase 1 step 2).
    ///
    /// Spawned partitions are initialized and added. Despawned partitions are
    /// shut down and removed.
    fn process_lifecycle_ops(&mut self) -> Result<(), PartitionError> {
        let ops = std::mem::take(&mut self.pending_lifecycle_ops);
        for op in ops {
            match op {
                LifecycleOp::Spawn(mut partition) => {
                    let result = fault::safe_init(partition.as_mut(), &self.timeout_config);
                    if let Err(e) = result.into_result() {
                        self.state_machine.force_state(ExecutionState::Error);
                        return Err(e.with_layer_depth(self.layer_depth));
                    }
                    self.partitions.push(partition);
                }
                LifecycleOp::Despawn(id) => {
                    if let Some(pos) = self.partitions.iter().position(|p| p.id() == id) {
                        let mut removed = self.partitions.remove(pos);
                        // Shutdown error is intentionally discarded: the partition
                        // is being removed regardless, similar to Drop semantics.
                        // A failing shutdown should not prevent despawn or poison
                        // the compositor — the partition is already gone from the
                        // active set by this point.
                        let _ = fault::safe_shutdown(removed.as_mut(), &self.timeout_config);
                    }
                }
            }
        }
        Ok(())
    }

    /// Process pending dump and load requests (FPA-014 Phase 1 step 3, FPA-023).
    ///
    /// Drains bus-mediated DumpRequest/LoadRequest messages and merges them
    /// with programmatic requests. Dump invokes `contribute_state()` on all
    /// partitions using post-tick-N-1 state.
    ///
    /// Load requires the execution state machine to be in a non-processing
    /// state (FPA-023). Phase 1 transiently pauses the compositor before
    /// applying loads, then resumes — the same pattern used by
    /// `Partition::load_state()` for nested compositors.
    fn process_pending_dump_load(&mut self) -> Result<(), PartitionError> {
        // Drain bus-mediated dump requests
        if !self.dump_reader.read_all().is_empty() {
            self.pending_dump = true;
        }

        if self.pending_dump {
            self.pending_dump = false;
            self.last_dump_result = Some(self.dump()?);
        }

        // Collect all pending loads: bus-mediated first, then programmatic
        // (last applied wins, so the explicit API takes precedence).
        let bus_loads: Vec<_> = self.load_reader.read_all();
        let programmatic_load = self.pending_load.take();

        if bus_loads.is_empty() && programmatic_load.is_none() {
            return Ok(());
        }

        // Transiently pause to satisfy FPA-023's idle precondition, apply
        // all loads, then resume. No partition methods are called during
        // pause/resume, and SharedContext isn't published until Phase 2,
        // so the transient Paused state is not observable.
        self.pause()?;
        let result = (|| {
            for load_req in bus_loads {
                self.apply_state_fragment(load_req.fragment)?;
            }
            if let Some(fragment) = programmatic_load {
                self.apply_state_fragment(fragment)?;
            }
            Ok(())
        })();
        self.resume()?;
        result
    }

    /// Apply a state fragment to partitions and system counters.
    ///
    /// Shared implementation used by both `load()` (external API with state
    /// guard) and `process_pending_dump_load()` (Phase 1 internal path, which
    /// is already at an idle tick boundary by construction).
    fn apply_state_fragment(&mut self, fragment: toml::Value) -> Result<(), PartitionError> {
        if let Some(partitions) = fragment.get("partitions").and_then(|v| v.as_table()) {
            for partition in &mut self.partitions {
                if let Some(envelope_value) = partitions.get(partition.id()) {
                    let state = if let Some(sc) = StateContribution::from_toml(envelope_value) {
                        sc.state
                    } else {
                        let pid = partition.id().to_string();
                        return Err(PartitionError::new(
                            &pid,
                            "load",
                            format!(
                                "partition '{}' state is not a valid StateContribution envelope \
                                 (missing state/fresh/age_ms fields)",
                                pid
                            ),
                        ).with_layer_depth(self.layer_depth));
                    };
                    fault::safe_load_state(partition.as_mut(), state, &self.timeout_config)
                        .into_result()
                        .map_err(|e| e.with_layer_depth(self.layer_depth))?;

                    // Seed the write buffer with the loaded envelope so the
                    // next swap makes the loaded snapshot visible as the
                    // pre-step read buffer for event evaluation and signals.
                    self.double_buffer
                        .write(&partition.id().to_string(), envelope_value.clone());
                }
            }
        }
        if let Some(system) = fragment.get("system").and_then(|v| v.as_table()) {
            if let Some(tc) = system.get("tick_count").and_then(|v| v.as_integer()) {
                self.tick_count = u64::try_from(tc).map_err(|_| {
                    self.make_error("compositor", "load", "tick_count is negative".to_string())
                })?;
            }
            if let Some(et) = system.get("elapsed_time").and_then(|v| v.as_float()) {
                self.elapsed_time = et;
            }
        }
        Ok(())
    }

    /// Phase 2 stepping loop, extracted so `run_tick` can guarantee deferred
    /// mode cleanup regardless of fault paths (FPA-014, FPA-011).
    fn run_phase2_stepping(&mut self, dt: f64) -> Result<(), PartitionError> {
        // For multi-rate partitions, each partition steps `rate` times per outer
        // tick with `dt / rate` per sub-step. If a partition faults mid-cycle and
        // a fallback is registered, the fallback completes the remaining sub-steps.
        let mut i = 0;
        while i < self.partitions.len() {
            let partition_id = self.partitions[i].id().to_string();
            let rate = self.rate_config.get_rate(&partition_id);
            let sub_dt = dt / rate as f64;

            for sub in 0..rate {
                let step_result = fault::safe_step(self.partitions[i].as_mut(), sub_dt, &self.timeout_config);

                if let Err(step_err) = step_result.into_result() {
                    // Check for fallback
                    if let Some(mut fallback) = self.fallbacks.remove(&partition_id) {
                        // Step the fallback for the failed sub-step
                        let fallback_result = fault::safe_step(fallback.as_mut(), sub_dt, &self.timeout_config);
                        if let Err(fallback_err) = fallback_result.into_result() {
                            self.state_machine.force_state(ExecutionState::Error);
                            return Err(fallback_err.with_layer_depth(self.layer_depth));
                        }

                        // Replace the partition with the fallback
                        self.partitions[i] = fallback;

                        // Complete remaining sub-steps with the fallback
                        for _remaining in (sub + 1)..rate {
                            let r = fault::safe_step(self.partitions[i].as_mut(), sub_dt, &self.timeout_config);
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
            let state = fault::safe_contribute_state(self.partitions[i].as_ref(), &self.timeout_config)
                .map_err(|e| e.with_layer_depth(self.layer_depth))?;
            let envelope = StateContribution {
                state,
                fresh: true,
                age_ms: 0,
            };
            self.double_buffer.write(&partition_id, envelope.to_toml());

            // Check for pending direct signals between partition steps.
            self.collect_inner_signals();

            i += 1;
        }
        Ok(())
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
                } else if let Some(inner_sup) = any.downcast_mut::<crate::supervisory::SupervisoryCompositor>() {
                    let signals = inner_sup.drain_emitted_signals();
                    self.emitted_signals.extend(signals);
                }
            }
        }
    }

    /// Build a signal map from the read buffer (pre-step snapshot).
    ///
    /// Extracts numeric values from partition state tables, using the format
    /// `partition_id.key` as the signal name. Values are unwrapped from the
    /// `StateContribution` envelope — signals come from the inner `state` key.
    fn build_signals(&self) -> HashMap<String, f64> {
        let mut signals = HashMap::new();
        for (id, value) in self.double_buffer.read_all() {
            // Navigate the StateContribution envelope by reference to avoid
            // cloning the entire state tree. Validate the full envelope shape
            // (state + fresh bool + age_ms int) to avoid misinterpreting a
            // partition state that happens to contain a "state" field.
            let inner = value
                .as_table()
                .filter(|t| {
                    t.contains_key("state")
                        && t.get("fresh").is_some_and(|v| v.is_bool())
                        && t.get("age_ms").is_some_and(|v| v.is_integer())
                })
                .and_then(|t| t.get("state"))
                .unwrap_or(value);
            if let Some(table) = inner.as_table() {
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
            let state = fault::safe_contribute_state(partition.as_ref(), &self.timeout_config)
                .map_err(|e| e.with_layer_depth(self.layer_depth))?;
            let envelope = StateContribution {
                state,
                fresh: true,
                age_ms: 0,
            };
            partitions.insert(partition.id().to_string(), envelope.to_toml());
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
        system.insert(
            "elapsed_time".to_string(),
            toml::Value::Float(self.elapsed_time),
        );
        system.insert(
            "execution_state".to_string(),
            toml::Value::String(self.state_machine.state().to_string()),
        );
        root.insert("system".to_string(), toml::Value::Table(system));
        Ok(toml::Value::Table(root))
    }

    /// Load state from a TOML composition fragment (FPA-022, FPA-023).
    ///
    /// Load is only valid when processing is idle (FPA-023): no partition
    /// lifecycle methods are in flight AND the execution state machine is in a
    /// non-processing state (Paused or Uninitialized).
    pub fn load(&mut self, fragment: toml::Value) -> Result<(), PartitionError> {
        let state = self.state_machine.state();
        if state != ExecutionState::Paused && state != ExecutionState::Uninitialized {
            return Err(self.make_error(
                "compositor",
                "load",
                format!(
                    "load requires Paused or Uninitialized state (FPA-023), but compositor is in {} state",
                    state
                ),
            ));
        }

        self.apply_state_fragment(fragment)
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
        // When called as a nested partition, the outer compositor is responsible
        // for ensuring idle state.  Automatically pause/resume so the inner
        // compositor's load() precondition (Paused) is satisfied.
        let was_running = self.state_machine.state() == ExecutionState::Running;
        if was_running {
            self.pause()?;
        }
        let result = self.load(state);
        if was_running {
            self.resume()?;
        }
        result
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}
