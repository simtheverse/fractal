# Inter-partition Communication

This document covers **horizontal** communication between peer partitions at the same
layer. For **vertical** communication between compositors and their child partitions
across layers, see [Inter-layer Communication](inter-layer-communication.md). For an
overview of how horizontal and vertical communication relate, see
[Communication in the Fractal Partition Pattern](communication-in-the-fractal-partition-pattern.md).

## What problem does it solve?

A partitioned system has a coordination problem. Partition A produces output state.
Partition B consumes that state and produces commands. Partition C renders both. Partition
D influences all of them. Each partition is independently replaceable — a contributor can
submit a new partition B implementation, a team can swap in a different partition A
engine — yet they must all exchange data through interfaces that survive those
substitutions.

The naive approach is to let partitions talk to each other directly: partition A calls a
method on the partition B module, partition B imports a type from the partition A crate.
This creates coupling that defeats the point of partitioning. Replace the partition A
crate and the partition B code breaks. Replace the partition B crate and the partition D
code that was importing its types breaks. The system is partitioned in name but coupled
in practice.

The fractal partition pattern's inter-partition communication design resolves this with
three principles: contracts live in one place, messages are typed, and the transport is
invisible.

## The contract crate

All inter-partition data structures and behavioral contracts are defined in a single
contract crate (the system core at layer 0). No partition imports types from another
partition. Every partition depends on the contract crate and nothing else.

This is a strict structural rule, not a guideline. The dependency graph has a star
topology: the contract crate at the center, partition crates at the tips, no edges
between tips. If a type from partition A needs to be visible to partition C, it is
defined in the contract crate, not in partition A's crate. If a partition B plugin needs
to know the output state format, it imports from the contract crate (or an ABI boundary
crate), never from partition A's crate.

The consequence is that replacing any partition is a local operation. The new
implementation depends on the contract crate, conforms to its contracts, and the rest of
the system is unaware that anything changed. The contract crate is the single source of
truth for what partitions may say to each other, and the only coupling point in the
system.

The fractal partition pattern requires this structure at every layer. At layer 1, a
partition's internal contract module plays the same role for its sub-partitions that
the system core plays at layer 0. Sub-partition X and sub-partition Y within
partition A share types through partition A's contract module, not through direct imports
from each other.

## Typed messages

All data crossing partition boundaries travels as instances of named, versioned message
types declared in the contract crate. `PartitionOutput`, `CommandMessage`, `EnvState`,
`SharedContext` — these are concrete structs with documented fields, statically checked
by the compiler.

The alternative — untyped byte buffers, serialized JSON, or dynamic payloads — trades
compile-time safety for flexibility that the system doesn't need. Partitions exchange a
small, stable set of message types. The types change infrequently and when they do, the
change should be visible at compile time to every consumer, not discovered at runtime
through a deserialization failure.

Typed messages also serve as documentation. Reading the contract crate's message
definitions tells you everything that flows between partitions — the vocabulary of the
system — without reading any partition's implementation.

## Transport independence

Messages travel between partitions through a transport abstraction (the layer-scoped bus
trait) that supports three modes: in-process synchronous channels, asynchronous
cross-thread message-passing, and network-based publish-subscribe. The active mode is
selected at runtime via configuration. No partition contains transport-specific code.

This separation means the same scenario runs identically on a developer's laptop (in-
process, minimal latency), on a multi-core workstation (async, partitions on separate
threads), or across a distributed lab setup (networked, partitions on separate machines).
The partition code doesn't change. The transport is an infrastructure concern configured
at the session level, invisible to the domain logic in each partition.

The requirement that all three transport modes produce identical final system state
(within floating-point determinism limits) is a strong correctness constraint. It means
partition logic cannot depend on timing assumptions, message ordering quirks, or
transport-specific side effects. If a scenario produces different results under different
transports, something is wrong with the partition logic, not the transport.

Two additional mechanisms make this constraint enforceable in practice. One
well-defined approach to enforcing this constraint is the compositor tick lifecycle
convention (FPA-014), which defines a double-buffered execution model: within a single
tick, all partitions read from the previous tick's outputs and write to the current
tick's buffer. No partition sees another partition's current-tick output, so the result
is independent of step order — which is what varies across transport modes. Bus delivery
semantics (FPA-007) specify per-message-type behavior (latest-value for continuous
state, queued for requests), ensuring that all transport implementations handle
producer–consumer rate mismatches identically. See the
[tick lifecycle and synchronization](tick-lifecycle-and-synchronization.md) explainer
for full details.

## What emerges from these principles

### Requests, not mutations

When a partition needs to influence shared state — pausing execution, stopping on a
limit exceedance, dumping a state snapshot — it does not reach in and mutate the value.
It emits a typed request on the bus. A single owner evaluates the request against defined
rules and applies or rejects it.

This pattern appears throughout the system. Partition D emits a pause transition request;
the orchestrator evaluates it against the execution state machine's transition rules.
Partition A emits a stop transition request when a monitored value exceeds a threshold;
the orchestrator applies the same rules. An event action emits `"state_dump"` with a
path; the orchestrator coordinates the snapshot through the state contribution contract.

The bus-mediated request pattern has several properties:

- **Single audit point.** Every state change flows through one owner. Logging at that
  point captures the complete history of who requested what and what was applied.

- **Conflict resolution.** When two partitions request conflicting transitions in the
  same tick — one requests pause, another requests stop — the owner applies a
  deterministic priority rule (stop beats pause beats resume) rather than letting the
  outcome depend on message ordering.

- **Partition ignorance.** A partition emitting a request doesn't need to know who the
  owner is, how many other partitions exist, or what other requests are in flight. It
  emits a typed message on the bus and the system handles the rest.

### Shared state machines

When multiple partitions need to coordinate around a state machine — execution lifecycle
states, mission phases, mode selections — the state machine's type and transition rules
are defined in the contract crate, not in any partition. One owner holds the
authoritative value. All other partitions observe it as read-only. Transitions happen
through bus requests.

The execution state machine (Idle → Running → Paused → Stopped) is the primary instance
of this pattern at layer 0. But the pattern is general. At layer 1, sub-partitions
within partition B might coordinate around a domain-specific phase state machine
(phase 1 → phase 2 → phase 3 → phase 4) defined in partition B's contract module. The
mechanism is identical: type in the contract module, single owner, bus-mediated
transitions, read-only observation by peers.

The value of defining this as a general pattern rather than special-casing the execution
state machine is that new shared state machines can be introduced at any layer without
inventing new coordination mechanisms. The contributor already knows how it works from
the execution state machine — the same owner/request/observe structure applies.

### State contribution contracts

When the orchestrator needs to collect state from all partitions — for a snapshot dump,
for telemetry aggregation — it doesn't reach into each partition's internals. The
contract crate defines a state contribution interface that each partition implements.
The orchestrator invokes the interface and assembles the results.

This preserves partition encapsulation. The orchestrator doesn't know what's inside a
partition's state. The partition doesn't know how the orchestrator will use the
contributed state. The contract stands between them, and both sides can change
independently as long as the contract holds.

The same interface works in reverse for loading a snapshot: the orchestrator decomposes
the fragment and passes each partition's section through the same contract. A
replacement partition that implements the contract automatically participates in dump and
load without any modification to the orchestrator or other partitions.

Because state snapshots are composition fragments — not opaque binary blobs — the
contributed state is human-readable, editable, and composable with the same `extends`
and override mechanisms used for all other configuration. A state contribution contract
that produces a TOML section is simultaneously a serialization interface and a
configuration interface.

### SharedContext as broadcast state

Multi-entity systems require each entity's partitions to be aware of peer entities.
Rather than having partitions query each other — which would create lateral coupling —
the system publishes an aggregated `SharedContext` each tick containing the
`PartitionOutput` of all active entities. Every partition receives the same
`SharedContext` through the bus.

This is a broadcast pattern: one publisher (the orchestrator), many consumers, no
consumer-specific channels. A partition B plugin coordinating with peers reads their
state from `SharedContext`. A partition C plugin rendering the full system does the same.
Neither knows or cares about the other's existence.

The `SharedContext` broadcast also establishes a consistency boundary. All partitions
that read `SharedContext` in a given tick see the same snapshot of entity states. There
is no risk of one partition seeing a partially updated set of entities while another sees
a different partial update.

## Design choices and tradeoffs

### Why a single contract crate, not per-partition interfaces

An alternative design would have each partition publish its own interface crate —
`partition-a-api`, `partition-b-api`, etc. — and let consuming partitions depend on the
specific interfaces they need. This is more granular and avoids putting unrelated types
in one crate.

The single contract crate trades granularity for simplicity. In a four-partition system
where the message vocabulary is small, the cost of co-locating all inter-partition types
is low. The benefit is a single dependency, a single versioning timeline, and a single
place to look when understanding what flows between partitions. As the system grows, the
contract crate may warrant internal module organization, but it remains one dependency
from each partition's perspective.

### Why typed messages, not a generic event bus

Many frameworks use a generic event bus where messages are boxed trait
objects, string-tagged payloads, or serialized blobs. This is maximally flexible — any
partition can emit anything — but it pushes type checking to runtime, makes the
interface vocabulary implicit, and makes it easy for partitions to develop hidden
dependencies on message formats that aren't declared anywhere.

Typed messages are less flexible but more honest. The contract crate is the complete
list of what can be said between partitions. If a new message type is needed, it must be
added to the contract crate, which forces a deliberate interface design decision rather
than an ad-hoc payload invention.

### Why transport independence is a correctness constraint

Requiring identical results across transport modes is expensive to verify. It would be
easier to allow transport-dependent behavior and document the differences.

The constraint exists because transport is an infrastructure choice, not a domain choice.
An operator selecting network transport for distributed execution should not get
different results than an operator selecting in-process transport for local
development. If they do, the partition logic has an implicit dependency on transport
timing, which is a bug — one that happens to be invisible when only one transport mode
is tested. The constraint makes these bugs visible.

### Why single-owner authority, not distributed consensus

Shared state machines use single-owner authority rather than distributed consensus
(voting, quorum, conflict-free replicated data types). Distributed consensus is more
robust to owner failure but introduces complexity and latency that are unnecessary in a
system where all partitions share a process or a tightly coupled network.

The single-owner model is simple: one partition decides, others observe. If the owner is
the orchestrator — as it is for the execution state machine at layer 0 — then the
orchestrator's liveness is already a prerequisite for the system running at all.
There is no scenario where the orchestrator is down but partitions need to reach
consensus on execution state. The failure mode doesn't exist, so the complexity of
handling it is pure cost.
