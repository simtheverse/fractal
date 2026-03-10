# Inter-layer Communication

## What problem does it solve?

The fractal partition pattern decomposes the system into layers. At each layer, a
compositor assembles partitions. But the pattern's explanations of contracts, typed
messages, and bus communication focus on the horizontal case — peer partitions at the
same layer exchanging data. The vertical case — how compositors communicate with their
child partitions, and how requests propagate across layer boundaries — is structurally
different and requires its own treatment.

Without explicit vertical communication rules, several problems emerge:

- A sub-partition deep in the hierarchy needs to stop the simulation, but its bus is
  local to its layer. How does the request reach the orchestrator?
- The compositor pushes shared context (WorldState, execution state) to its partitions.
  Is this a trait call or a bus broadcast? The answer affects how partitions consume it.
- State snapshots must collect contributions from every partition at every layer. The
  collection must recurse through compositors without breaking encapsulation.
- A sub-partition faults during execution. Who catches it, who decides the response, and
  how does the outer layer learn about it?

This document answers these questions by defining the compositor's communication
responsibilities at runtime — not just as an assembler at startup, but as a gateway
between layers during execution.

## The layer-scoped bus

Each compositor owns a bus instance for its layer's partitions. The layer 0 orchestrator
owns the layer 0 bus. If the physics partition decomposes into sub-partitions at layer 1,
the physics compositor owns a layer 1 bus for atmosphere, aerodynamics, and gravity. If
the atmosphere model further decomposes at layer 2, its compositor owns a layer 2 bus.

```
Layer 0 bus (orchestrator):
  ↔ physics partition
  ↔ gnc partition
  ↔ visualization partition
  ↔ ui partition

Layer 1 bus (physics compositor):
  ↔ atmosphere sub-partition
  ↔ aerodynamics sub-partition
  ↔ gravity sub-partition

Layer 1 bus (gnc compositor):
  ↔ estimator sub-partition
  ↔ guidance sub-partition
  ↔ control sub-partition
```

Bus instances are independent. They may run different transport modes — the layer 0 bus
might use network transport for distributed execution while a layer 1 bus uses
in-process transport because its sub-partitions are tightly coupled. The transport
independence guarantee applies per bus instance: each bus produces identical results
regardless of its transport mode, but different buses are not required to use the same
mode.

A sub-partition at layer 1 publishes only to the layer 1 bus. It has no reference to the
layer 0 bus and no knowledge that a layer 0 exists. This is the encapsulation guarantee:
the outer layer sees partitions as opaque units, not as compositors with visible
children.

## Downward communication

The compositor communicates downward to its partitions through two mechanisms,
distinguished by purpose.

### Trait calls for lifecycle control

The compositor drives execution by calling trait methods on each partition:

- `init(config)` — initialize with configuration derived from composition fragments
- `step(dt)` — advance one timestep
- `shutdown()` — clean up resources
- `contribute_state()` — request a state snapshot contribution
- `load_state(fragment)` — restore state from a snapshot fragment

These are imperative, sequential operations. The compositor controls the order in which
partitions step, enforces tick boundaries, and manages the partition lifecycle. This
requires call-and-return semantics — the compositor calls one partition at a time and
waits for completion before proceeding.

Trait calls are the mechanism for control flow that must be ordered. The compositor
decides which partition steps first, when to collect state, and when to shut down.

### Bus broadcast for shared context

Data that all partitions need — WorldState, execution state, environment context — flows
downward through the layer's bus. The compositor publishes it on the bus, and partitions
consume it alongside peer messages. From the partition's perspective, there is no
distinction between data from the compositor and data from a peer — both arrive as typed
messages on the bus.

This means partitions consume downward context using the same subscription mechanism they
use for horizontal data. A partition that reads `WorldState` does not know or care
whether it was published by the compositor or by a peer partition — it subscribes to
`WorldState` on the bus and receives it.

The principled split is:

| Mechanism | Purpose | Examples |
|---|---|---|
| Trait calls | Imperative lifecycle control | `step()`, `init()`, `shutdown()`, `contribute_state()` |
| Bus broadcast | Shared context and data | WorldState, execution state, environment state |

Trait calls are targeted (one partition at a time) and sequential (the compositor
controls order). Bus broadcasts are untargeted (all partitions receive) and decoupled
(partitions subscribe to types, not to the compositor).

## Upward communication

Partitions communicate upward to the compositor through the bus — the same bus they use
for horizontal communication with peers. The compositor, as the bus owner, is the
designated reader for certain message types.

### Requests on the bus

When a partition needs to influence state owned by the compositor — execution lifecycle,
shared state machines — it emits a typed request on the bus. The compositor receives the
request and arbitrates using the same single-owner authority pattern described in
[Inter-partition Communication](inter-partition-communication.md).

A sub-partition emitting `ExecutionStateRequest::Stop` on the layer 1 bus is using the
same mechanism as a partition emitting the same request on the layer 0 bus. The
compositor at each layer is the arbitrator for its bus. The pattern is uniform; only the
scope changes.

### Data contributions

Partitions publish their output data on the bus — `PlantState`, `GNCCommand`, sensor
readings — and the compositor consumes it. This is the same horizontal publishing
mechanism, but the compositor happens to be one of the consumers. The compositor may
aggregate contributions (assembling individual partition outputs into a composite output
for the layer above) or forward selected data to the outer bus as part of its own
contract obligations.

## Relay across layers

When a request on an inner bus needs to reach an outer layer — a sub-partition's
`ExecutionStateRequest::Stop` needs to reach the orchestrator — the compositor acts as
a relay gateway. This is the mechanism that connects layer-scoped buses into a coherent
system.

### The relay chain

```
Layer 2 sub-partition emits ExecutionStateRequest::Stop
  → layer 2 bus
  → layer 1 compositor receives, decides to relay
  → layer 1 compositor emits on layer 1 bus
  → layer 0 compositor (partition) receives, decides to relay
  → layer 0 compositor emits on layer 0 bus
  → orchestrator arbitrates
```

Each compositor in the chain independently decides whether to relay. There is no
automatic forwarding — the compositor has full authority over what crosses its layer
boundary.

Because each compositor processes its inner bus during its own `step()` and relays on
the next outer-bus read cycle, a request emitted at layer N may take up to N physics
ticks to reach the orchestrator. This latency is acceptable for normal execution state
transitions. Safety-critical signals that cannot tolerate relay latency use the direct
signal mechanism, which bypasses the relay chain entirely.

### Compositor relay authority

The compositor is not a transparent pipe. When it receives a request on its inner bus,
it may:

- **Relay as-is.** Forward the request to the outer bus unchanged. This is the common
  case for requests like `ExecutionStateRequest::Stop` where the compositor has no
  reason to intervene.

- **Transform.** Modify the request before relaying. The compositor might add context
  ("physics partition requested stop because aerodynamics sub-partition detected
  structural load exceedance") or change the request type (converting an internal
  warning into an external stop request).

- **Suppress.** Handle the request internally without relaying. The compositor might
  respond to a sub-partition failure by switching to a fallback implementation rather
  than propagating a stop request. The outer layer never knows the internal event
  occurred.

- **Aggregate.** Collect multiple requests from different sub-partitions within a single
  tick and relay a single consolidated request. This prevents the outer bus from seeing
  redundant or conflicting requests from a partition's internals.

This authority preserves encapsulation: the outer layer sees only what the compositor's
contract promises, regardless of what happens internally. A compositor that relays
everything transparently is making a deliberate choice, not following a default.

### Why compositor authority, not transparent relay

Transparent relay is simpler but violates encapsulation. If any inner request
automatically appears on the outer bus, the outer layer implicitly depends on the inner
structure — it may observe messages from sub-partitions it shouldn't know about, or
its behavior may change when a partition's internal decomposition changes.

Compositor authority ensures the outer layer's experience is determined by the
compositor's contract, not by its implementation. This is the same principle that makes
partitions replaceable: the contract, not the internals, defines the interface.

The cost is that every compositor must handle relay decisions. In practice, most
compositors relay `ExecutionStateRequest` transparently and suppress everything else —
a small amount of boilerplate for a strong encapsulation guarantee.

## Direct signals

While the default path for inter-layer requests is the compositor relay
chain, certain safety-critical signals must reach the orchestrator without risk of being
suppressed or delayed by intermediate compositors. The contract crate at the outermost
layer may declare **direct signal types** that bypass the relay chain entirely.

### When direct signals are warranted

Direct signals exist for a narrow set of scenarios where compositor authority is a
liability rather than an asset:

- **Emergency stop.** A hardware fault or safety limit exceedance that must halt the
  simulation immediately, regardless of any compositor's policy.
- **Hardware fault.** A sensor or actuator failure detected at any layer that requires
  system-wide response.

These are signals where the cost of a compositor suppressing them (even accidentally)
exceeds the cost of breaking layer encapsulation.

### How direct signals work

Direct signal types are declared in a contract crate. Any partition within that contract
crate's hierarchy can emit a declared direct signal. The signal bypasses all intermediate
bus instances within the hierarchy and reaches the declaring crate's orchestrator
directly.

```
Layer 2 sub-partition emits DirectSignal::EmergencyStop
  → bypasses layer 2 bus
  → bypasses layer 1 bus
  → reaches universe's orchestrator directly
  → orchestrator applies emergency response
```

The mechanism for bypass is implementation-specific — it might be a separate channel, a
shared atomic, or a direct callback registered during initialization. The key property
is that no compositor in the chain can intercept or suppress it.

### Direct signals are hierarchy-scoped

Direct signals are scoped to the contract crate that declares them — they are not truly
system-global when the system is embedded. When universe is a partition in an outer
application:

```
Outer app (layer 0):
  outer-core (outer contract crate)
  outer orchestrator ↔ universe (partition) ↔ other partitions

Universe (layer 1 from outer app's perspective):
  sim-core (universe's contract crate)
  universe orchestrator ↔ physics ↔ gnc ↔ viz ↔ ui
```

A `DirectSignal::EmergencyStop` declared in `sim-core` and emitted by a physics
sub-partition reaches universe's orchestrator — not the outer app's orchestrator. The
signal does not cross universe's boundary. The outer app learns of the event only
through universe's contract interface on the outer bus (e.g., a status change or request
that universe emits as a result of handling the emergency internally).

This scoping preserves encapsulation. The outer app treats universe as an opaque
partition. It does not know about `sim-core`'s direct signal types, just as it does not
know about universe's internal partitions. If the outer app needs its own direct signals,
it declares them in `outer-core`, independent of universe's internal signals.

### Constraints on direct signals

Direct signals are not a general-purpose communication mechanism. They are constrained
by design:

- **Small, stable vocabulary.** The set of direct signal types at any layer should be
  minimal and change infrequently. If a signal can be handled through the normal relay
  chain, it should be.
- **No data flow.** Direct signals carry minimal payload — a signal type, a reason
  string, and a source identifier. They are notifications, not data channels.
- **Audit trail.** Every direct signal emission is logged with the emitting partition's
  identity and layer depth, providing visibility into when the escape hatch is used.
- **Hierarchy-scoped.** Direct signals do not propagate beyond the declaring contract
  crate's boundary. Each embedding context defines its own safety mechanisms.

The existence of direct signals does not weaken the relay chain — it complements it.
Normal inter-layer communication goes through compositor relay with full authority. Direct
signals exist for the rare case where that authority must be overridden for safety.

## Recursive state contributions

State contribution contracts — used for snapshots, telemetry aggregation, and dump/load
operations — must work across layers. A compositor that owns sub-partitions implements
the state contribution trait by delegating to its children and assembling the results.

### Dump (downward collection, upward assembly)

When the orchestrator requests a state snapshot, it calls `contribute_state()` on each
layer 0 partition. A partition that is a compositor delegates recursively:

```
orchestrator calls physics.contribute_state()
  physics compositor calls atmo.contribute_state()  → atmosphere fragment
  physics compositor calls aero.contribute_state()  → aerodynamics fragment
  physics compositor calls gravity.contribute_state() → gravity fragment
  physics compositor assembles:
    [physics]
    internal_state = ...
    [physics.atmosphere]
    ...from atmosphere fragment...
    [physics.aerodynamics]
    ...from aerodynamics fragment...
    [physics.gravity]
    ...from gravity fragment...
  returns assembled fragment to orchestrator
```

The compositor's contribution includes both its own internal state and the nested
contributions of its children. The result is a nested TOML fragment — not a flat
key-value map — preserving the hierarchical structure and remaining compatible with the
`extends` and override mechanisms used for all other composition fragments.

The outer layer sees one contribution per partition. It does not know whether the
contribution was assembled from sub-partitions or produced monolithically. A replacement
physics partition with no internal decomposition produces the same top-level structure.

### Load (downward decomposition, downward delegation)

Loading a snapshot reverses the process:

```
orchestrator calls physics.load_state(physics_fragment)
  physics compositor extracts own internal state
  physics compositor extracts [physics.atmosphere] section
  physics compositor calls atmo.load_state(atmosphere_section)
  physics compositor extracts [physics.aerodynamics] section
  physics compositor calls aero.load_state(aerodynamics_section)
  physics compositor extracts [physics.gravity] section
  physics compositor calls gravity.load_state(gravity_section)
```

The compositor decomposes the fragment along the same boundaries used for assembly and
delegates each section to the corresponding sub-partition through the same
`load_state()` trait call.

### Composition fragment compatibility

Because state contributions are TOML composition fragments, they participate in the same
inheritance and override system as configuration. A snapshot fragment can be used as a
configuration input — loading a snapshot is equivalent to configuring the system with a
composition fragment that specifies exact state values. This equivalence holds at every
layer because the nesting structure matches the partition hierarchy.

## Fault propagation

When a sub-partition faults — returns an error from `step()`, panics, or times out — the
compositor is responsible for the response. Faults are a compositor-internal concern, not
a bus concern. There is no fault-specific infrastructure.

### The compositor's responsibility

The compositor calls `step()` on each sub-partition. If a sub-partition returns an error,
the compositor decides the response:

- **Emit a stop request.** The most common response. The compositor emits
  `ExecutionStateRequest::Stop` on its bus (which may then be relayed to the outer
  layer through the normal relay chain). The outer layer sees a stop request from the
  partition, not a raw fault from a sub-partition.

- **Fall back.** If an alternative implementation is available, the compositor might
  switch to it and continue execution. The outer layer never knows the primary
  implementation faulted.

- **Log and continue.** For non-fatal errors — a sub-partition reporting a degraded
  result, a recoverable timeout — the compositor may log the fault and proceed. The
  outer layer sees no change in behavior.

### Why faults are not a bus concern

A separate fault channel or fault message type would create a parallel communication
path with its own transport, relay, and arbitration semantics. This complexity is
unnecessary because:

- The compositor already has a direct call-and-return relationship with its
  sub-partitions. It catches errors from `step()` as a normal part of execution.
- The compositor's response to a fault is a domain decision (stop? fall back? continue?),
  not a routing decision. It belongs in the compositor's logic, not in bus infrastructure.
- The outer layer should see the compositor's *decision*, not the raw fault. This is the
  same encapsulation principle that governs relay authority.

If richer fault context is needed at the outer layer — for diagnostics, logging, or
operator display — the compositor can include fault details in the payload of its stop
request or publish a diagnostic message on the outer bus. The mechanism is the existing
typed message system, not a new fault channel.

## Design choices and tradeoffs

### Why layer-scoped buses, not a global bus

A global bus lets any partition at any depth publish directly to the orchestrator. This
is simpler but creates several problems:

- The orchestrator sees messages from sub-partitions, creating implicit dependencies on
  internal structure.
- Replacing a partition's internal decomposition changes the messages visible on the
  global bus, breaking the replaceability guarantee.
- Transport mode selection becomes system-wide — you cannot run the layer 0 bus over the
  network while keeping a layer 1 bus in-process.
- Conflict resolution becomes harder when the orchestrator must arbitrate requests from
  partitions at multiple depths without knowing their relationships.

Layer-scoped buses trade routing simplicity for encapsulation. The compositor relay chain
is the cost; the contract boundary as the communication boundary is the benefit.

### Why bus broadcast for downward data, not trait arguments

An alternative design passes downward context as arguments to trait methods —
`step(dt, world_state, exec_state)` instead of publishing `WorldState` on the bus. This
is more explicit but has costs:

- It couples the trait signature to the set of shared context types. Adding a new shared
  context type requires changing the trait, which requires changing every implementation.
- It creates a distinction between how partitions receive peer data (bus subscription)
  and compositor data (method arguments). Partitions must handle two consumption
  mechanisms for data that is functionally equivalent.
- It prevents partitions from being agnostic about the source of data. A partition that
  subscribes to `WorldState` on the bus does not know or care whether it was published
  by the compositor or by a peer.

Bus broadcast for shared context keeps all data consumption uniform: partitions subscribe
to typed messages on the bus, regardless of who published them.

### Why recursive nested state, not flat contributions

A flat contribution model — each sub-partition writes to a globally namespaced key-value
store — is simpler but loses the hierarchical structure that makes composition fragments
composable. Nested TOML fragments can be used with `extends`, overridden at any scope,
and edited by humans. Flat keys cannot participate in the inheritance system and require
convention-based namespacing that is fragile as the system deepens.

The recursive model also preserves encapsulation for state contributions: the outer layer
receives one fragment per partition, structured however the partition chooses. A
partition's internal state structure is not exposed through flat keys visible to the
entire system.

### Why direct signals exist despite compositor authority

The compositor relay chain is the right default — it preserves encapsulation and gives
each layer control over what crosses its boundary. But for safety-critical signals, the
cost of a missed or delayed relay exceeds the cost of breaking encapsulation. A hardware
fault that a compositor inadvertently suppresses could allow a simulation to continue in
an unsafe state.

Direct signals are the escape hatch: declared sparingly in a contract crate, scoped to
that crate's hierarchy, carrying minimal payload, and logged for audit. They complement
the relay chain rather than replacing it — normal communication uses relay,
safety-critical communication uses direct signals. When the system is embedded as a
partition, its direct signals remain internal — the embedding system defines its own
safety mechanisms independently.

### Why the tick lifecycle exists

The inter-layer communication architecture defines *what* is communicated and *who*
has authority over it, but does not by itself determine *when* messages become visible
to other partitions. The compositor tick lifecycle (SIM-SYS-062) fills this gap: it
defines a three-phase execution model with double-buffered message isolation, tick
barriers for WorldState assembly, and direct signal polling between partition steps.
Without this lifecycle, different transport modes could make different visibility
choices, violating transport independence. See the
[tick lifecycle and synchronization](tick-lifecycle-and-synchronization.md) explainer
for full details.
