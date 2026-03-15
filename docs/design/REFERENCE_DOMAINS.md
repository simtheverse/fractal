# Reference Domain Applications

This document describes four real-world applications built on FPA, sketched in
enough detail to validate framework APIs, test patterns, and architectural
decisions. Each application exercises different aspects of the architecture.
When designing implementation, creating tests, or evaluating APIs, consult
these sketches to ensure the framework serves real applications, not just
test partitions.

These are not implementations — they are design references that anchor the
prototype against concrete use cases.

### Layer structure reminder

The system is an acyclic graph of compositors and partitions. Layer 0 is
the single top-level compositor (the system itself). It composes layer 1
partitions. A layer 1 partition that is itself a compositor composes layer 2
partitions, and so on. Each layer uses the same structural primitives.

---

## 1. Quick-Service Restaurant Kiosk

**Domain:** Event-driven UI application with hardware integration.

### Concept of Operations

A self-service ordering kiosk at a restaurant chain. Customers browse the menu,
customize items, build an order, pay, and receive a receipt. The system manages
a session lifecycle (idle → browsing → ordering → payment → complete → idle)
and integrates with POS hardware (card reader, receipt printer). An operator
can remotely push menu updates and retrieve sales data.

### Partitions

The layer 0 compositor composes these layer 1 partitions:

| Partition | Implementation | Role |
|-----------|---------------|------|
| SessionManager | SessionFSM | Session state machine, timeout handling |
| MenuDisplay | MenuRenderer | Renders menu from catalog, handles navigation |
| OrderBuilder | OrderAccumulator | Accumulates items, applies modifiers, computes totals |
| PaymentProcessor | CardPayment | Drives card reader hardware, processes transactions |
| ReceiptPrinter | ThermalPrinter | Formats and prints receipts |
| RemoteSync | CloudSync | Pushes sales data, pulls menu updates |

### Inter-Partition Data Flow

All communication flows through the layer 0 bus:

```
MenuDisplay ──publishes──► MenuSelection (Queued)
    ↓ subscribes
OrderBuilder ──publishes──► OrderState (LatestValue)
    ↓ subscribes                    ↓ subscribes
PaymentProcessor                SessionManager
    │ publishes                     │ publishes
    ▼                               ▼
PaymentComplete (Queued)    SessionTransition (Queued)
    ↓ subscribes                    ↓ subscribes
ReceiptPrinter              All partitions (lifecycle)

RemoteSync subscribes to OrderState, publishes MenuCatalog (LatestValue)
MenuDisplay subscribes to MenuCatalog
```

### What This Strains

- **No meaningful `dt`**: The kiosk is event-driven. `run_tick(dt)` is called
  on UI frame boundaries or event pump cycles. `dt` carries frame time but
  partitions don't use it for physics — they use it for session timeout
  tracking. The framework must not assume `dt` is a physics timestep.

- **Session timeout via events**: SessionManager uses a time-triggered event
  (`at: 120.0`) to return to idle if no interaction occurs. The event fires
  based on elapsed compositor time, which accumulates from `dt` values passed
  to `run_tick`. The framework's event system must work for non-physics time.

- **Hardware partition construction**: PaymentProcessor and ReceiptPrinter
  need hardware handles at construction time. The factory receives config
  (device paths, baud rates) and the bus (for publishing PaymentComplete).
  This validates that `PartitionFactory` receiving config + bus is sufficient
  for real partition construction.

- **External state injection**: RemoteSync receives menu updates from a cloud
  service and publishes MenuCatalog on the bus. MenuDisplay subscribes and
  re-renders. This is inter-partition communication driven by external I/O,
  not by the compositor's tick cycle. The partition must integrate with async
  I/O within the synchronous step() contract.

- **Drop-in replacement**: CardPayment can be replaced with CashPayment or
  TestPayment without changing any other partition. The contract (publishes
  PaymentComplete, subscribes to OrderState) is the same. This is FPA-002
  in action — validates that the registry and composition fragment support
  swapping implementations by changing one line of TOML.

- **Operator state dump**: An operator retrieves the kiosk state (current
  order, session state, sales totals) via DumpRequest. The dump is a TOML
  file that can be loaded into a test kiosk for debugging. Validates FPA-022/023.

---

## 2. Flight Dynamics Simulation

**Domain:** Real-time physics simulation with multi-rate subsystems.

### Concept of Operations

A flight dynamics simulator for pilot training. The aerodynamics model runs
at 120 Hz for stability, while the cockpit instruments update at 30 Hz and
the instructor station polls at 1 Hz. The system loads aircraft configurations
from TOML fragments, supports mid-session aircraft swaps (load a different
state snapshot), and records flight data for playback.

### Partitions

The layer 0 compositor composes these layer 1 partitions:

| Partition | Rate | Role |
|-----------|------|------|
| Aerodynamics | 4x | Forces, moments, integration (120 Hz at 30 Hz base) |
| FlightControls | 4x | Control surface dynamics, autopilot (120 Hz) |
| Engine | 2x | Thrust model, fuel flow (60 Hz) |
| Instruments | 1x | Cockpit display state (30 Hz base rate) |
| Environment | 1x | Wind, weather, terrain |
| DataRecorder | 1x | Flight data recording for playback |
| InstructorStation | 1x | Remote monitoring, fault injection, repositioning |

**Aerodynamics** is itself a compositor (layer 1) composing layer 2 partitions:

| Sub-partition | Role |
|---------------|------|
| LiftDrag | Aerodynamic coefficient lookup |
| Atmosphere | Density, temperature, pressure model |
| Integrator | State vector integration (position, velocity, orientation) |

### Inter-Partition Data Flow

Layer 0 bus (connects layer 1 partitions):

```
FlightControls ──publishes──► ControlSurfaces (LatestValue)
                                    ↓ subscribes
Aerodynamics ─────publishes──► AircraftState (LatestValue)
    ↑ subscribes                    ↓ subscribes to SharedContext
Engine (thrust)              Instruments, DataRecorder, InstructorStation

Environment ──publishes──► WindState (LatestValue)
    ↓ subscribes
Aerodynamics, Engine

InstructorStation ──publishes──► RepositionCommand (Queued)
                                 FaultInjection (Queued)
    ↓ subscribes
Aerodynamics (reposition), Engine (fault)
```

Aerodynamics inner bus (layer 1 bus, connects layer 2 partitions):
```
LiftDrag ──publishes──► Coefficients (LatestValue)
    ↓ subscribes
Integrator ──publishes──► IntegratedState (LatestValue)
    ↑ subscribes
Atmosphere ──publishes──► AtmosphereState (LatestValue)
```

### What This Strains

- **Multi-rate scheduling**: Aerodynamics at 4x, Engine at 2x, others at 1x.
  Each partition's `step()` receives `dt / rate`. The double buffer must
  ensure that Instruments (1x) sees the final state from Aerodynamics' 4th
  sub-step, not an intermediate. Validates multi-rate (FPA-009) and tick
  isolation (FPA-014).

- **Fractal nesting with different rates**: The Aerodynamics compositor is
  a layer 1 partition stepped at 4x by the layer 0 compositor. Its inner
  layer 2 partitions (LiftDrag, Atmosphere, Integrator) run at 1x relative
  to their compositor's tick — but 4x relative to the system tick. The
  layer 1 bus is independent of the layer 0 bus. Validates FPA-001
  (fractality), FPA-008 (bus independence), FPA-012 (recursive state).

- **Mixed transport across layers**: The layer 0 bus uses network transport
  (InstructorStation runs on a separate machine). The layer 1 Aerodynamics
  bus uses in-process transport (tight coupling, no serialization overhead).
  Validates FPA-004 (layer-independent transport selection).

- **Mid-session state load**: InstructorStation repositions the aircraft by
  publishing a LoadRequest with a state snapshot. The layer 0 compositor
  pauses, loads, resumes — all happening in Phase 1 of the next tick. The
  loaded state must propagate correctly through the nested Aerodynamics
  compositor. Validates FPA-023 (load while idle).

- **Fallback partition**: If Aerodynamics panics (numerical instability),
  the layer 0 compositor activates a SimplifiedAero fallback that uses a
  reduced model. The fallback completes the remaining sub-steps for that
  tick. Validates FPA-011 (fault handling with fallback).

- **Direct signals**: A structural failure detection in Aerodynamics emits
  a direct signal ("structural_failure") that bypasses the relay policy
  and reaches the layer 0 compositor immediately. This is a safety-critical
  path that cannot be suppressed. Validates FPA-013.

- **Network serialization for real**: InstructorStation communicates over
  NetworkBus. AircraftState, ControlSurfaces, RepositionCommand all need
  codecs registered. SharedContext crosses the network boundary every tick.
  This is where the codec registration pattern gets a real workout — not
  just test types, but domain types with nested structs, enums, and floats.

- **Reference data and contract versioning**: The flight model is validated
  against known reference trajectories. A V1 contract defines standard
  atmosphere conditions; V2 adds wind shear. V1 reference data must remain
  valid when V2 types are added. Validates FPA-039.

---

## 3. Collaborative Document Editor

**Domain:** Event-driven application with distributed state and conflict resolution.

### Concept of Operations

A collaborative document editor (like a simplified Google Docs). Multiple
users edit the same document simultaneously. Each user's editor instance
runs as an FPA system. A server coordinates state, resolves conflicts, and
broadcasts changes. The editor supports undo/redo, formatting, and
cursor tracking.

### Partitions

The layer 0 compositor composes these layer 1 partitions:

| Partition | Role |
|-----------|------|
| DocumentModel | Authoritative document state, operation application |
| InputHandler | Keyboard/mouse events → edit operations |
| CursorTracker | Local and remote cursor positions |
| UndoManager | Operation history, undo/redo stack |
| NetworkSync | Sends local ops to server, receives remote ops |
| Renderer | Produces render output from document state |

### Inter-Partition Data Flow

All communication flows through the layer 0 bus:

```
InputHandler ──publishes──► EditOperation (Queued)
    ↓ subscribes
DocumentModel ──publishes──► DocumentState (LatestValue)
    ↑ subscribes                    ↓ subscribes
NetworkSync                  Renderer, CursorTracker
    │ publishes
    ▼
RemoteOperation (Queued) ──subscribes──► DocumentModel
UndoManager subscribes to EditOperation, publishes UndoOperation (Queued)
DocumentModel subscribes to UndoOperation
```

### What This Strains

- **Queued message ordering is critical**: EditOperations and RemoteOperations
  are Queued messages. The order in which DocumentModel processes them
  determines the document state. If the bus reorders or drops queued messages,
  the document diverges. This is the strongest test of Queued delivery
  semantics — correctness depends on it, not just convenience.

- **Variable tick rate**: The editor steps when the user types or when a
  remote operation arrives — not on a fixed clock. Some ticks process a
  burst of 50 remote operations; others process nothing. The compositor
  must handle zero-work ticks efficiently and bursty ticks correctly.

- **State snapshots for undo**: UndoManager uses `contribute_state()` to
  capture undo checkpoints. The state must include the full document and
  operation history. `load_state()` restores to a previous checkpoint.
  This exercises state dump/load with large, complex state trees — not
  just integer counters.

- **Network partition resilience**: When NetworkSync loses connection,
  local editing continues. When reconnection occurs, a burst of remote
  operations arrives. The system must apply them in order without losing
  local changes. This tests the bus's queued delivery under asymmetric
  load and the compositor's ability to process variable-length queues.

- **Multiple instances, shared contract**: Each user runs their own FPA
  system with identical partitions but different state. The contract
  (EditOperation, DocumentState, RemoteOperation) is shared. The
  NetworkSync partition's implementation differs between client and server.
  This validates FPA-002 (drop-in replacement) at the deployment level.

- **Render partition is output-only**: Renderer subscribes to DocumentState
  and CursorPositions but publishes nothing. It's a pure consumer. The
  framework must support partitions that only read from the bus. Its
  `contribute_state()` returns render statistics, not document content —
  the state contribution is independent of what the partition reads.

---

## 4. Industrial Process Controller

**Domain:** Safety-critical control system with redundancy and strict timing.

### Concept of Operations

A process controller for a chemical plant reactor. The system monitors
temperature, pressure, and flow rate sensors, computes control outputs
(valve positions, heater power), and enforces safety interlocks. A redundant
backup controller runs in hot standby, receiving the same sensor data.
If the primary faults, the backup assumes control within one tick.

### Partitions

The layer 0 compositor composes these layer 1 partitions:

| Partition | Role |
|-----------|------|
| SensorInput | Reads sensor hardware, publishes readings |
| ControlLaw | PID control, computes actuator commands |
| SafetyInterlock | Independent safety checks, emergency shutdown |
| ActuatorOutput | Drives actuator hardware from commands |
| AlarmManager | Alarm state machine, operator notification |
| DataLogger | Logs all state for regulatory compliance |

**Redundancy:** A second FPA system runs identically with the same
fragment. A supervisory layer determines which system's ActuatorOutput
is connected to the physical actuators.

### Inter-Partition Data Flow

All communication flows through the layer 0 bus:

```
SensorInput ──publishes──► SensorReadings (LatestValue)
    ↓ subscribes
ControlLaw ──publishes──► ActuatorCommands (LatestValue)
    ↓ subscribes                ↓ subscribes
ActuatorOutput           SafetyInterlock
                              │ publishes
                              ▼
                         SafetyAction (Queued) ─► emergency shutdown
                         AlarmEvent (Queued) ──► AlarmManager

DataLogger subscribes to SharedContext (all partition states every tick)
```

### What This Strains

- **Deterministic tick timing**: The controller runs at exactly 100 Hz.
  Every tick must complete within 10ms. The fault handling timeouts
  (50ms step, 500ms init) must be configurable per-domain — 50ms is
  too generous for a 10ms tick budget. This challenges whether timeout
  constants should be in the spec or configurable.

- **Fallback with identity**: If ControlLaw faults (numerical divergence),
  a SafeControlLaw fallback activates. The fallback drives actuators to
  safe positions (valves closed, heaters off). The fallback must have
  the same partition ID and publish the same ActuatorCommands type.
  The downstream ActuatorOutput doesn't know the switch happened.
  Validates FPA-011 (fallback identity requirement).

- **Direct signals for safety**: SafetyInterlock detects an overpressure
  condition and emits a direct signal ("emergency_shutdown"). This signal
  must reach the layer 0 compositor immediately — it cannot be suppressed
  by a relay policy. The compositor transitions to a safe state. Validates
  FPA-013 (direct signals cannot be suppressed).

- **Hot standby via state dump/load**: The backup system receives the
  primary's state dump every N ticks via network. If the primary faults,
  the backup loads the last good state and continues from where the
  primary left off. The state transfer happens over NetworkBus with
  real serialization. Validates FPA-004 (network transport for real),
  FPA-022/023 (state as operational checkpoint).

- **Regulatory compliance via DataLogger**: DataLogger subscribes to
  SharedContext and records every partition's state every tick. The
  recorded data must be traceable: which implementation versions were
  running, what contract version was in effect, what configuration was
  loaded. This is exactly what ReferenceFile provenance is designed for,
  but at runtime rather than test time.

- **SafetyInterlock is independent**: SafetyInterlock must not depend on
  ControlLaw's output. It reads SensorReadings directly and makes its
  own determination. If ControlLaw is in fallback or faulted, safety
  checks continue unaffected. This validates that partitions are truly
  independent — the safety case depends on it.

- **Relay policy for alarm management**: AlarmManager runs inside the
  reactor's layer 0 compositor. When it detects an alarm condition, it
  publishes a TransitionRequest. The compositor's relay policy determines
  whether this request propagates to the plant-wide orchestrator or is
  handled locally. Different alarm severities use different relay policies
  (forward critical, suppress informational). Validates FPA-010.

- **Identical fragment, different hardware**: The primary and backup
  controllers use the exact same composition fragment. Only the
  SensorInput and ActuatorOutput implementations differ (primary uses
  real hardware, backup uses network-mirrored inputs). The fragment
  doesn't change — the registry maps "SensorInput" to different
  factories. Validates that the framework's config-driven composition
  supports deployment variation without config changes.

---

## Cross-Cutting Validation Matrix

Each domain stresses different FPA capabilities. Use this matrix to verify
that framework changes serve all four domains, not just the one currently
being tested.

| Capability | Kiosk | Flight Sim | Doc Editor | Controller |
|------------|-------|------------|------------|------------|
| Event-driven (no fixed dt) | **primary** | | **primary** | |
| Multi-rate scheduling | | **primary** | | |
| Fractal nesting | | **primary** | | |
| Network transport (real) | | **primary** | **primary** | **primary** |
| Mixed transport per layer | | **primary** | | |
| State dump/load | validates | **primary** | **primary** | **primary** |
| Fallback partitions | | **primary** | | **primary** |
| Direct signals | | validates | | **primary** |
| Relay policy | | | | **primary** |
| Drop-in replacement | **primary** | validates | **primary** | **primary** |
| Queued message ordering | validates | | **primary** | |
| Hardware integration | **primary** | | | **primary** |
| Config-driven composition | **primary** | **primary** | **primary** | **primary** |
| Contract versioning | | **primary** | | |
| Variable tick rate | **primary** | | **primary** | |
| Deterministic timing | | **primary** | | **primary** |
| Large/complex state | | validates | **primary** | |
| Provenance/compliance | | | | **primary** |

**primary** = this domain is the hardest test of this capability.
**validates** = this domain uses the capability but doesn't push its limits.
