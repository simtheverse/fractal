# Execution Strategies in the Fractal Partition Pattern

## The compositor as lifecycle authority

The core architecture (FPA-009) requires the compositor at each layer to coordinate
partition execution. But it does not prescribe *how* that coordination works. The
compositor is always the lifecycle authority — no partition initializes, begins
processing, or shuts down without the compositor's involvement — but the degree of
runtime control varies by execution strategy.

This document describes the spectrum of execution strategies available to FPA-conforming
systems, when each is appropriate, and how the core architecture's mechanisms adapt to
each. For a complete overview of all compositor roles (not just execution coordination),
see
[The Compositor in the Fractal Partition Pattern](the-compositor-in-the-fractal-partition-pattern.md).

## The spectrum

Execution strategies fall along a spectrum defined by who decides when a partition
processes:

```
Direct invocation          Multi-rate             Supervisory coordination
─────────────────────────────────────────────────────────────────────────────
Compositor calls step()    Compositor calls at    Partitions run own loops;
on each partition;         different rates;       compositor manages lifecycle
full control over          compositor still       boundaries and fault detection
ordering and timing        invokes, but not
                           uniformly
```

All three are FPA-conforming. The compositor remains the lifecycle authority in every
case. What changes is the granularity of control.

## Direct invocation

Under direct invocation, the compositor calls each partition's lifecycle methods
(`init`, `step`, `shutdown`) and waits for completion. The compositor decides which
partition steps, when, and in what order. Partitions are passive — they execute when
called and return when done.

The invocation mechanism is implementation-defined:

- **In-process trait calls.** The compositor holds a reference to each partition and calls
  methods directly. Lowest latency, simplest implementation. Partitions share a process
  and may share a thread.

- **Cross-process dispatch.** The compositor sends a message to a partition running in a
  separate process and waits for a completion response. The partition still executes only
  when told to — it does not run a loop of its own. This gives process isolation (a
  panicking partition does not crash the compositor) while preserving the compositor's
  ordering control.

- **Remote procedure calls.** The compositor invokes lifecycle methods on a partition
  running on a separate compute node. Same control semantics as cross-process, but
  across a network boundary.

In all cases, the compositor has full control over execution order. It can guarantee that
partition A's output is available before partition B steps, that all partitions complete
before shared context is assembled, and that faults are detected immediately (the call
returns an error or times out).

### When to use direct invocation

Direct invocation is the right choice when:

- **Deterministic reproducibility is required.** The compositor controls step ordering,
  so the result is independent of thread scheduling, network timing, or OS scheduling
  decisions. This is essential for systems where the same configuration must produce the
  same output every time — flight simulation, physics modeling, regression testing.

- **Partitions have data dependencies within a processing cycle.** If partition B needs
  partition A's current output before it can step, the compositor can enforce that
  ordering by stepping A first. Under async execution, this would require explicit
  synchronization barriers.

- **Debugging and tracing require a linear execution history.** A sequential step log
  with compositor-controlled ordering is easier to reason about than interleaved async
  traces.

### The tick lifecycle convention

The tick lifecycle (FPA-014, defined in FPA-CON-000) is a specific direct-invocation
strategy that adds a three-phase structure with double-buffered message isolation:

- **Phase 1** assembles shared context, processes lifecycle operations, and swaps buffers.
- **Phase 2** steps all partitions against the previous cycle's data.
- **Phase 3** evaluates events, arbitrates requests, and relays messages.

The double-buffer ensures that no partition sees another partition's current-cycle output
during stepping — which makes the result independent of step order and enables safe
concurrent stepping as an optimization. This is the strongest determinism guarantee
available and is detailed in the companion explainer:
[Tick Lifecycle and Synchronization](tick-lifecycle-and-synchronization.md).

Systems that adopt the tick lifecycle get deterministic reproducibility, transport
independence (same results under in-process, async, and network transport), and safe
concurrent execution — properties that are particularly valuable for simulation, physics
modeling, and any domain where reproducibility is a correctness requirement.

## Multi-rate execution

Multi-rate execution is a middle ground: the compositor still invokes partitions
directly, but not all partitions step at the same rate. A compositor might step a
high-frequency partition 16 times for every one step of a low-frequency partition, or
organize partitions into rate groups that execute at different multiples of a base rate.

The compositor remains in full control — it decides which partitions step in each cycle
and how many times. Partitions are still passive; they execute when called. The
difference from uniform direct invocation is that the compositor's scheduling logic is
rate-aware.

### Approaches to multi-rate

**Sub-stepping within the compositor.** The compositor runs its own processing cycle at
the slowest rate. During each cycle, it calls `step()` on high-rate partitions multiple
times. Low-rate partitions step once. The outer layer sees one compositor step per cycle;
the inner rate differences are an implementation detail.

**Layer decomposition.** The high-rate partition is placed behind a compositor at layer 1.
The layer 1 compositor sub-steps it N times per outer `step()` call. The outer compositor
calls `step()` once on the layer 1 compositor; the layer 1 compositor internally runs N
cycles. This is the fractal approach — the inner tick lifecycle nests inside the outer
one. It has the advantage of cleanly separating rate concerns and keeping each
compositor's logic simple.

### When to use multi-rate

Multi-rate execution is appropriate when partitions have genuinely different update
requirements — a physics model at 1000 Hz and a display at 60 Hz, for example — but the
system still needs the compositor to control ordering and timing. It preserves the
determinism benefits of direct invocation while avoiding the waste of stepping slow
partitions at a fast rate.

## Supervisory coordination

Under supervisory coordination, partitions run their own processing loops. They are
not passive recipients of `step()` calls — they execute continuously (or event-driven)
at their own rate, on their own thread, process, or compute node. The compositor's role
shifts from caller to supervisor.

The compositor still:

- **Controls lifecycle boundaries.** It tells partitions when they may start processing
  (after initialization is complete and the bus is connected) and when they must stop
  (shutdown). No partition begins or ends its processing loop without the compositor's
  authorization.

- **Owns the bus.** It publishes shared context, receives requests, and arbitrates shared
  state machines. Partitions interact with each other and with the compositor exclusively
  through the bus.

- **Detects faults.** Instead of catching panics from a direct call, the compositor
  monitors partitions through heartbeats, health messages, or connection state. A
  partition that stops publishing, misses a heartbeat deadline, or disconnects is
  considered faulted. The compositor applies the same fault handling policy (FPA-011):
  propagate the error or activate a fallback.

- **Manages relay.** Inter-layer requests from self-scheduling partitions still flow
  through the bus, and the compositor still has relay authority (FPA-010) over what
  crosses its layer boundary.

What the compositor does *not* do under supervisory coordination:

- **Call `step()`.** Partitions schedule their own processing cycles.
- **Control execution order.** Partitions may process in any order, at any rate, at any
  time. The compositor does not impose ordering constraints.
- **Guarantee deterministic reproducibility.** Without compositor-controlled ordering,
  results may depend on timing, scheduling, and network conditions. This is acceptable
  for systems where throughput or latency matters more than reproducibility.

### When to use supervisory coordination

Supervisory coordination is the right choice when:

- **Partitions run on separate compute nodes.** A partition on a remote machine runs its
  own loop and publishes results on the bus. The compositor cannot meaningfully call
  `step()` at sub-millisecond granularity across a network — the overhead would dominate.
  Instead, the partition runs autonomously and the compositor supervises.

- **Partitions have independent timing requirements.** A data ingestion partition runs
  event-driven (processing messages as they arrive), an analysis partition runs at its own
  rate, and a display partition runs at the display refresh rate. There is no natural
  shared tick.

- **Throughput or latency is the priority.** Lock-step execution forces all partitions to
  wait for the slowest one each cycle. Supervisory coordination lets fast partitions run
  ahead and slow partitions catch up, maximizing throughput.

- **The system is a pipeline, not a simulation.** Processing stages that consume input,
  transform it, and produce output at their own rate do not need synchronized stepping.

### Fault detection under supervisory coordination

Under direct invocation, fault detection is immediate — the call returns an error or
times out. Under supervisory coordination, fault detection is asynchronous. The
compositor uses mechanisms such as:

- **Heartbeat monitoring.** The partition publishes a periodic heartbeat message on the
  bus. The compositor treats a missed heartbeat deadline as a fault.
- **Connection state.** If the bus transport detects a dropped connection (process exit,
  network failure), the compositor treats the disconnection as a fault.
- **Error messages.** The partition publishes an error message on the bus when it detects
  an internal failure. The compositor receives it through normal bus subscription.

The fault handling policy is the same regardless of detection mechanism: the compositor
logs the fault with diagnostic context (which partition, which layer) and either
propagates the error to the outer layer or activates a configured fallback (FPA-011).
The difference is latency — a fault under supervisory coordination may take longer to
detect than one under direct invocation.

## How core mechanisms adapt

The FPA core architecture defines mechanisms (bus, relay, fault handling, shared state
machines, events, state snapshots) that work across all execution strategies. Some
mechanisms behave differently depending on the strategy:

| Mechanism | Direct invocation | Supervisory coordination |
|---|---|---|
| Lifecycle control | Compositor calls `init`, `step`, `shutdown` | Compositor signals start/stop; partition runs own loop |
| Fault detection | Call returns error or times out | Heartbeat, connection monitoring, error messages |
| Execution ordering | Compositor-controlled, deterministic | Uncontrolled, timing-dependent |
| Relay latency | Bounded by processing cycles | Bounded by partition publish rate and bus latency |
| Shared context | Published at defined points in the cycle | Published periodically by the compositor |
| State snapshots | Compositor invokes `contribute_state()` on each partition | Compositor requests contributions via bus; partitions respond asynchronously |
| Bus delivery semantics | Latest-value and queued semantics per FPA-007; rate mismatches are compositor-managed | Same semantics, but rate mismatches are inherent and the bus must handle them without compositor mediation |
| Transport independence | Guaranteed by the tick lifecycle convention (FPA-014) when adopted | Requires the implementation to define its own consistency guarantees |

## Composability across strategies

Execution strategy is layer-local. A system built with one strategy can be embedded as a
partition in a system using a different strategy — without modification to either side.
This is one of the strongest properties of the fractal structure: the compositor at each
layer boundary acts as an adapter between strategies.

### The compositor as adapter

When a partition is itself a compositor, it has two faces: it presents an interface to the
outer layer (matching whatever the outer layer's execution strategy expects) and
internally coordinates its sub-partitions using whatever strategy they require. The outer
layer does not know or care about the inner strategy. It sees only the compositor's
contract outputs on the outer bus.

This means any combination works:

**Lock-step outer, supervisory inner.** The outer compositor calls `step()` on the
partition. The inner compositor's `step()` implementation collects the latest outputs
from its self-scheduling sub-partitions, aggregates them, and returns. The async
internals are invisible to the outer layer.

**Supervisory outer, lock-step inner.** The partition runs its own processing loop
(self-scheduling from the outer compositor's perspective). Each iteration of that loop
runs a full tick lifecycle internally with lock-step sub-partitions. The outer compositor
sees the partition publishing outputs on the bus at its own rate.

**Lock-step outer, multi-rate inner.** The outer compositor calls `step()` once. The
inner compositor runs N cycles for fast sub-partitions and one cycle for slow
sub-partitions, returning a composite result.

### Data freshness

When strategies differ across a layer boundary, the data a compositor returns may not
have been computed for the current invocation. A supervisory compositor embedded in a
lock-step system returns the latest available state from its async sub-partitions — which
may be from the current instant, from a moment ago, or from several processing cycles
back if a sub-partition is slow.

The outer layer needs to know this. A consuming partition may need to distinguish between
a freshly computed physics state and a stale one carried forward from a previous cycle.
The difference affects whether the consumer should proceed normally, use interpolation or
extrapolation, or flag a degraded-data condition.

The compositor shall indicate data freshness as metadata accompanying its output on the
outer bus (FPA-009). The freshness representation is defined in the contract crate
alongside the output type. Possible representations include:

- **A cycle identifier or timestamp** indicating when the data was last computed.
  Consumers compare against the current cycle to determine staleness.
- **A freshness flag** (e.g., `Fresh` vs `Stale`) for simple binary decisions.
- **An age metric** indicating how many outer-layer cycles have elapsed since the data
  was last updated.

The contract crate defines which representation is used and what the consumer's
obligations are when it receives stale data. This is a contract-level concern, not a
transport concern — freshness metadata travels with the typed message and is part of the
interface specification.

Under direct invocation with the tick lifecycle, data freshness is trivially `Fresh` for
every cycle — the compositor computed it synchronously. The freshness metadata may be
omitted or always set to fresh when the system uses a uniform lock-step strategy. It
becomes meaningful at layer boundaries where strategies diverge.

### Concrete example

```
Layer 0: orchestrator (tick lifecycle, in-process)
  ├── partition A (compositor at layer 1: supervisory, networked)
  │     ├── sub-partition A1 (remote node, self-scheduling)
  │     ├── sub-partition A2 (remote node, self-scheduling)
  │     └── sub-partition A3 (remote node, self-scheduling)
  ├── partition B (compositor at layer 1: tick lifecycle, in-process)
  │     ├── sub-partition B1 (in-process, direct invocation)
  │     └── sub-partition B2 (in-process, direct invocation)
  ├── partition C (leaf partition, no sub-partitions)
  └── partition D (leaf partition, no sub-partitions)
```

The layer 0 orchestrator uses the tick lifecycle. Every outer tick, it calls `step()` on
each partition.

Partition A's compositor supervises self-scheduling sub-partitions on remote nodes. When
the orchestrator calls `step()`, partition A's compositor collects the latest outputs from
A1, A2, and A3, aggregates them, and returns. If A2's latest output is two outer ticks
old (because A2 is slow or its node is loaded), partition A's output carries freshness
metadata indicating that the A2 component is stale. Partition C, consuming partition A's
output, can read the freshness metadata and decide how to handle it — proceed with stale
data, interpolate, or flag the condition.

Partition B uses the tick lifecycle internally with in-process sub-partitions. Its output
is always fresh — B1 and B2 were stepped synchronously during this outer tick. Its
freshness metadata reflects this.

Both strategies coexist in the same system because each compositor owns its own execution
strategy. The outer layer sees two partitions that both respond to `step()` calls and
produce typed output on the bus. One happens to be synchronous internally; the other
happens to be distributed. The contract boundary and the freshness metadata are what
make this transparent.

### Embedding across systems

The composability property extends outward. A system built with the tick lifecycle —
say, a flight simulation with synchronized physics and flight software — can be embedded
as a partition in an outer system using supervisory coordination. The outer system's
compositor supervises the flight simulation as a self-scheduling partition. The flight
simulation runs its own tick lifecycle internally, unaware that it is embedded.

Conversely, a distributed data processing system using supervisory coordination can be
embedded as a partition in a lock-step system. The outer compositor calls `step()`, and
the embedded system's compositor collects latest results from its distributed
sub-partitions and returns them with freshness metadata.

No modification is needed on either side. The compositor at the boundary adapts. This is
the same replaceability guarantee that makes partitions swappable — extended to execution
strategy.

## Choosing a strategy

| Concern | Direct invocation | Multi-rate | Supervisory |
|---|---|---|---|
| Deterministic reproducibility | Yes (with tick lifecycle) | Yes (compositor controls rates) | No (timing-dependent) |
| Transport independence | Yes (with tick lifecycle) | Yes | Requires explicit guarantees |
| Distributed execution | Possible (via RPC) but adds latency per step | Possible | Natural fit |
| Throughput | Limited by slowest partition per cycle | Better (slow partitions step less often) | Best (no synchronization overhead) |
| Latency | Bounded by cycle time | Bounded by fastest rate group | Lowest (partitions process immediately) |
| Debugging simplicity | Highest (linear execution trace) | Moderate | Lowest (async traces) |
| Fault detection latency | Immediate | Immediate | Heartbeat interval |

The choice is domain-driven. A flight simulation with coupled physics and flight software
needs deterministic lock-step execution — direct invocation with the tick lifecycle. A
distributed data processing pipeline needs throughput and independent scaling —
supervisory coordination. A system with both concerns uses different strategies at
different layers.

## Related documents

- [Tick Lifecycle and Synchronization](tick-lifecycle-and-synchronization.md) — the
  specific direct-invocation convention (FPA-014) that provides deterministic
  reproducibility
- [Inter-layer Communication](inter-layer-communication.md) — how messages flow between
  layers through compositor relay
- [Communication in the Fractal Partition Pattern](communication-in-the-fractal-partition-pattern.md) —
  overview of horizontal and vertical communication
- [Conventions in the Fractal Partition Pattern](conventions-in-the-fractal-partition-pattern.md) —
  why the tick lifecycle is a convention, not a core requirement
