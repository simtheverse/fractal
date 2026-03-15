# Fractal Partition Architecture — Specification
## A Domain-Agnostic Reference Architecture for Layered, Partition-Uniform Systems

---

| Field         | Value                                      |
|---------------|--------------------------------------------|
| Document ID   | FPA-SRS-000                                |
| Version       | 0.1.0 (draft)                              |
| Status        | Draft                                      |

---

## Table of Contents

1. [Purpose and Scope](#1-purpose-and-scope)
2. [Definitions and Abbreviations](#2-definitions-and-abbreviations)
3. [Core Pattern](#3-core-pattern)
4. [Communication](#4-communication)
5. [Inter-layer Communication](#5-inter-layer-communication)
6. [Configuration and Composition](#6-configuration-and-composition)
7. [State Management](#7-state-management)
8. [Events](#8-events)
9. [Requirements Index](#9-requirements-index)

---

## 1. Purpose and Scope

This document defines the **Fractal Partition Architecture** (FPA), a domain-agnostic
software architecture pattern for systems that decompose into layers and partitions with
uniform structural primitives. Any system built on this pattern is organized according to
the **fractal partition pattern**: the system is decomposed into layers (layer 0 at the
system level, layer 1 at the partition level, and so on) and partitions at each layer,
with each partition applying the same structural primitives — contracts, events,
configuration, and composition — regardless of its position in the hierarchy. This layer
and partition uniformity principle ensures that constructs learned at one level apply
identically at every other level.

This specification is the parent document for domain-specific system specifications that
instantiate the pattern. A domain-specific specification (e.g., for a flight simulation
framework, a robotics control system, or a data processing pipeline) references this
document and defines the concrete partitions, message types, and domain concerns of that
system. All child specifications shall include a traceability field referencing the FPA
identifier(s) from this document that each child requirement satisfies.

**Implementation neutrality:** The requirements in this document are intended to specify
behavioral and structural contracts without prescribing implementation technology.
References to specific tools, languages, and frameworks — including Rust, TOML, and
Cargo — are illustrative examples of one viable realization and shall not be read as
mandating those choices. A conforming implementation may substitute equivalent
technologies provided all stated behavioral and structural requirements are satisfied.

**Core architecture:** This document defines the core architecture. Conventions that
complement the core pattern — the tick lifecycle and the verification and testing
discipline — are defined in the companion document FPA-CON-000.

### Companion Documents

The following companion explanation documents provide conceptual discussion and
worked examples for the patterns defined in this specification:

- `fractal-partition-pattern.md`
- `applications-of-the-fractal-partition-pattern.md`
- `communication-in-the-fractal-partition-pattern.md`
- `inter-partition-communication.md`
- `inter-layer-communication.md`
- `the-compositor-in-the-fractal-partition-pattern.md`
- `execution-strategies-in-the-fractal-partition-pattern.md`
- `events-as-a-fractal-primitive.md`
- `testing-in-the-fractal-partition-pattern.md`
- `test-reference-data-in-the-fractal-partition-pattern.md`
- `tick-lifecycle-and-synchronization.md`
- `conventions-in-the-fractal-partition-pattern.md`
- `CONVENTIONS.md`

---

## 2. Definitions and Abbreviations

| Term              | Definition                                                                 |
|-------------------|----------------------------------------------------------------------------|
| Compositor        | A component that selects and assembles partition implementations at startup and, at runtime, owns the layer's bus instance, coordinates partition execution (ranging from direct invocation of lifecycle methods to supervisory coordination of self-scheduling partitions), publishes shared context on the bus, arbitrates requests, and relays inter-layer messages to the outer bus with authority to filter, transform, or suppress. The compositor is always the lifecycle authority regardless of execution strategy |
| Contract crate    | A module or package that defines traits and data types but contains no implementation. Named `<partition>-contract` or `<system>-contract` by convention (see FPA-040). In a Rust realization, this is a Rust crate; other technologies may use equivalent constructs. Each contract crate maintains a `docs/design/SPECIFICATION.md` serving as its Interface Control Document (ICD) |
| Composition fragment | A configuration block — inline or named — that selects partition implementations at a given scope within the fractal structure. A top-level composition fragment at layer 0 selects system-wide parameters. A composition fragment at layer 1 selects partition-level parameters. All composition fragments share the same override and inheritance semantics (see FPA-020, FPA-021) |
| Delivery semantic | A per-message-type specification of how the bus delivers messages to consumers. Latest-value retains only the most recent value (suitable for continuous state). Queued retains all messages in order (suitable for requests that must not be dropped). Declared in the contract crate alongside the type. See FPA-007 |
| Direct signal     | A safety-critical signal type declared in a contract crate that bypasses the compositor relay chain within that contract crate's hierarchy and reaches the declaring crate's orchestrator directly. Scoped to the declaring crate's jurisdiction — does not propagate beyond the boundary when the system is embedded as a partition in an outer system. Reserved for scenarios where compositor suppression would be unsafe (e.g., emergency stop, hardware fault). See FPA-013 |
| Event action      | An action identifier specified in an event's configuration definition. All event actions are declared in contract crates and scoped to the declaring crate's hierarchy. Actions defined in the system-level contract crate are available at every layer because all partitions depend on that contract crate. Actions defined in a partition's contract crate are available to events within that partition's hierarchy. The event mechanism is uniform; the action vocabulary is contract-crate-scoped. See FPA-029 |
| Fractal partition pattern | The architectural principle that the system is decomposed into layers and partitions, where each partition at every layer applies the same contract/implementation/compositor structure and the same event, configuration, and communication primitives as the system level. Named for the self-similarity of structure at every scale. See FPA-001 |
| Layer             | A level in the system's hierarchical decomposition. Layer 0 is the system level; layer 1 is the partition level. The fractal partition pattern applies at every layer: each uses the same structural primitives (contracts, events, composition) as the layer above it |
| Layer and partition uniformity principle | The defining property of the fractal partition pattern: structural primitives (contracts, events, configuration, composition, specification, and documentation structure) are identical in kind across all layers and partitions. A construct available at layer 0 is available in the same form at layer 1 and beyond |
| Layer-scoped bus  | A bus instance owned by the compositor at a given layer, connecting that layer's partitions. Each compositor owns a separate bus instance; sub-partitions publish only to their layer's bus, not to buses at other layers. Inter-layer communication occurs through compositor relay. See FPA-008 |
| Partition         | A functional subdivision of the system at a given layer. At layer 0, the top-level partitions defined by the domain-specific system specification. At layer 1, sub-components within a partition (e.g., sub-models, sub-services). Each partition is independently replaceable provided it conforms to its layer's interface contracts |
| Relay authority   | The compositor's right to decide whether a message received on its inner bus is forwarded to the outer bus. The compositor may relay as-is, transform, suppress, or aggregate messages before re-emitting them. See FPA-010 |
| State snapshot    | A composition fragment produced by capturing the complete system state at a point in time. A state snapshot is not a distinct system primitive — it is a composition fragment whose fields happen to have been machine-generated rather than hand-authored. Snapshots are loadable, inheritable, and overridable using the same mechanisms as any other composition fragment (see FPA-022) |
| Tick lifecycle    | An optional synchronization convention in which the compositor executes each processing cycle as a three-phase sequence: Phase 1 (pre-cycle processing), Phase 2 (partition stepping with message isolation), Phase 3 (post-cycle processing). Defined in FPA-CON-000 (FPA-014). Systems may adopt this convention for deterministic reproducibility or use alternative execution strategies (multi-rate, fully asynchronous) |

---

## 3. Core Pattern

---

### FPA-001 — Fractal Partition Pattern

**Statement:** The system shall apply the **fractal partition pattern** at every level
of its decomposition, in both runtime architecture and specification structure. At
layer 0 (system level), the top-level partitions use contract/implementation/
compositor structure with shared interface types in the system-level contract crate. At
layer 1 (partition level), the internal structure of each partition shall apply the same
contract/implementation/compositor pattern, such that sub-components within a partition
are each independently replaceable via trait objects without modifying their siblings.
The **layer and partition uniformity principle** shall hold: the same structural
primitives — contracts, events (see FPA-024), configuration (see FPA-019), composition,
specification, documentation structure (see FPA-030), and testing structure
(see FPA-032, FPA-033) — shall be identical in kind at every layer.

*Illustrative example: In a flight simulation system, layer 0 partitions might include
physics, GN&C, visualization, and UI. At layer 1, the physics partition might decompose
into aerodynamic model, atmosphere model, and navigation estimator — each independently
replaceable via the same contract/compositor structure.*

**Rationale:** The fractal partition pattern ensures that the modularity, event handling,
composition mechanisms, and specification structure proven at the system level are
available in the same form within each partition. Like a fractal, zooming into any
partition reveals the same contract/compositor/event/specification structure as the
system as a whole — and zooming out, the system itself is an independently replaceable
partition within a larger system. A host system invoking this system as
one stage in a larger pipeline treats it as a partition
within its own layer 0; the same contracts and interfaces that make the system's internal
partitions swappable make the system itself swappable in that outer context. This
maximizes the surface area of independent replaceability at every scale, allows
composition to be adjusted at the sub-model level (including selections via
composition presets), and allows teams to work on isolated sub-components — all using
the same constructs they encounter at the system level. Applying the
pattern to specification structure ensures that each layer's spec nucleates the next
layer's specs, propagating traceability and structural discipline to arbitrary depth.

**Verification Expectations:**
- Pass: Within a partition, substituting a sub-component with an
  alternative implementation requires no changes to the other sub-component source files
  at the same layer.
- Pass: Each partition contains a `contracts/` module or equivalent defining internal
  sub-component traits that are imported by implementations but not by callers outside
  the partition.
- Pass: Events defined at layer 1 (within a partition) use the same definition schema
  and trigger types as events defined at layer 0 (system level).
- Fail: A sub-model within a partition is instantiated by name (e.g., via `match` on a
  string) in more than one location, indicating the compositor pattern is not applied.
- Pass: Each partition's specification identifies its own sub-partitions and uses the
  same specification structure (statement, rationale, verification expectations,
  traceability) as this document.
- Fail: A partition implements domain-specific mechanisms (events, composition,
  configuration) using constructs that differ from those defined at the system level.
- Fail: A partition's specification defines requirements without identifying
  independently replaceable sub-partitions or without using the same structural
  primitives as this document.

---

### FPA-002 — Partition Independence

**Statement:** Each partition shall be independently replaceable at the module level
without requiring modification to the source code of any other partition, provided the
replacement implementation conforms to the inter-partition interface contracts defined in
the layer's contract crate.

**Rationale:** Independent replaceability at every layer is the defining structural
guarantee of a fractally partitioned system. At layer 0, it is the primary mechanism by
which the system accommodates alternative implementations and third-party components
without requiring those contributors to understand or modify the broader system.

**Verification Expectations:**
- Pass: An alternative implementation of any single partition is substituted in the
  orchestrator and the system compiles and executes without modifying source files in any
  other partition.
- Pass: The substitution requires changes only to orchestrator configuration or dependency
  declarations.
- Fail: Substituting one partition requires editing source code in any other partition.
- Fail: The compiler reports unresolved imports from a replaced partition's internal
  modules in any other partition.

---

### FPA-003 — Inter-partition Interface Ownership

**Statement:** All inter-partition data structures and behavioral contracts shall be
defined exclusively within the layer's contract crate. No partition shall import types or
traits directly from another partition's module.

**Rationale:** Centralizing interface ownership in a single contract crate prevents
circular dependencies, enforces the direction of coupling, and provides a single source
of truth for interface evolution and versioning.

**Verification Expectations:**
- Pass: A dependency graph of the system shows no direct edges between partition
  modules other than through the layer's contract crate.
- Pass: Dependency analysis for each partition lists no other partition as a
  direct or transitive dependency (except the contract crate).
- Fail: Any partition's dependency declaration lists another partition under
  its direct dependencies.

---

## 4. Communication

---

### FPA-004 — Transport Abstraction

**Statement:** The system shall support at minimum three inter-partition communication
modes: (a) in-process synchronous channels, (b) asynchronous message-passing across
threads or processes, and (c) network-based publish-subscribe over a configurable
endpoint. The Bus trait shall be object-safe via a typed extension pattern (object-safe
core with typed blanket-impl extension), allowing runtime transport selection without
recompilation. Transport selection is a compositor configuration choice, not a partition
concern — the typed extension pattern preserves compile-time type safety at the partition
API while supporting `dyn Bus` for runtime transport selection. Partitions connected by
a bus instance are not required to share a process, thread, or physical machine — the
bus abstraction shall support partitions executing in separate processes, on separate
cores, or on separate compute nodes. Consistent with the fractal partition pattern
(FPA-001), each compositor at every layer owns a bus instance for its partitions (see
FPA-008). Bus instances at different layers are independent and may use different
transport modes — the layer 0 bus might use network transport while a layer 1 bus uses
in-process transport. The transport independence guarantee (identical results across
modes) applies per bus instance.

When network-based transport is in use, message types that cross the bus must support
serialization and deserialization. The base message trait shall not require serialization
bounds — this would impose network transport concerns on in-process and async modes,
violating transport independence. Instead, the network transport implementation shall
define a serialization-capable message subtrait extending the base message trait with
serialization bounds, and require codec registration per message type.
Framework-defined message types (shared context, transition requests, dump and load
requests) shall implement the serialization subtrait so they function across all
transport modes without partition-side configuration beyond compositor-level codec
registration. The serialization requirement is a property of the transport
configuration, not of the message contract.

**Rationale:** In-process channels minimize latency for single-machine development.
Asynchronous channels support partitions running on separate threads, in separate
processes, or on separate cores at independent update rates. Network-based transport
enables distributed execution across machines and integration with external tools. No
single mode satisfies all deployment contexts. The typed extension pattern — an
object-safe core trait with typed methods provided via blanket-impl extension traits —
allows partitions to interact with the bus through compile-time-checked typed APIs while
the compositor selects the concrete transport at runtime via `dyn Bus`. Layer-scoped bus
instances allow transport mode to be selected independently at each layer, matching the
deployment needs of each compositor's partitions without imposing a system-wide choice.

**Verification Expectations:**
- Pass: The same configuration executes to completion under all three transport modes with
  identical final state (within floating-point determinism limits) when run on
  a single machine.
- Pass: Switching transport mode requires only a change to the configuration for the
  relevant layer; no source files are modified.
- Pass: A layer 0 bus configured for network transport and a layer 1 bus configured for
  in-process transport operate correctly in the same run.
- Fail: A partition contains a compile-time import or branch that is specific to one
  transport mode (i.e., transport choice is not fully abstracted behind the bus
  trait).
- Pass: Framework message types function under network transport without partition-side
  configuration beyond compositor-level codec registration.
- Pass: A partition publishing messages through the base message trait compiles and
  functions under all transport modes without transport-specific code.
- Fail: The base message trait requires serialization bounds, imposing network transport
  concerns on in-process and async modes.
- Fail: The system fails to initialize or exchange messages under any of the three
  transport modes.

---

### FPA-005 — Typed Message Contracts

**Statement:** All data exchanged between partitions shall be transmitted as instances
of named, versioned message types declared in the layer's contract crate. Untyped byte
buffers, raw pointers, and dynamically-typed serialized payloads shall not be used as the
primary message format at partition boundaries.

**Rationale:** Typed messages make interface contracts explicit and compiler-enforced,
prevent silent data misinterpretation across partitions, and provide a stable basis for
version compatibility tracking.

**Verification Expectations:**
- Pass: Every field consumed by a receiving partition from an inter-partition message is
  statically typed and verifiable by the compiler at the sender's module boundary.
- Pass: The contract crate declares partition output types as named types with documented
  field semantics.
- Fail: Any inter-partition exchange relies on untyped byte buffers or dynamically-typed
  serialized values as the message payload without an enclosing typed wrapper defined in
  the contract crate.

---

### FPA-006 — Shared State Machine Synchronization

**Statement:** When a state machine must be observed by multiple partitions at the same
layer, its type and transition rules shall be defined in the contract crate for that
layer. Exactly one owner — the orchestrator at layer 0, or a designated partition at
deeper layers — shall hold the authoritative value. All other partitions at that layer
shall observe the current state as a read-only value published through the contract
crate. State transitions shall be requested via the bus, never by direct
mutation of the authoritative value. The owner shall evaluate requests against the state
machine's transition rules and reject invalid transitions. Consistent with the fractal
partition pattern (FPA-001), this mechanism shall be identical at every layer. The state
machine vocabulary (the set of states and transition rules) is defined by the contract
crate for each layer, not by this specification. FPA-006 defines the pattern — single
owner, bus-mediated requests, transition rule enforcement — not the specific states.

**Rationale:** Partitions at the same layer frequently need to coordinate around shared
state machines — execution lifecycle, mission phase, mode selections — without coupling
to each other's internals. Placing the type and transition rules in the contract crate
makes the state machine part of the layer's interface contract rather than an
implementation detail of any single partition. Single-owner authority with bus-mediated
requests prevents conflicting mutations and provides a single audit point for
transitions. The fractal partition pattern requires this mechanism to be available at
every layer. A domain-specific system might define an execution lifecycle state machine
at layer 0, while sub-partitions at layer 1 or deeper define their own shared state
machines (e.g., mission phase, mode selections) using the same pattern.

**Verification Expectations:**
- Pass: All partitions at a given layer read a shared state machine's current value from
  the same contract-crate-defined type; no partition defines its own copy of the state
  enum.
- Pass: A partition requesting a transition emits a typed request on the bus; the owner
  evaluates and applies or rejects it according to the defined transition rules.
- Pass: An invalid transition request is rejected by the owner and logged; the state
  machine value remains unchanged.
- Pass: At layer 1, a sub-partition defines a shared state machine in its layer's
  contract module using the same owner/request/observe pattern used at layer 0.
- Fail: A partition directly mutates a shared state machine's value without emitting a
  request on the bus.
- Fail: The mechanism for defining and synchronizing a shared state machine at layer 1
  differs structurally from the mechanism used at layer 0.

---

### FPA-007 — Bus Delivery Semantics

**Statement:** Each message type declared in a contract crate shall specify its
delivery semantic as part of the type's interface contract. Two delivery semantics
are defined:

- **Latest-value:** The bus retains only the most recent published value for the
  message type. A consumer that reads slower than the producer publishes will see
  only the most recent value, not intermediate values. Suitable for continuous state
  (e.g., partition output messages that represent a continuously updated snapshot).
  A subscriber created after a value has been published shall not observe that value;
  retention applies only to messages published after subscription. The bus does not
  replay historical messages to late subscribers.
- **Queued:** The bus retains all published instances in order. A consumer receives
  every instance regardless of rate mismatch. Suitable for requests and commands
  where dropping an instance is a correctness failure (e.g., execution state
  transition requests).

The specified semantic shall be enforced identically across all transport modes.
Queued messages shall not be silently dropped under any transport mode.

**Rationale:** Under async transport with partitions at different update rates,
a producer may publish faster than a consumer reads. Without a
specified delivery semantic per message type, different transport implementations make
different choices — leading to transport-dependent behavior. Declaring the semantic in
the contract crate alongside the type makes bus delivery behavior part of the interface
contract rather than a transport implementation detail, supporting the transport
independence guarantee (FPA-004).

**Verification Expectations:**
- Pass: The delivery semantic for each message type is declared in the contract crate
  and behaves identically under all three transport modes.
- Pass: A latest-value message type published multiple times within a processing cycle is
  read by a slower consumer as a single value — the most recent — with no error or data
  loss concern.
- Pass: A queued message type published multiple times within a processing cycle is
  received by the consumer as a complete, ordered sequence with no dropped instances.
- Fail: A transport implementation silently drops queued messages to maintain throughput.
- Fail: The delivery behavior for a message type varies across transport modes.

---

## 5. Inter-layer Communication

---

### FPA-008 — Layer-scoped Bus

**Statement:** Each compositor shall own a bus instance for its layer's partitions.
The layer 0 orchestrator owns the layer 0 bus; a compositor at layer 1 owns a layer 1
bus for its sub-partitions; the pattern continues at deeper layers. Bus instances at
different layers are independent: they may use different transport modes, and a
sub-partition at layer N publishes only to the layer N bus, never to a bus at any other
layer. Inter-layer communication between buses occurs exclusively through compositor
relay (FPA-010) or direct signals (FPA-013).

**Rationale:** Layer-scoped buses preserve the encapsulation guarantee of the fractal
partition pattern (FPA-001). The outer layer sees its partitions as opaque units —
it does not observe messages from sub-partitions and cannot depend on a partition's
internal decomposition. A partition that decomposes into multiple sub-components
at layer 1 looks identical on the layer 0 bus to a monolithic partition. This ensures
that replacing a partition's internal structure does not change the messages visible on
the outer bus, maintaining the replaceability guarantee. Independent transport modes per
layer allow deployment flexibility — the layer 0 bus can use network transport for
distributed execution while a layer 1 bus uses in-process transport for tightly coupled
sub-partitions.

**Verification Expectations:**
- Pass: A compositor at layer 1 creates and owns a bus instance that connects its
  sub-partitions; this bus is distinct from the layer 0 bus.
- Pass: A sub-partition at layer 1 publishes a typed message that is received by its
  peers on the layer 1 bus but is not visible on the layer 0 bus.
- Pass: The layer 0 bus and a layer 1 bus operate with different transport modes in the
  same run without interference.
- Fail: A sub-partition at layer 1 or deeper holds a reference to or publishes directly
  on a bus at a different layer.
- Fail: Replacing a partition's internal decomposition (adding or removing
  sub-partitions) changes the set of messages visible on the outer bus.

---

### FPA-009 — Compositor Runtime Role

**Statement:** The compositor at each layer shall be active at runtime, not only at
assembly time. Its runtime responsibilities shall include: (a) coordinating partition
execution through a lifecycle contract (`init`, `step`, `shutdown`). The degree of
control ranges from direct invocation — the compositor calls each partition's lifecycle
methods via trait calls (in-process), message-based dispatch (cross-process), or remote
procedure calls (cross-node) — to supervisory coordination, where partitions run their
own execution loops and the compositor manages initialization, shutdown, fault detection,
and shared context publication. The compositor is always the lifecycle authority: even
when partitions self-schedule their processing, the compositor controls when they may
start, when they must stop, and under what conditions they are considered faulted.
(b) Owning the layer's bus instance and publishing shared context — aggregated state,
execution state, environment context — as typed messages on that bus; (c) receiving and
arbitrating requests from partitions on its bus, acting as the single owner for shared
state machines at that layer (FPA-006); (d) relaying inter-layer requests to the outer bus
according to its relay authority (FPA-010); and (e) detecting and handling faults from its
partitions (FPA-011). The compositor's execution strategy — lock-step ticks, multi-rate
scheduling, or fully asynchronous operation — is an implementation choice, scoped to
that compositor's layer. Different layers may use different execution strategies
independently: a layer 0 compositor using lock-step ticks can compose a partition whose
internal layer 1 compositor uses supervisory coordination, and vice versa. The
compositor at the layer boundary adapts between strategies — presenting the interface the
outer layer expects while internally using whatever strategy its sub-partitions require.
When a compositor's execution strategy differs from the outer layer's, the data it
returns may reflect the latest available state from its sub-partitions rather than state
computed synchronously for the current cycle. The compositor shall indicate data
freshness — whether its output was computed for the current invocation or is the most
recent previously computed result — as metadata accompanying its output on the outer bus.
The freshness representation is defined in the contract crate alongside the output type.
The core architecture does not mandate a particular synchronization model; the tick
lifecycle convention (FPA-014 in FPA-CON-000) is one available strategy that provides
deterministic reproducibility.

SharedContext is a framework-level message type defined in the contract crate, not an
internal compositor type. It is published on the layer's bus like any other typed message.

The Partition trait provides the following lifecycle guarantees: `init()` shall be called
before any `step()`; `step(dt)` shall be called with `dt` equal to the elapsed time since
the previous step invocation; `shutdown()` shall be called after all steps complete;
`load_state()` shall only be called when no `step()` is in flight.

Under supervisory coordination, the synchronous `init()` and `shutdown()` methods on
the Partition trait constitute lifecycle *signals* — they spawn or stop sub-partition
tasks but do not guarantee that initialization has completed or that all asynchronous
work has ceased by the time they return. This is inherent to the model: sub-partitions
running on separate tasks, processes, or nodes cannot be synchronously joined through
a synchronous trait method. Lifecycle *confirmation* — the guarantee that sub-partitions
have actually initialized or stopped — requires a mechanism outside the synchronous
trait, such as an async init/shutdown path or the compositor's existing fault detection
mechanisms (heartbeat expiry, connection state). A supervisory compositor should provide
async counterparts (e.g., `async_init()`, `async_shutdown()`) that await sub-partition
lifecycle completion and propagate faults per FPA-011. The compositor retains lifecycle
authority in both cases: it decides *when* to signal init/shutdown, and it detects
*whether* sub-partitions have actually completed or faulted.

The execution strategy (lock-step, multi-rate, supervisory) is a compositor concern and
is runtime-configurable. The Partition trait is strategy-neutral — it defines the
lifecycle contract without prescribing how the compositor schedules invocations.

For multi-rate execution, "shared context updated at each sub-step" means the partition's
write-buffer slot is overwritten with the latest sub-step result, not that a bus
publication occurs per sub-step. SharedContext is published once per outer tick.

**Rationale:** The compositor's assembly-time role
(selecting and instantiating partition implementations) is complemented by runtime
coordination responsibilities. Making these explicit is necessary because the
compositor sits at the boundary between layers: it is simultaneously the bus owner for
its inner layer (role a-c) and a partition on the outer layer (role d). Without a
defined runtime role, the compositor's responsibilities for relay, fault handling, and
downward data broadcast are ambiguous. The principled split is: lifecycle coordination
for execution control (the compositor is the authority over partition lifecycle), bus
broadcast for shared context (partitions subscribe to typed messages regardless of
source), and bus requests for upward communication (partitions emit, compositor
arbitrates). The degree of execution control varies by strategy: under lock-step
execution, the compositor directly invokes each partition's `step()` and controls
ordering; under fully asynchronous operation, partitions run their own processing loops
while the compositor supervises lifecycle boundaries (init, shutdown, fault detection)
and maintains the bus. In both cases the compositor remains the lifecycle authority —
no partition decides independently when it may start or whether it should continue after
a fault. This flexibility allows FPA-conforming systems to range from lock-step
simulations with in-process trait calls to fully distributed systems with partitions on
separate compute nodes running autonomous processing loops. Because execution strategy is
layer-local, a system built with one strategy can be embedded as a partition in a system
using a different strategy without modification — the compositor at the boundary adapts.
Data freshness metadata ensures that when strategies differ across a layer boundary, the
outer layer can distinguish between freshly computed output and cached state from an
asynchronous partition, enabling informed decisions about whether to proceed, wait, or
use fallback values.

**Verification Expectations:**
- Pass: The compositor controls partition lifecycle: no partition initializes, begins
  processing, or shuts down without the compositor's coordination.
- Pass: Under direct-invocation strategies, the compositor invokes each partition's
  lifecycle methods (`init`, `step`, `shutdown`) in a defined protocol.
- Pass: Under supervisory strategies, the compositor manages initialization and shutdown
  sequencing, detects partition faults (via heartbeat, timeout, or equivalent), and
  publishes shared context — even though partitions schedule their own processing.
- Pass: Shared context (aggregated state, execution state) is available to partitions as
  typed messages on the layer's bus, published by the compositor.
- Pass: A partition consumes shared context using the same bus subscription mechanism it
  uses for peer data — there is no distinct mechanism for compositor-originated vs.
  peer-originated data.
- Pass: The compositor receives typed request messages on its bus and
  arbitrates them as the single owner at its layer.
- Fail: A compositor is inert after assembly — it does not participate in runtime
  communication or bus management.
- Fail: Shared context is passed to partitions exclusively through trait method
  arguments, preventing uniform bus-based consumption.
- Pass: A partition built with one execution strategy (e.g., supervisory coordination
  internally) is composable into a system using a different strategy (e.g., lock-step
  ticks) without modification to either the partition or the outer system.
- Pass: When a compositor's output reflects previously computed state rather than state
  computed for the current invocation, the output carries freshness metadata indicating
  this, as defined in the contract crate.
- Fail: A partition initializes or shuts down without the compositor's involvement.
- Fail: The outer layer's execution strategy constrains or is constrained by the
  execution strategies used at inner layers.

---

### FPA-010 — Compositor Relay Authority

**Statement:** When a compositor receives a request on its inner bus that is relevant to
the outer layer — such as an execution state transition request that must reach the
orchestrator — the compositor shall act as a relay gateway. The compositor shall have
full authority over what crosses its layer boundary: it may relay the request as-is,
transform it (adding context, changing the request type), suppress it (handling
internally without forwarding), or aggregate multiple requests from a single tick into
one consolidated message. Inter-layer requests propagate through the relay chain — each
compositor in the chain independently decides whether to relay — until the request
reaches the layer 0 orchestrator for final arbitration.

**Rationale:** Compositor relay authority preserves the encapsulation guarantee: the
outer layer sees only what the compositor's contract promises, regardless of internal
events. Without relay authority, any inner request would automatically appear on the
outer bus, creating an implicit dependency on the partition's internal structure. A
compositor that relays everything transparently is making a deliberate choice, not
following a default. The relay chain also provides a natural audit point at each layer
boundary, and allows compositors to handle certain situations locally (e.g., falling back
to an alternative sub-partition implementation) without propagating to the outer layer.

**Verification Expectations:**
- Pass: A sub-partition emitting a stop request on a layer 1 bus causes
  the compositor to receive the request and emit a corresponding request on the layer 0
  bus; the orchestrator arbitrates the relayed request.
- Pass: A compositor suppresses an internal request (e.g., by switching to a fallback
  implementation) without any message appearing on the outer bus.
- Pass: A compositor transforms a request before relaying — for example, adding context
  about which sub-partition originated the request.
- Pass: The relay chain operates correctly through multiple layers: a layer 2 request
  relayed by the layer 1 compositor, then relayed by the layer 0 partition, reaches the
  orchestrator.
- Fail: A request from a sub-partition appears on the outer bus without passing through
  the compositor's relay logic.
- Fail: The compositor is a transparent pipe with no ability to filter, transform, or
  suppress inter-layer messages.

---

### FPA-011 — Compositor Fault Handling

**Statement:** When a sub-partition faults during any lifecycle invocation — including
`step()`, `init()`, `shutdown()`, `contribute_state()`, and `load_state()` (or their
equivalents under the active invocation mechanism) — by returning an error, panicking,
or timing out, the compositor at that layer shall catch
the fault, log it with the faulting sub-partition's identity and layer depth, and
propagate the error to the outer layer by returning an error from the compositor's own
trait method call. For the purposes of this requirement, a sub-partition is considered
to have *timed out* on a given trait method call if that call does not return within a
per-invocation elapsed-time deadline enforced by the compositor. The deadline values are
domain-configurable: each compositor instance accepts a timeout configuration specifying
the maximum duration for `step()` and `contribute_state()` calls and for `init()`,
`load_state()`, and `shutdown()` calls. The default values are 50 ms for
step/contribute_state and 500 ms for init/load_state/shutdown. Domains shall configure
values appropriate to their timing constraints (e.g., an industrial process controller
may use 5 ms for step; a kiosk application may use 100 ms). A compositor shall always
enforce finite, non-zero deadlines — deadline enforcement shall not be disabled or set to
an unbounded duration, as this would eliminate the compositor's ability to detect hung
partitions.
These deadlines apply to the trait method call returning, not to the guarantee that all
work initiated by the sub-partition has ceased. Under supervisory coordination, a
sub-partition's `shutdown()` may return promptly after signaling its internal tasks to
stop, while those tasks complete asynchronously (see FPA-009).
These deadlines are per call (not per tick) and are measured using a
monotonic clock. Deadline enforcement is defined in terms of the compositor's observable
behavior: when a per-invocation deadline expires, the compositor shall stop waiting for
the sub-partition's trait method call, record a timeout for that call, and proceed as if
the call had returned an error. The specification does not require the compositor to
synchronously preempt or forcibly cancel the timed-out computation; implementations may
rely on cooperative cancellation, thread or process isolation, or other containment
mechanisms. However, an implementation shall ensure that any work performed by a
sub-partition after its call has been declared timed out cannot affect the correctness
of subsequent compositor decisions for that run (for example, by confining
the call to an isolated worker that is discarded after a timeout). The compositor is
responsible for enforcing these deadlines for its sub-partitions and for treating a
timeout exactly as a fault equivalent to an error return or panic. The error shall
include the compositor's context (which sub-partition faulted, during which operation)
but the failure itself shall not be silently suppressed. The compositor shall respond to
a fault in one of the following ways, in order of preference: (1) propagate the error to
the outer layer by returning an error from the compositor's own trait method call, which
cascades through the compositor chain until the orchestrator receives it; or (2) if a
fallback implementation is configured for the faulting sub-partition, switch to the
fallback, log the fault and the fallback activation, and continue processing — the
compositor does not return an error to the outer layer in this case, but the fault and
fallback are recorded in the compositor's diagnostic log. If no fallback is configured,
the compositor shall always propagate the error (option 1). There shall be no
fault-specific bus channel or message type; the compositor's error return from its own
trait call is the propagation mechanism when errors are propagated.

The compositor shall invoke all sub-partition lifecycle methods through fault-handling
wrappers that catch panics, enforce per-invocation deadlines, and enrich errors with
compositor context. No compositor code path shall call a sub-partition lifecycle method
without these protections.

When a sub-partition fault is propagated to the outer layer, the compositor shall
transition its execution state to Error before returning, preventing further lifecycle
invocations in an inconsistent state. When a fallback is activated, the compositor
remains in its current execution state.

The error returned by the compositor includes context identifying the faulting
sub-partition's identity, layer depth, and the operation that faulted — both in logged
output and in the error value returned to the outer layer.

A fallback implementation configured for a sub-partition shall have the same partition
identity (`id()`) as the primary partition it replaces. The compositor shall reject
registration of a fallback whose identity does not match the target partition.

**Rationale:** A sub-partition fault means the system's state may be invalid.
Silently continuing without a failed sub-component produces incorrect results;
silently omitting a partition's state from a snapshot produces a snapshot that loses
data on reload; running after a failed `init()` means the system was never correctly
assembled. The compositor's role in fault handling is to detect faults — by catching raw panics
under direct invocation, or by detecting heartbeat failures, error reports, or abnormal
termination under supervisory coordination — add diagnostic context (which partition,
which layer, which operation), and either propagate a clean error or activate a
configured fallback.
The outer layer sees the compositor's error return (not the raw panic) when errors are
propagated, preserving encapsulation of the internal structure while ensuring the failure
is visible. When a fallback is configured, the compositor logs the fault and the fallback
activation, allowing the system to continue operating in a degraded mode rather than
halting — this is valuable for systems that prioritize availability or where a
lower-fidelity alternative can safely substitute for the faulted partition. Domain-specific
systems may choose to mandate error propagation (fail-fast) as a policy by not
configuring any fallbacks. A separate fault channel or fault message type would create a
parallel communication path with its own transport, relay, and arbitration semantics —
complexity that is unnecessary because the compositor's call-and-return relationship with
the outer layer already provides an error propagation path.

**Verification Expectations:**
- Pass: A sub-partition returning an error from `step()` is caught by the compositor;
  with no fallback configured, the compositor returns an error from its own `step()`
  call, which cascades to the orchestrator.
- Pass: A sub-partition panicking during `init()` is caught by the compositor; the
  compositor returns an error from its own `init()` call, preventing the run
  from starting.
- Pass: A sub-partition returning an error from `contribute_state()` is caught by the
  compositor; the compositor returns an error from its own `contribute_state()` call,
  and the dump operation fails with a diagnostic identifying the faulting sub-partition.
- Pass: The error returned by the compositor includes context identifying the faulting
  sub-partition's identity, layer depth, and the operation that faulted.
- Pass: The compositor logs the fault with full details regardless of whether the outer
  layer also logs it.
- Pass: When a fallback is configured for a sub-partition and that sub-partition faults
  during `step()`, the compositor activates the fallback, logs the fault and fallback
  activation, and continues processing without returning an error.
- Fail: A sub-partition fault is silently absorbed by the compositor without logging and
  without either propagating the error or activating a configured fallback.
- Fail: A sub-partition fault propagates as a raw panic or unhandled exception without
  compositor context wrapping.
- Pass: A compositor constructed with domain-specific timeout values enforces those
  values rather than the defaults.
- Pass: A compositor always enforces some finite, non-zero deadline — it is not possible
  to disable deadline enforcement.
- Fail: A compositor operates with infinite or disabled deadline enforcement.
- Fail: A dedicated fault bus or fault message channel exists alongside the regular bus.

---

### FPA-012 — Recursive State Contribution

**Statement:** A partition that is itself a compositor shall implement the state
contribution contract (FPA-022) by recursively invoking `contribute_state()` on its
sub-partitions and assembling their contributions into a nested TOML fragment. The
compositor's contribution shall include both its own internal state and the nested
contributions of its children, preserving the hierarchical structure. Loading a snapshot
shall reverse the process: the compositor shall decompose its fragment section along the
same boundaries used for assembly and delegate each sub-section to the corresponding
sub-partition's `load_state()` implementation. The outer layer shall receive one
contribution per partition — it shall not observe the internal decomposition.

**Rationale:** State snapshots must capture state at every layer without breaking
encapsulation. If the orchestrator needed to know about sub-partitions to collect their
state, replacing a partition's internal structure would require modifying the
orchestrator. Recursive delegation through compositors ensures that each layer handles
its own collection and assembly. Nested TOML fragments preserve compatibility with the
`extends` and override mechanisms (FPA-020, FPA-021) — a snapshot fragment can
be used as a configuration input, and state values at any depth can be overridden using
the same composition semantics as all other configuration.

**Verification Expectations:**
- Pass: A compositor at layer 1 calls `contribute_state()` on each of its sub-partitions
  and assembles the results into a nested TOML fragment under its partition's key.
- Pass: The orchestrator receives one state contribution per layer 0 partition; it does
  not observe whether the contribution was assembled from sub-partitions or produced
  monolithically.
- Pass: A state snapshot captured from a compositor partition, when loaded via
  `load_state()`, correctly restores the state of each sub-partition through recursive
  decomposition.
- Pass: The nested fragment structure is compatible with `extends` inheritance — a
  fragment that extends a snapshot can override a specific sub-partition's state value.
- Fail: The orchestrator must enumerate or know about sub-partitions to capture a
  complete state snapshot.
- Fail: State contributions from sub-partitions are flat key-value pairs rather than
  nested TOML sections, breaking composition fragment compatibility.

---

### FPA-013 — Direct Signals

**Statement:** A contract crate at any layer may declare a set of **direct signal
types** — safety-critical signals that bypass the compositor relay chain within that
contract crate's jurisdiction and reach the orchestrator that owns that contract crate
directly, regardless of the emitting partition's depth within the hierarchy. Direct
signals are scoped to the contract crate that declares them: signals declared in
a system's contract crate reach that system's orchestrator; they do not propagate beyond
the system's boundary when the system is embedded as a partition in an outer system. An
outer system that embeds the inner system may declare its own direct signals in its own
contract crate, independent of the inner system's signals. At runtime, the contract
crate boundary is enforced by the compositor boundary. Each compositor maintains a signal
registry defining which direct signal types are recognized at its layer. When collecting
signals from inner compositors, the outer compositor filters against its own registry —
only signals whose type identifier is registered at the outer layer propagate.
Unregistered signals are silently dropped at the boundary, analogous to the compositor's
relay authority for bus messages (FPA-010). This makes boundary scoping explicit and
declarative: an outer compositor opts in to each signal type it receives from inner
layers. Any partition within the
declaring contract crate's hierarchy may emit a declared direct signal. Direct signals
shall carry minimal payload: a signal type identifier, a reason string, and the identity
of the emitting partition. Every direct signal emission shall be logged with the emitting
partition's identity and layer depth. The set of direct signal types at any layer shall
be small, stable, and reserved for scenarios where compositor relay suppression would be
unsafe (e.g., emergency stop, hardware fault).

**Rationale:** The compositor relay chain (FPA-010) is the correct default for
inter-layer communication — it preserves encapsulation and gives each compositor control
over what crosses its boundary. However, for safety-critical signals, the cost of a
compositor inadvertently suppressing or delaying a relay exceeds the cost of bypassing
encapsulation. A hardware fault or emergency condition detected deep in the hierarchy
must reach the responsible orchestrator without depending on every compositor in the
chain making the correct relay decision. Direct signals are the escape hatch: declared
sparingly, constrained to minimal payload, and audited. They complement the relay chain
rather than replacing it — normal inter-layer communication uses compositor relay,
safety-critical communication uses direct signals. Scoping direct signals to the
declaring contract crate's jurisdiction preserves encapsulation at the embedding
boundary: when the system is a partition in an outer system, its internal direct signals
are an implementation detail invisible to the outer system. The outer system defines its
own safety mechanisms in its own contract crate. The inner system communicates the
outcome of internal emergencies through its contract interface on the outer bus, just as
any other partition would.

**Verification Expectations:**
- Pass: A sub-partition at layer 2 emitting an emergency stop direct signal causes
  the system's orchestrator to receive the signal without any intermediate compositor
  relay step.
- Pass: When the system is embedded as a partition in an outer system, a direct signal
  declared in the system's contract crate reaches the system's orchestrator but does not
  appear on the outer system's bus. The outer system learns of the event only through
  the system's contract interface (e.g., a status change or request on the outer bus).
- Pass: An outer system declares its own direct signal types in its own contract crate,
  independent of the inner system's direct signal types.
- Pass: Every direct signal emission is logged with the emitting partition's identity
  and layer depth.
- Pass: A direct signal carries only a type identifier, reason string, and emitter
  identity — not arbitrary data payloads.
- Fail: A compositor at an intermediate layer intercepts or suppresses a direct signal
  within the declaring contract crate's hierarchy.
- Fail: A direct signal declared in the system's contract crate propagates beyond the
  system's boundary when the system is embedded as a partition in an outer system.
- Fail: Direct signals are used for non-safety-critical communication that could be
  handled through the normal compositor relay chain.
- Pass: A signal type emitted within an inner compositor's hierarchy but not registered
  at the outer compositor's layer is dropped at the compositor boundary and does not
  reach the outer orchestrator.
- Pass: The outer compositor's signal registry is the sole mechanism determining which
  inner signals cross the boundary.
- Fail: The set of direct signal types is large or changes frequently, indicating misuse
  as a general communication mechanism.

---

## 6. Configuration and Composition

---

### FPA-019 — Composition Fragments

**Statement:** The system shall use TOML files as its primary runtime configuration
interface. Each file is a composition fragment at a specific scope within the fractal
structure. A layer 0 fragment specifies system-level parameters (timestep,
transport mode) and references one or more layer 1 fragments. A layer 1 fragment
specifies partition-level content: sub-component selections, events, initial conditions,
and partition-specific configuration. Composition fragments at narrower scopes follow the
same structure. An outer-layer fragment may inline or override any field that would
otherwise be defined by an inner-layer fragment it composes; this allows a layer 0
fragment to fully define layer 1 content inline without referencing separate fragment
files, or to override individual fields of a referenced fragment. The system shall accept
a layer 0 composition fragment as its entry point. When no fragment is specified, the
orchestrator shall load a default configuration. When a path to a fragment is provided
as a command-line argument, the orchestrator shall use it instead of the default.

**Rationale:** A human-readable, file-based configuration surface separates operational
intent from implementation. Structuring configuration as composition fragments at every
layer — rather than a single monolithic file — mirrors the fractal partition pattern:
the same TOML structure, override semantics, and inheritance rules apply at every scope.
Layer 0 fragments, layer 1 fragments, sub-component selections, and presets are all
portable artifacts that can be independently version-controlled, exchanged between teams,
and reused across different compositions without access to system source code.

**Verification Expectations:**
- Pass: The orchestrator launched with no arguments loads the default configuration and
  executes a complete run.
- Pass: The orchestrator launched with a path to a custom fragment loads the specified
  fragment instead of the default and executes a complete run.
- Pass: A layer 0 fragment referencing a layer 1 fragment by path correctly composes
  the layer 1 fragment's sub-component selections, events, and configuration into the
  run.
- Pass: A layer 0 fragment that inlines all layer 1 content (sub-component selections,
  events, initial conditions) without referencing any external fragment executes a
  complete run.
- Pass: A layer 0 fragment that references a layer 1 fragment and overrides a single
  field produces a run that matches the referenced fragment in all respects except
  the overridden field.
- Pass: Two operators on different machines produce identical initial state
  from the same fragments (excluding non-deterministic transport effects).
- Fail: Any parameter that is documented as configurable requires a source
  code change or environment variable to override.
- Fail: Configuration at any scope requires a format or structure distinct from the
  TOML composition fragment format used at other scopes.

---

### FPA-020 — Composition Fragment Inheritance

**Statement:** Any TOML composition fragment shall support an `extends` field whose
value is a path to a base fragment of the same scope. Fields present in the inheriting
fragment shall override the corresponding fields in the base. Fields absent from the
inheriting fragment shall be inherited unchanged from the base. This mechanism shall be
available at every scope: layer 0 fragments, layer 1 fragments, sub-component
selections, and configuration presets. Additionally, an outer-layer fragment shall be
able to override fields defined by any inner-layer fragment it composes: a layer 0
fragment may override layer 1 fragment fields, a layer 1 fragment may override
sub-component fields, and so on to arbitrary depth.

**Rationale:** Variants of a composition fragment at any scope are typically small diffs
against a common base — a fragment that differs only in one sub-component, a layer 0
fragment that differs only in transport mode, a sub-component configuration that differs
only in initial conditions. Inheritance allows these variants to be expressed minimally,
keeping them automatically synchronized with base fragment changes and reducing the
maintenance burden of configuration libraries. Cross-layer overrides allow outer
fragments to customize inner fragments without forking them: an operator can distribute a
standard configuration and have each layer 0 fragment override only the parameters
relevant to that run, or a single layer 0 file can fully define all content without
separate fragment files. Consistent with the fractal partition pattern, the same
inheritance and override mechanism applies at every layer and scope.

**Verification Expectations:**
- Pass: A fragment containing only `extends = "base.toml"` and a single
  overridden field produces a run that differs from the base only in
  the overridden field's effect.
- Pass: A layer 0 fragment extending a base layer 0 fragment inherits the base's
  references and parameters, overriding only specified fields.
- Pass: A layer 0 fragment that references a layer 1 fragment by name and overrides a
  sub-component within that fragment produces a run where only the overridden
  sub-component differs from the named fragment's defaults.
- Pass: Modifying a shared field in a base fragment is reflected in all inheriting
  fragments without modifying the inheriting files.
- Fail: A circular `extends` chain (A extends B extends A) is silently accepted; the
  system shall detect and report it as a configuration error.
- Fail: The `extends` mechanism is available only for one scope; other scopes
  require a different override mechanism.

---

### FPA-021 — Named Composition Fragments

**Statement:** Any composition fragment reference within a TOML file shall be
expressible as either an inline table or a named reference resolvable to a fragment file
in a configurable directory. Named fragments shall be applicable at any scope within the
fractal structure — sub-component selections, plugin sets, partition configurations,
or layer 0/layer 1 fragments. Inline overrides shall be supported to modify individual
fields within a named fragment without defining an entirely new fragment (via the
inheritance mechanism of FPA-020).

**Rationale:** Named composition fragments are a natural consequence of the fractal
partition pattern: since every scope composes independently replaceable sub-components
via the same compositor mechanism, a reusable named selection of those sub-components is
useful at every layer and in every domain. Named fragments give operators a stable,
communicable vocabulary (e.g., "run your implementation against the standard
configuration") while inline overrides allow individual field substitution without
creating a new fragment. Because the naming and override mechanism is the same at every
scope, there is no domain-specific preset system — all named selections at any scope
are composition fragments.

**Verification Expectations:**
- Pass: A partition referencing a named sub-component selection produces the same behavior
  as one with an inline table containing the fields defined in the corresponding named
  fragment file.
- Pass: An inline override of a single field in an otherwise named fragment
  activates only that difference, leaving all other selections by the fragment unchanged.
- Pass: A named fragment at any scope (e.g., a visualization plugin set, a sub-component
  selection) selects a specific set of implementations, and inline overrides add or
  remove individual elements without replacing the entire fragment.
- Pass: A layer 0 fragment referencing a layer 1 fragment by name resolves to the
  corresponding named fragment file and composes its contents into the run.
- Fail: The system accepts an unrecognized fragment name without reporting an error.
- Fail: The named fragment mechanism is available only at specific scopes; other scopes
  require a different mechanism to achieve named sub-component selections.

---

### FPA-015 — Standard Composition Entry Point

**Statement:** The system shall provide a standard composition function that serves
as the primary entry point for operators, embedders, and system tests. The function
accepts three inputs: (a) a composition fragment (FPA-019) specifying the system
configuration, (b) a partition registry mapping implementation names to partition
constructors, and (c) a bus instance for the layer's transport (FPA-004). The function
creates partitions from the composition fragment via registry lookup, wires events
declared in the configuration (FPA-028), and returns a compositor ready for lifecycle
execution. Partition creation shall always go through the registry — the composition
function shall not accept pre-constructed partition instances. The composition function
operates at any layer: an inner compositor is composed from its own fragment, registry,
and bus, consistent with the fractal partition pattern (FPA-001).

**Rationale:** The composition fragment (FPA-019) defines what a system looks like;
the composition function defines how that description becomes a running system. Without
a standard entry point, each application invents its own composition pattern, breaking
uniformity and making FPA-034 (system tests use operator entry points) unverifiable
against a concrete API. Registry-based partition creation enforces runtime
configurability (FPA-002) — implementations are selected by name in configuration,
not hardcoded in application code. The three-input signature (fragment, registry, bus)
separates the three concerns that vary independently: system structure, available
implementations, and transport mode.

**Verification Expectations:**
- Pass: System tests, interactive applications, and embedders all invoke the same
  composition function, differing only in the composition fragment and bus provided.
- Pass: Changing which partition implementation is used for a given role requires only
  a change to the composition fragment's implementation name, not to application code.
- Pass: The composition function operates identically at layer 0 and at inner layers.
- Fail: A system test or application constructs a compositor by directly instantiating
  partitions, bypassing the composition function and registry.
- Fail: The composition function accepts pre-constructed partition instances, bypassing
  registry lookup.

---

## 7. State Management

---

### FPA-022 — State Snapshot as Composition Fragment

**Statement:** The system shall support capturing the complete state at any
point during execution and emitting it as a TOML composition fragment. The contract
crate for each layer shall define a state contribution contract that each partition at
that layer implements. The orchestrator shall assemble the complete snapshot by invoking
each partition's state contribution implementation and composing the results into a
single fragment. A partition that is itself a compositor shall implement the state
contribution contract by recursively invoking `contribute_state()` on its sub-partitions
and assembling their contributions into a nested TOML fragment (see FPA-012). Loading
a snapshot shall use the same contract in reverse: the orchestrator decomposes the
fragment and passes each partition's section to that partition's load implementation; a
compositor partition decomposes its section further and delegates to its sub-partitions.
The resulting snapshot fragment shall be a valid composition fragment loadable by the same
mechanism used for layer 0 and layer 1 fragments (FPA-019). The snapshot shall include
the current time, execution state, and the complete internal state of each
partition for all active entities. The snapshot fragment shall be human-readable,
inspectable, and editable using standard text tools.

**Rationale:** State persistence is a natural extension of the composition fragment
mechanism rather than a separate system. Initial conditions are already composition
fragment content — a state snapshot is a complete set of initial conditions captured at
a specific point in time. By expressing snapshots as composition fragments, they
inherit all existing fragment capabilities: `extends` inheritance (FPA-020), named
references (FPA-021), inline overrides, and cross-layer override semantics. An
operator can extend a snapshot and override a single entity's state, or reference a
named snapshot in a layer 0 fragment. Defining the state contribution contract in the
contract crate (consistent with FPA-003) ensures that all partitions serialize and
deserialize state through a uniform interface rather than each inventing its own
mechanism. The orchestrator coordinates dump and load without needing knowledge of any
partition's internal state structure — it invokes the contract and composes or
decomposes the resulting fragment sections. Recursive delegation through compositors
ensures that the snapshot captures state at every layer without the orchestrator needing
to know the internal decomposition of any partition.

**Verification Expectations:**
- Pass: A state snapshot captured during active execution produces a valid TOML file
  that parses without error as a composition fragment.
- Pass: The snapshot fragment, when loaded as the basis for a new run, produces a
  system whose initial state matches the captured state (within floating-point
  determinism limits).
- Pass: A snapshot fragment supports the `extends` field: a fragment containing only
  `extends = "snapshot.toml"` and a single overridden entity state produces a
  run that differs from the snapshot only in the overridden entity's initial
  state.
- Pass: Each partition's state section in the snapshot uses the same TOML structure as
  the corresponding partition's configuration section in a layer 1 fragment.
- Pass: A snapshot fragment is usable as a named fragment: referencing a snapshot by name
  resolves to the snapshot file and loads correctly.
- Fail: State snapshots are emitted in a binary or opaque format that cannot be parsed
  as a TOML composition fragment.
- Fail: Loading a snapshot requires a mechanism distinct from the composition fragment
  loader used for layer 0 and layer 1 fragments.
- Pass: The state contribution contract is defined in the contract crate for the layer;
  no partition defines its own serialization interface for state snapshots.
- Pass: An alternative partition implementation that conforms to the state contribution
  contract produces a snapshot whose section is loadable by the orchestrator without
  modification to the orchestrator or any other partition.
- Fail: The snapshot omits partition state that would be necessary to reproduce the
  captured state on reload.
- Fail: A partition implements state dump or load through a mechanism other than the
  contract defined in the layer's contract crate.

---

### FPA-023 — State Dump and Load Operations

**Statement:** The system shall provide operations to dump and load state.
A dump operation shall capture the current state and write it to a specified file path
as a composition fragment (FPA-022). A load operation shall accept a state snapshot
composition fragment and restore the system to the captured state, replacing the
current state of all partitions and entities. Dump shall be invocable while the system
is actively processing or while processing is idle. Load shall be invocable only while
processing is idle — specifically, when no partition lifecycle methods are in flight AND
the execution state machine is in a non-processing state (e.g., Paused or
Uninitialized). For lock-step compositors, this is inherently satisfied since `load()`
and `step()` cannot execute concurrently. For supervisory compositors, the compositor
must pause partition tasks before loading state. Loading while partitions are actively
stepping shall not be supported. When the system is actively processing, the
implementation shall ensure
temporal consistency — all partition contributions shall correspond to the same completed
processing cycle. When the tick lifecycle convention (FPA-014) is adopted, this is
achieved by processing dump at a tick boundary in Phase 1. Both operations shall be
requestable via the bus, a UI partition, and the event system (as event actions),
consistent with the uniform request mechanisms used for shared state machine transitions
(FPA-006).

**Rationale:** Dump and load are the operational interface to state snapshots. Dump
during active execution enables capturing transient conditions without stopping;
dump while idle enables deliberate checkpointing. Restricting load to idle states
prevents mid-step state corruption. Ensuring temporal consistency for dump operations
guarantees that all partition contributions correspond to a single point in time
regardless of execution strategy. Making dump and load available through the same
request channels as other bus-mediated operations — bus messages, UI controls, and
event actions — keeps the operational surface uniform. An event action for state dump
with a path parameter allows configuration authors to script automatic checkpoints at
specific times or conditions without UI interaction.

**Verification Expectations:**
- Pass: A dump operation invoked while the system is actively processing produces a valid
  snapshot fragment without interrupting execution.
- Pass: A dump captured during active processing contains state from a single completed
  processing cycle — all partition contributions correspond to the same point in time.
- Pass: A dump operation invoked while processing is idle produces a snapshot fragment,
  and loading that fragment in a new run produces identical initial state.
- Pass: A load operation invoked while processing is idle replaces all entity and
  partition state with the snapshot's state; on resumption, the system continues from
  the loaded state.
- Pass: An event defined with a state dump action and a file path parameter
  triggers at the specified condition and produces a snapshot file at the given path.
- Pass: A load request emitted while the system is actively processing is rejected
  (logged and ignored), and the system continues unaffected.
- Fail: Dump or load requires a dedicated API distinct from the bus message and event
  action mechanisms used for other bus-mediated operations.
- Fail: A dump captured during active processing contains partition states from different
  processing cycles.

---

## 8. Events

---

### FPA-024 — Event System Architecture

**Statement:** The system shall provide a unified event system in which discrete events
can be defined, armed, triggered, and handled at every layer of the fractal partition
hierarchy. The event mechanism — trigger types, arming lifecycle, configuration schema,
and evaluation semantics — shall be identical at every layer and in every partition,
consistent with the fractal partition pattern (FPA-001). The semantic meaning of events —
what actions they invoke and what domain concerns they express — shall vary by layer and
partition. Each layer's contract crate defines the action vocabulary available to events
scoped at that layer (see FPA-029).

**Rationale:** The fractal partition pattern requires that structural primitives be
uniform in kind across all layers. The event primitive is no exception: its mechanism
(trigger + action + parameters, declaratively defined in configuration) is the same
everywhere. But just as the bus carries different typed messages at different layers, and
contracts specify different behavioral obligations at different layers, events express
different domain concerns at different layers. Layer 0 events address infrastructure and
execution lifecycle (output snapshots, health checks, execution state transitions).
Layer 1 events address domain concerns (phase transitions, failure injection, mode
changes). Layer 2+ events address sub-component-internal concerns (model transitions,
convergence thresholds). The event mechanism is the uniform primitive; the action
vocabulary is the layer-scoped semantic content.

**Verification Expectations:**
- Pass: A system-level event and a partition-level event defined in the same
  configuration both trigger at their specified conditions during a run.
- Pass: A partition-level event defined within a partition uses the same event
  definition schema and trigger types as a system-level event defined outside any
  partition scope.
- Pass: A partition-level event uses a domain-specific action identifier defined in that
  partition's contract crate, and the action is handled within the partition without
  requiring system-level awareness of the action type.
- Fail: A partition implements its own ad-hoc event mechanism that does not conform to
  the system event interface defined in the contract crate.
- Fail: Events can only be defined at the system level; partition-scoped events are not
  supported.

---

### FPA-025 — Time-triggered Events

**Statement:** The event system shall support events triggered at a specified time.
Consistent with the fractal partition pattern (FPA-001), time semantics vary by
layer: at layer 0 (system level), time-triggered events shall reference wall-clock time
elapsed since system start; at layer 1 (partition level), time-triggered events shall
reference logical time as defined by the system clock. Logical time shall be tracked as
the cumulative sum of `dt` values passed to the compositor's step invocations, not
derived from tick count multiplied by the current `dt`.

The event engine is time-semantic-agnostic. The compositor at each layer passes the
appropriate time basis to the event engine: wall-clock elapsed time at layer 0, or
logical/simulation time at deeper layers.

**Rationale:** Wall-clock triggers at layer 0 support infrastructure concerns such as
output snapshots, periodic health checks, and real-time synchronization boundaries
that are independent of logical time scaling. Logical-time triggers at layer 1
(partition level) support timeline events (e.g., "trigger action at T+120s")
that must track the system clock, including during time-scaled or paused execution.

**Verification Expectations:**
- Pass: A layer 0 event configured to trigger at wall-clock T+5s fires at approximately
  5 seconds of real elapsed time, regardless of whether the system is running at 0.5x
  or 2x real-time speed.
- Pass: A layer 1 event configured to trigger at logical time T+60s fires when the
  system clock reaches 60 seconds, regardless of wall-clock elapsed time.
- Pass: A layer 1 time-triggered event does not advance while the system is paused.
- Fail: A logical-time event fires based on wall-clock time, causing it to trigger at
  the wrong phase when time scaling is active.

---

### FPA-026 — Condition-triggered Events

**Statement:** The event system shall support events triggered by logical conditions
evaluated against observable signals. A condition shall be expressible as a boolean
predicate over one or more named signals (e.g., `value_a < 100.0`,
`value_b > 1.0 && value_c > 500.0`). The set of observable signals shall include
any value published on the bus or exposed as a named field within a
partition's state. Equality predicates (`==`) shall use exact floating-point comparison.
Configuration authors requiring tolerance-based comparison should express this using
compound predicates (e.g., `value > threshold - epsilon && value < threshold + epsilon`).

**Rationale:** Many events are defined not by clock time but by runtime
conditions: triggering at a threshold, changing mode when a measurement enters a
specific range, or initiating an action when a state variable reaches a boundary.
Condition triggers allow configuration authors to express event logic declaratively
without embedding procedural checks in partition source code.

**Verification Expectations:**
- Pass: An event conditioned on `value_a < 500.0` triggers on the first tick in
  which the monitored signal falls below 500, and does not trigger
  while the signal remains above 500.
- Pass: A compound condition using logical AND over two signals triggers only when both
  sub-conditions are simultaneously satisfied.
- Pass: Condition predicates reference signals by name as published on the bus or defined
  in partition state, without requiring the event author to specify memory addresses or
  internal data paths.
- Pass: A condition-triggered event with an action that requests a shared state machine
  transition causes the appropriate state change on the first tick the condition is met
  (see FPA-006).
- Fail: Condition-triggered events can only monitor system-level signals; partition-
  internal state is not observable by the event system.

---

### FPA-027 — Partition-scoped Event Arming

**Statement:** Each partition shall be capable of defining and arming events scoped to
its own domain (layer 1). Partition-scoped events shall be evaluated against that
partition's internal signals and shall invoke handlers within the partition's execution
context. Consistent with the fractal partition pattern (FPA-001), the mechanism
for defining and arming events shall be uniform across all partitions and identical in
structure to system-level (layer 0) event definitions.

**Rationale:** The fractal partition pattern requires that each partition have the same
event capabilities as the system level. Partitions contain domain-specific state that may
not be published on the inter-partition bus but is meaningful for triggering domain-
specific behavior. A partition may arm an event on an internal convergence
metric; another partition may arm an event on a load threshold; yet another
partition may arm a transition on proximity to a waypoint. Requiring all such
events to be defined at the system level would violate partition encapsulation and create
coupling between the event configuration and partition internals.

**Verification Expectations:**
- Pass: A partition arms an event on an internal signal not published to the bus,
  and the event triggers correctly when the condition is met during partition execution.
- Pass: Two partitions each arm independent events using the same event definition
  schema, and both trigger correctly without interference.
- Pass: A partition-scoped event is defined in the partition's section of the
  configuration and does not require modification to any other partition's configuration.
- Pass: A partition event armed on an internal signal fires an action declared in the
  system-level contract crate, and the resulting request reaches the orchestrator through
  the relay chain (see FPA-010).
- Fail: Arming an event within a partition requires modifying the system-level contract
  crate or the system-level event dispatcher source code.

---

### FPA-028 — Event Definition in Configuration

**Statement:** Events shall be declaratively defined in configuration files.
Layer 0 (system-level) events shall be defined in a top-level `[[events]]` array.
Layer 1 (partition-level) events shall be defined within the corresponding partition's
configuration section (e.g., `[[partition_a.events]]`, `[[partition_b.events]]`).
Each event entry shall specify a trigger (time or condition), an action identifier, and
optional parameters. Consistent with the fractal partition pattern (FPA-001), the
event entry schema shall be identical at both layers.

**Rationale:** Declarative event definition in the configuration file keeps timelines
version-controlled, portable, and inspectable alongside other configuration.
The fractal partition pattern requires that the same event schema be usable at every
layer, so that authors learn one event syntax and apply it uniformly. It avoids
hard-coding event logic in partition source code and allows operators to modify event
sequences without recompilation.

**Verification Expectations:**
- Pass: A configuration containing a partition-level event entry with a time trigger and
  an action identifier causes the specified action to execute in the partition at
  the specified time.
- Pass: A configuration containing both system-level and partition-level event entries
  executes both event categories correctly in the same run.
- Pass: Modifying the trigger time or condition of an event requires only a change to the
  configuration; no source code modification is needed.
- Fail: Events can only be defined programmatically in source code; no configuration-file
  representation exists.

---

### FPA-029 — Contract-crate-scoped Event Action Vocabulary

**Statement:** All event action identifiers shall be declared in contract crates. The
set of action identifiers available to events at a given scope is the union of actions
declared in the contract crates that the partition at that scope transitively depends on.
Each contract crate declares its action vocabulary as part of its contract interface,
alongside trait definitions and typed message declarations. The event configuration
schema — trigger, action identifier, and parameters — shall be identical regardless of
which contract crate declares the action. Action identifiers shall be validated at
configuration load time: an action identifier used in an event entry must be declared in
a contract crate visible at that event's scope, or the configuration shall be rejected.

There is one mechanism for declaring event actions, one mechanism for dispatching them,
and one configuration schema for defining them. What varies across layers is the action
vocabulary — which actions are available — determined by the contract-crate dependency
graph.

**Rationale:** The fractal partition pattern produces a system where events at different
layers express fundamentally different domain concerns. Layer 0 events address
infrastructure: execution lifecycle, output checkpoints, periodic health checks.
Layer 1 events address domain concerns: phase transitions, failure injection, mode
changes. Layer 2+ events address sub-component-internal concerns: model transitions,
convergence thresholds, internal mode switches.

Actions declared in the system-level contract crate are available at every layer because
every partition transitively depends on the system-level contract crate. Actions declared in a
partition's contract crate are available to events within that partition because the
partition's implementation depends on its contract crate. The system-level contract
crate's actions are available everywhere for the same reason its typed messages are
available everywhere — the dependency graph makes them visible.

This mirrors how all other contract-crate-scoped primitives work. Typed messages are
declared in contract crates and available wherever the dependency graph reaches. Traits
are declared in contract crates and implemented by partitions that depend on those
crates. Event actions follow the same pattern: declared in contract crates, available
wherever the dependency graph reaches, handled by the partition whose dispatcher owns
that scope.

Without contract-crate scoping, the event system must either restrict actions to a
hardcoded set — forcing partitions to encode domain logic procedurally rather than
declaratively — or maintain a single global action registry that couples all partitions
to each other's domain vocabulary. Contract-crate scoping avoids both: each
contract crate owns its action namespace, the dependency graph determines visibility,
and no global coordination is needed.

**Verification Expectations:**
- Pass: An action declared in the system-level contract crate is usable in a layer 1
  partition event because the partition transitively depends on the system-level contract
  crate.
- Pass: An action declared in a partition's contract crate is used in a partition-level
  event entry and handled by that partition's event dispatcher.
- Pass: An action declared in one partition's contract crate is rejected at configuration
  load time if used in another partition's event entry (when the second partition does
  not depend on the first partition's contract crate).
- Pass: Event entries using actions from the system-level contract crate and actions from
  a partition's contract crate have identical configuration syntax — the same trigger,
  action, and parameters fields.
- Pass: An action's effects on the outer layer are visible only through whatever the
  partition publishes on the outer bus as part of its normal contract obligations — not
  through a separate event propagation mechanism.
- Fail: Event actions are defined in a global registry visible to all partitions,
  creating cross-partition coupling on domain vocabulary.
- Fail: A partition handles domain-specific event logic procedurally in its `step()`
  implementation because the event system does not support contract-crate-scoped actions.
- Fail: Actions declared in the system-level contract crate use a different declaration
  mechanism, dispatch path, or configuration schema than actions declared in a
  partition's contract crate.

---

### FPA-040 — Contract Crate Naming and Documentation

**Statement:** The contract crate at each layer shall follow the naming convention
`<partition>-contract`, where `<partition>` is the short name of the system or partition
that owns the contract. At layer 0, this is `<system>-contract` (e.g.,
`universe-contract`). At layer 1, this is `<partition>-contract` (e.g.,
`physics-contract`). When the contract crate is a module within a partition's crate
rather than a standalone crate, the module shall be named `contract`.

Each contract crate shall maintain a `docs/` directory following the same Diataxis
documentation structure defined in FPA-030. The contract crate's
`docs/design/SPECIFICATION.md` serves as the Interface Control Document (ICD) for that
layer's inter-partition boundary. It specifies the types, traits, delivery semantics,
shared state machines, and event action vocabularies exported by the contract crate. Its
requirements trace to the parent layer's specification.

**Rationale:** The contract crate is the single source of truth for inter-partition
interfaces (FPA-003). A predictable naming convention makes the contract crate
discoverable by inspection — a contributor encountering any partition can locate
its contract crate without searching. Requiring the same Diataxis documentation
structure as partitions ensures the interface is specified to the same standard as
implementations, maintaining the layer and partition uniformity principle. The ICD is not
a new document type — it is the contract crate's `SPECIFICATION.md`, containing
requirements that trace upward to the parent layer's specification just as partition
specifications do.

**Verification Expectations:**
- Pass: The system-level contract crate is named `<system>-contract` (or `contract` as a
  module) and contains a `docs/design/SPECIFICATION.md` with requirements tracing to the
  system specification.
- Pass: A partition that decomposes into sub-partitions has a contract crate or module
  named `<partition>-contract` (or `contract`) with a `docs/design/SPECIFICATION.md`
  tracing to the partition specification.
- Pass: The contract crate's `docs/` directory follows the same Diataxis structure
  (tutorials, how-to, reference, explanation, design) used by partitions.
- Fail: A contract crate has no `docs/design/SPECIFICATION.md` or its requirements do
  not trace to the parent layer's specification.
- Fail: The contract crate uses an ad-hoc name that does not identify it as a contract
  crate by convention.

---

## 9. Requirements Index

| ID      | Title                                                          |
|---------|----------------------------------------------------------------|
| FPA-001 | Fractal Partition Pattern                                      |
| FPA-002 | Partition Independence                                         |
| FPA-003 | Inter-partition Interface Ownership                            |
| FPA-004 | Transport Abstraction                                          |
| FPA-005 | Typed Message Contracts                                        |
| FPA-006 | Shared State Machine Synchronization                           |
| FPA-007 | Bus Delivery Semantics                                         |
| FPA-008 | Layer-scoped Bus                                               |
| FPA-009 | Compositor Runtime Role                                        |
| FPA-010 | Compositor Relay Authority                                     |
| FPA-011 | Compositor Fault Handling                                      |
| FPA-012 | Recursive State Contribution                                   |
| FPA-013 | Direct Signals                                                 |
| FPA-015 | Standard Composition Entry Point                               |
| FPA-019 | Composition Fragments                                          |
| FPA-020 | Composition Fragment Inheritance                               |
| FPA-021 | Named Composition Fragments                                    |
| FPA-022 | State Snapshot as Composition Fragment                         |
| FPA-023 | State Dump and Load Operations                                 |
| FPA-024 | Event System Architecture                                      |
| FPA-025 | Time-triggered Events                                          |
| FPA-026 | Condition-triggered Events                                     |
| FPA-027 | Partition-scoped Event Arming                                  |
| FPA-028 | Event Definition in Configuration                              |
| FPA-029 | Contract-crate-scoped Event Action Vocabulary                  |
| FPA-040 | Contract Crate Naming and Documentation                        |
