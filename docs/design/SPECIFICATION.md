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
6. [Execution Model](#6-execution-model)
7. [Configuration and Composition](#7-configuration-and-composition)
8. [State Management](#8-state-management)
9. [Events](#9-events)
10. [Verification and Testing](#10-verification-and-testing)
11. [Requirements Index](#11-requirements-index)

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

### Companion Documents

The following companion explanation documents provide conceptual discussion and
worked examples for the patterns defined in this specification:

- `fractal-partition-pattern.md`
- `communication-in-the-fractal-partition-pattern.md`
- `inter-partition-communication.md`
- `inter-layer-communication.md`
- `events-as-a-fractal-primitive.md`
- `testing-in-the-fractal-partition-pattern.md`
- `test-reference-data-in-the-fractal-partition-pattern.md`
- `tick-lifecycle-and-synchronization.md`

---

## 2. Definitions and Abbreviations

| Term              | Definition                                                                 |
|-------------------|----------------------------------------------------------------------------|
| Compositor        | A component that selects and assembles partition implementations at startup and, at runtime, owns the layer's bus instance, drives partition execution via trait calls, publishes shared context on the bus, arbitrates requests, and relays inter-layer messages to the outer bus with authority to filter, transform, or suppress |
| Contract crate    | A module or package that defines traits and data types but contains no implementation. In a Rust realization, this is a Rust crate; other technologies may use equivalent constructs |
| Composition fragment | A configuration block — inline or named — that selects partition implementations at a given scope within the fractal structure. A top-level composition fragment at layer 0 selects system-wide parameters. A composition fragment at layer 1 selects partition-level parameters. All composition fragments share the same override and inheritance semantics (see FPA-020, FPA-021) |
| Delivery semantic | A per-message-type specification of how the bus delivers messages to consumers. Latest-value retains only the most recent value (suitable for continuous state). Queued retains all messages in order (suitable for requests that must not be dropped). Declared in the contract crate alongside the type. See FPA-007 |
| Direct signal     | A safety-critical signal type declared in a contract crate that bypasses the compositor relay chain within that contract crate's hierarchy and reaches the declaring crate's orchestrator directly. Scoped to the declaring crate's jurisdiction — does not propagate beyond the boundary when the system is embedded as a partition in an outer system. Reserved for scenarios where compositor suppression would be unsafe (e.g., emergency stop, hardware fault). See FPA-013 |
| Event action      | An action identifier specified in an event's configuration definition. All event actions are declared in contract crates and scoped to the declaring crate's hierarchy. Actions defined in the system-level contract crate (e.g., `"sys_stop"`, `"sys_pause"`, `"sys_resume"`) are available at every layer because all partitions depend on that contract crate. Actions defined in a partition's contract crate are available to events within that partition's hierarchy. The event mechanism is uniform; the action vocabulary is contract-crate-scoped. See FPA-029 |
| Fractal partition pattern | The architectural principle that the system is decomposed into layers and partitions, where each partition at every layer applies the same contract/implementation/compositor structure and the same event, configuration, and communication primitives as the system level. Named for the self-similarity of structure at every scale. See FPA-001 |
| Layer             | A level in the system's hierarchical decomposition. Layer 0 is the system level; layer 1 is the partition level. The fractal partition pattern applies at every layer: each uses the same structural primitives (contracts, events, composition) as the layer above it |
| Layer and partition uniformity principle | The defining property of the fractal partition pattern: structural primitives (contracts, events, configuration, composition, specification, and documentation structure) are identical in kind across all layers and partitions. A construct available at layer 0 is available in the same form at layer 1 and beyond |
| Layer-scoped bus  | A bus instance owned by the compositor at a given layer, connecting that layer's partitions. Each compositor owns a separate bus instance; sub-partitions publish only to their layer's bus, not to buses at other layers. Inter-layer communication occurs through compositor relay. See FPA-008 |
| Partition         | A functional subdivision of the system at a given layer. At layer 0, the top-level partitions defined by the domain-specific system specification. At layer 1, sub-components within a partition (e.g., sub-models, sub-services). Each partition is independently replaceable provided it conforms to its layer's interface contracts |
| Relay authority   | The compositor's right to decide whether a message received on its inner bus is forwarded to the outer bus. The compositor may relay as-is, transform, suppress, or aggregate messages before re-emitting them. See FPA-010 |
| State snapshot    | A composition fragment produced by capturing the complete system state at a point in time. A state snapshot is not a distinct system primitive — it is a composition fragment whose fields happen to have been machine-generated rather than hand-authored. Snapshots are loadable, inheritable, and overridable using the same mechanisms as any other composition fragment (see FPA-022) |
| Tick lifecycle    | The three-phase execution model for each tick: Phase 1 (pre-tick processing: direct signals, lifecycle operations, shared context assembly, buffer swap), Phase 2 (partition stepping with intra-tick message isolation), Phase 3 (post-tick processing: event evaluation, output collection, bus request processing, relay). See FPA-014 |

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
threads, and (c) network-based publish-subscribe over a configurable endpoint. The active
mode shall be selectable at runtime via configuration without recompilation. Consistent
with the fractal partition pattern (FPA-001), each compositor at every layer owns a
bus instance for its partitions (see FPA-008). Bus instances at different layers are
independent and may use different transport modes — the layer 0 bus might use network
transport while a layer 1 bus uses in-process transport. The transport independence
guarantee (identical results across modes) applies per bus instance.

**Rationale:** In-process channels minimize latency for single-machine development.
Asynchronous channels support partitions running on separate threads at different update
rates. Network-based transport enables distributed execution across machines and
integration with external tools. No single mode satisfies all deployment contexts.
Layer-scoped bus instances allow transport mode to be selected independently at each
layer, matching the deployment needs of each compositor's partitions without imposing a
system-wide choice.

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
partition pattern (FPA-001), this mechanism shall be identical at every layer.

**Rationale:** Partitions at the same layer frequently need to coordinate around shared
state machines — execution lifecycle, mission phase, mode selections — without coupling
to each other's internals. Placing the type and transition rules in the contract crate
makes the state machine part of the layer's interface contract rather than an
implementation detail of any single partition. Single-owner authority with bus-mediated
requests prevents conflicting mutations and provides a single audit point for
transitions. The fractal partition pattern requires this mechanism to be available at
every layer: the execution state machine (FPA-015) is one instance at layer 0, but
sub-partitions at layer 1 or deeper may define their own shared state machines using
the same pattern.

**Verification Expectations:**
- Pass: All partitions at a given layer read a shared state machine's current value from
  the same contract-crate-defined type; no partition defines its own copy of the state
  enum.
- Pass: A partition requesting a transition emits a typed request on the bus; the owner
  evaluates and applies or rejects it according to the defined transition rules.
- Pass: An invalid transition request is rejected by the owner and logged; the state
  machine value remains unchanged.
- Pass: At layer 1, a sub-partition defines a shared state machine in its layer's
  contract module using the same owner/request/observe pattern used for the execution
  state machine at layer 0.
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
- Pass: A latest-value message type published multiple times within a tick is read by a
  slower consumer as a single value — the most recent — with no error or data loss
  concern.
- Pass: A queued message type published multiple times within a tick is received by the
  consumer as a complete, ordered sequence with no dropped instances.
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
assembly time. Its runtime responsibilities shall include: (a) driving partition
execution by calling lifecycle trait methods (`init`, `step`, `shutdown`) on each
partition in a controlled order; (b) owning the layer's bus instance and publishing
shared context — aggregated state, execution state, environment context — as typed
messages on that bus; (c) receiving and arbitrating requests from partitions on its bus,
acting as the single owner for shared state machines at that layer (FPA-006); (d) relaying
inter-layer requests to the outer bus according to its relay authority (FPA-010);
and (e) catching and handling faults from its partitions (FPA-011).

**Rationale:** The compositor's assembly-time role
(selecting and instantiating partition implementations) is complemented by runtime
communication responsibilities. Making these explicit is necessary because the
compositor sits at the boundary between layers: it is simultaneously the bus owner for
its inner layer (role a-c) and a partition on the outer layer (role d). Without a
defined runtime role, the compositor's responsibilities for relay, fault handling, and
downward data broadcast are ambiguous. The principled split is: trait calls for
imperative lifecycle control (the compositor controls execution order), bus broadcast for
shared context (partitions subscribe to typed messages regardless of source), and bus
requests for upward communication (partitions emit, compositor arbitrates).

**Verification Expectations:**
- Pass: The compositor calls `step()` on each partition in a defined order; partitions
  do not self-schedule.
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

**Statement:** When a sub-partition faults during any trait method call — including
`step()`, `init()`, `shutdown()`, `contribute_state()`, and `load_state()` — by
returning an error, panicking, or timing out, the compositor at that layer shall catch
the fault, log it with the faulting sub-partition's identity and layer depth, and
propagate the error to the outer layer by returning an error from the compositor's own
trait method call. For the purposes of this requirement, a sub-partition is considered
to have *timed out* on a given trait method call if that call does not return within a
per-invocation elapsed-time deadline enforced by the compositor: `step()` and
`contribute_state()` calls shall each have a maximum duration of 50 ms, and `init()`,
`load_state()`, and `shutdown()` calls shall each have a maximum duration of 500 ms.
Implementations may enforce stricter (shorter) per-invocation deadlines than these
maxima but shall not use longer deadlines.
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
but the failure itself shall not be suppressed — it
cascades through the compositor chain until the orchestrator receives it and stops the
run with a clear diagnostic. There shall be no fault-specific bus channel or
message type; the compositor's error return from its own trait call is the propagation
mechanism.

**Rationale:** A sub-partition fault means the system is in an invalid state.
Silently continuing without a failed sub-component produces incorrect results;
silently omitting a partition's state from a snapshot produces a snapshot that loses
data on reload; running after a failed `init()` means the system was never correctly
assembled. The compositor's role in fault handling is to catch raw panics (preventing
undefined behavior from escaping), add diagnostic context (which partition, which layer,
which operation), and propagate a clean error — not to absorb the failure. The outer
layer sees the compositor's error return, not the raw panic, preserving encapsulation of
the internal structure while ensuring the failure is visible. A separate fault channel
or fault message type would create a parallel communication path with its own transport,
relay, and arbitration semantics — complexity that is unnecessary because the
compositor's call-and-return relationship with the outer layer already provides an
error propagation path. Graceful degradation (e.g., falling back to an alternative
partition implementation on fault) is not included — there is no current use case that
requires it. If one arises, a dedicated requirement can be added without weakening the
cascade guarantee defined here.

**Verification Expectations:**
- Pass: A sub-partition returning an error from `step()` is caught by the compositor;
  the compositor returns an error from its own `step()` call, which cascades to the
  orchestrator and stops the run.
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
- Fail: A sub-partition fault is silently absorbed by the compositor and the run
  continues running with missing or degraded partition output.
- Fail: A sub-partition fault propagates as a raw panic or unhandled exception without
  compositor context wrapping.
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
contract crate, independent of the inner system's signals. Any partition within the
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
- Fail: The set of direct signal types is large or changes frequently, indicating misuse
  as a general communication mechanism.

---

## 6. Execution Model

---

### FPA-014 — Compositor Tick Lifecycle

**Statement:** The compositor at each layer shall execute each tick as a
three-phase lifecycle. All transport modes shall enforce this lifecycle identically. A
partition that is itself a compositor shall execute its own complete three-phase tick
lifecycle within the outer compositor's Phase 2 `step()` call for that partition —
the fractal structure nests tick lifecycles recursively.

**Phase 1 — Pre-tick processing (between tick N-1 and tick N):**

1. Check for pending direct signals (FPA-013) and process them.
2. Process pending lifecycle operations (e.g., spawn and despawn requests as defined by
   the domain-specific specification). Spawned entities become active; despawned entities
   are removed and their resources released.
3. Process pending dump and load requests (FPA-023). Dump invokes
   `contribute_state()` on all partitions using post-tick-N-1 state. Load replaces
   partition state via `load_state()`.
4. Assemble shared context from tick N-1 partition outputs and publish it on the bus
   into the **current write buffer**.
5. Publish execution state and shared context on the bus into the **current write
   buffer** — this is the buffer that will become the read buffer for tick N after
   the swap in step 6 and will be visible to all partitions during Phase 2.
   Execution state and shared context are thus stable and readable by all
   partitions throughout Phase 2 of tick N.
6. Swap the read/write buffers: the **new read buffer** (the buffer that was the
   write buffer prior to this step) now contains tick N-1 partition outputs plus the
   shared context and execution state published in steps 4-5; the **new
   write buffer** (the buffer that was the read buffer prior to this step) is cleared
   to receive tick N outputs.

**Phase 2 — Partition stepping:**

The normative execution model is sequential: for each partition in the compositor's step
order (a deterministic ordering defined by the compositor and stable across ticks; the
mechanism by which the compositor determines this order is implementation-defined):

1. The partition reads inter-partition messages from the read buffer (tick N-1 outputs).
   A message published by partition A during tick N-1 is visible; a message published by
   partition A during the current tick N is not visible to any other partition until tick
   N+1.
2. The compositor calls `partition.step(dt)`. The partition writes its outputs to the
   write buffer (tick N outputs).
3. After each partition's `step()` returns, the compositor checks for pending direct
   signals and processes them before stepping the next partition.

A compositor may step partitions concurrently as an optimization (e.g., under async or
network transport where sequential stepping would impose unnecessary serialization
latency). Concurrent stepping is permitted provided the compositor upholds the following
invariants:

- **Intra-tick message isolation:** Each partition reads from the read buffer and writes
  to the write buffer. No partition observes another partition's current-tick output.
  Write paths shall be isolated — concurrent `step()` calls shall not contend on shared
  write buffer state.
- **Direct signals:** The compositor shall check for pending direct signals at least once
  before Phase 3. Under concurrent stepping the worst-case direct signal latency is the
  duration of the longest partition's `step()` call, rather than one partition's step
  duration under sequential stepping.
- **Request collection:** Bus requests (e.g., execution state transition requests)
  emitted by concurrently stepping partitions shall be collected in a thread-safe manner
  for arbitration in Phase 3.
- **Tick barrier:** All partition `step()` calls shall complete before Phase 3 begins.
- **Determinism:** The result shall be identical to that produced by sequential
  stepping in any order — the double-buffered approach guarantees this provided the
  invariants above are upheld.

**Phase 3 — Post-tick processing:**

1. Evaluate all event conditions against the partition state as it existed at the
   beginning of the tick (pre-step state), before any event actions have been applied
   (FPA-024 through FPA-028). Collect the set of events whose conditions are
   satisfied. Apply triggered event actions in configuration declaration order. An
   action's side effects are not visible to other event conditions until the following
   tick.
2. Collect tick N outputs from all partitions.
3. Process bus requests (execution state transition requests, etc.) received during this
   tick. Arbitrate conflicting requests per FPA-018.
4. Relay qualified requests to the outer bus per FPA-010.
5. Check for pending direct signals.

The intra-tick message isolation guarantee — that no partition sees another partition's
current-tick output during Phase 2 — shall hold regardless of step order, thread
scheduling, and transport mode. This eliminates ordering sensitivity: the result is
identical regardless of which partition steps first.

**Rationale:** The specification defines what is communicated between partitions but
prior to this requirement did not fully define when communication becomes visible
relative to the step loop. Under in-process synchronous transport, the compositor's
sequential `step()` calls impose a natural ordering that makes visibility implicit. Under
async and network transport, sequential stepping would impose unnecessary serialization
latency — particularly over a network, where each sequential `step()` call incurs a
round-trip. Without an explicit tick lifecycle, different transport implementations make
different choices about message visibility, producing different results —
violating the transport independence guarantee (FPA-004).

The double-buffered approach (partitions read from tick N-1, write to tick N) is the
simplest model that eliminates ordering sensitivity. No partition sees another's
current-tick output, so the result is the same whether partitions step sequentially,
concurrently, or in any order. This makes the transport independence guarantee trivially
enforceable for intra-tick data flow and also makes concurrent stepping safe as an
optimization — the double-buffer provides isolation without requiring locks on the read
path. The tick barrier (all partitions complete before shared context assembly) ensures
temporal consistency for shared context and state dumps (FPA-023).

The sequential model is normative because it is simplest to reason about and implement
correctly. Concurrent stepping is an allowed optimization because the double-buffered
design already provides the isolation needed — an implementation that upholds the stated
invariants produces identical results regardless of whether partitions step sequentially
or concurrently. This allows compositors to choose the strategy appropriate to their
transport mode: sequential for in-process transport (simple, no threading overhead),
concurrent for network transport (avoids serialized round-trips).

Under sequential stepping, direct signal polling between partition steps (FPA-013)
gives safety-critical signals a worst-case latency of one partition's step duration.
Under concurrent stepping, the worst case is the longest partition's step duration.
Both are bounded and both avoid reentrancy hazards — the compositor checks signals while
no partition state is being mutated.

Snapshot evaluation of event conditions (Phase 3, step 1) eliminates event ordering
sensitivity: reordering event entries in configuration does not change which events fire,
only the order in which their actions are applied. This is consistent with the
double-buffered approach — just as partitions see a snapshot of inter-partition data,
events see a snapshot of partition state.

**Verification Expectations:**
- Pass: Two partitions exchanging data via the bus produce identical results
  regardless of step order, when the step order is varied between test runs.
- Pass: A message published by partition A during tick N is not visible to partition B
  during tick N; partition B reads A's tick N output during tick N+1.
- Pass: Under async transport, the compositor waits for all partition `step()` calls to
  complete before assembling shared context.
- Pass: Under sequential stepping, direct signals are checked at least once between
  each pair of partition `step()` calls; the worst-case signal latency is one partition's
  step duration. Under concurrent stepping, direct signals are checked at least once
  before Phase 3; the worst-case latency is the longest partition's step duration.
- Pass: Event conditions are evaluated against pre-step state; an event action that
  modifies a signal does not cause a second event conditioned on that signal to fire in
  the same tick.
- Pass: Lifecycle operations (spawn, despawn) are processed in Phase 1; all partitions in
  the subsequent tick see the updated entity set.
- Pass: Dump requests during a Running state are processed in Phase 1 at a tick boundary;
  all contributed state corresponds to the same completed tick.
- Pass: The same configuration produces identical results under synchronous, async, and
  network transport modes (within floating-point determinism limits).
- Fail: A partition reads another partition's output from the current tick during
  Phase 2.
- Fail: Event conditions are evaluated after event actions have been applied, causing
  cascading event firing within a single tick.
- Fail: Shared context is assembled before all partitions have completed their current
  tick.

---

### FPA-015 — Execution State Machine

**Statement:** The system shall define a formal execution state machine in the
system-level contract crate with the following states and transitions:

| State     | Valid Transitions                        |
|-----------|------------------------------------------|
| Idle      | -> Running (start)                       |
| Running   | -> Paused (pause), -> Stopped (stop)     |
| Paused    | -> Running (resume), -> Stopped (stop)   |
| Stopped   | -> Idle (reset)                          |

The current execution state shall be available as a named resource or type in the
contract crate accessible to all partitions. No partition shall maintain a private copy
of the execution state that diverges from the authoritative value held by the
orchestrator.

This state machine is one instance of the shared state machine synchronization pattern
(FPA-006) at layer 0. Domain-specific systems may define additional shared state machines
at any layer using the same pattern.

**Rationale:** In a fractally partitioned
system, any partition at any layer may need to observe or influence the execution
lifecycle. Without a formal state machine, partitions cannot consistently reason about
valid transitions, detect invalid requests, or synchronize their behavior with the
global execution lifecycle. An explicit, centrally defined state machine prevents
undefined behavior when multiple sources (UI, events, partition logic) attempt to
influence execution state.

**Verification Expectations:**
- Pass: The contract crate exports an enum or equivalent type representing the four
  execution states (Idle, Running, Paused, Stopped) and a function or method that
  validates whether a requested transition is valid from the current state.
- Pass: Requesting an invalid transition (e.g., Running -> Idle) returns an error or
  rejection rather than silently succeeding.
- Pass: All partitions read execution state from the same contract crate resource; no
  partition defines its own execution state enum.
- Fail: The execution state is represented as a bare boolean or integer flag without
  enforced transition semantics.

---

### FPA-016 — Execution State Transition Requests

**Statement:** Any partition shall be capable of requesting an execution state
transition by emitting a typed execution state request message on its layer's bus. The
request type shall be defined in the system-level contract crate and shall carry the
requested transition and the identity of the requesting partition. At layer 0, the
orchestrator receives requests directly on the layer 0 bus and is the sole authority for
evaluating and applying state transitions according to the state machine defined in
FPA-015. At deeper layers, requests emitted on an inner bus are received by the
compositor at that layer, which relays them to the outer bus according to its relay
authority (see FPA-010). The request propagates through the compositor relay chain
until it reaches the layer 0 orchestrator for arbitration. Requests that represent
invalid transitions shall be logged with the requesting partition's identity and ignored.
Valid transitions shall take effect within one tick of receipt at the
orchestrator. Because requests from deeper layers traverse the compositor relay chain,
a request emitted at layer N may take up to N ticks to reach the orchestrator.
Safety-critical signals that cannot tolerate relay latency shall use the direct signal
mechanism (FPA-013) instead.

**Rationale:** The fractal partition pattern (FPA-001) implies that partitions at
any layer may generate events that affect execution state. Multiple sources may
legitimately need to change execution state: a UI partition for interactive control, a
partition detecting a safety limit exceedance, an event handler reaching a scripted
pause point, or a condition-triggered event firing a stop. Because buses are
layer-scoped (FPA-008), requests from inner layers reach the orchestrator through
compositor relay rather than direct emission on a global bus. Each compositor in the
relay chain may add context or transform the request (FPA-010), ensuring that the
outer layer sees only what the compositor's contract promises. The relay chain introduces
latency proportional to layer depth — each compositor processes its inner bus during its
own `step()` and relays on the next outer-bus read cycle. This latency is acceptable for
normal execution state transitions; safety-critical scenarios that require immediate
response use direct signals (FPA-013), which bypass the relay chain entirely. The
orchestrator remains the single arbitration point for execution state changes.

**Verification Expectations:**
- Pass: A layer 0 partition emitting an execution state stop request during a
  Running state causes the orchestrator to transition to the Stopped state within
  one tick.
- Pass: A layer 0 partition emitting an execution state pause request during a
  Running state causes the orchestrator to transition to the Paused state within
  one tick.
- Pass: A layer 1 sub-partition emitting an execution state stop request on the layer 1
  bus causes the compositor to relay the request to the layer 0 bus, where the
  orchestrator arbitrates and transitions to the Stopped state.
- Pass: An execution state resume request emitted while the system is in the
  Running state is logged as an invalid transition and ignored; the system continues
  running without interruption.
- Pass: Each applied transition is logged with the identity of the partition that
  requested it.
- Fail: A partition directly mutates the execution state resource without emitting a
  request on the bus.
- Fail: A sub-partition at layer 1 or deeper emits an execution state request directly
  on the layer 0 bus, bypassing its compositor's relay authority.
- Fail: Two partitions emitting conflicting requests in the same tick causes undefined
  behavior (see FPA-018 for conflict resolution).

---

### FPA-017 — Execution State Change as Event Action

**Statement:** The system-level contract crate shall declare `"sys_pause"`, `"sys_stop"`,
and `"sys_resume"` (or equivalent action identifiers) as event action identifiers in its
contract-crate action vocabulary. When an event with one of these action identifiers
fires, the event dispatcher shall emit the corresponding execution state request on the
bus at the layer where the event is defined. At layer 0, this reaches the orchestrator
directly. At layer 1 or deeper, the request follows the compositor relay chain
(FPA-010, FPA-016) to reach the orchestrator for arbitration. These actions shall be
usable with both time-triggered and condition-triggered events.

**Rationale:** Execution state transitions are event actions declared in the
system-level contract crate using the same contract-crate action vocabulary mechanism as
any other event action (FPA-029). Because every partition in the system depends on the
system-level contract crate, these actions are available at every layer — the
contract-crate dependency graph makes them visible everywhere. Their handlers emit typed
bus messages (execution state requests) that enter the arbitration pipeline. Encoding
pause points, safety stops, or phase transitions as event actions keeps the logic
declarative and configurable in composition fragments, avoiding hard-coded procedural
checks in partition source code. Connecting the event system to the execution state
request mechanism (FPA-016) ensures all event-driven state changes flow through the same
arbitration path as UI-initiated changes. Because buses are layer-scoped (FPA-008),
event-driven requests at inner layers reach the orchestrator through the same compositor
relay path as all other inter-layer requests.

**Verification Expectations:**
- Pass: A configuration entry for a partition-level event with a time trigger and
  `action = "sys_pause"` (or equivalent) causes the system to pause when the partition's
  clock reaches the specified time.
- Pass: A configuration entry for a partition-level event with a condition trigger and
  `action = "sys_stop"` (or equivalent) causes the system to stop when the monitored
  signal exceeds the threshold.
- Pass: A system-level event entry with `action = "sys_resume"` (or equivalent) and a
  time trigger successfully resumes a paused system at the specified time.
- Pass: The execution state request emitted by an event action is indistinguishable from
  one emitted by a UI partition or by partition code directly; the orchestrator processes
  it identically.
- Fail: Execution state change actions require partition source code modifications
  rather than configuration.

---

### FPA-018 — Execution State Transition Conflict Resolution

**Statement:** When the orchestrator receives multiple execution state request messages
within the same tick that request conflicting transitions, it shall apply the
following deterministic priority order: (1) Stop takes priority over all other requests.
(2) Pause takes priority over Resume. (3) Among requests of equal priority, the request
shall be designated the primary request for logging purposes using a deterministic,
transport-independent rule (the specific tie-breaking mechanism is
implementation-defined). All valid requests of the same type shall be applied (the
outcome is the same regardless of which is designated primary). All received requests
and the resolution outcome shall be logged with the requesting partition identities.

**Rationale:** In a fractally partitioned system, any partition at any layer may
generate events that request execution state changes. Concurrent events may independently
request conflicting transitions in the same tick — for example, one partition requesting
pause while another partition simultaneously requests stop. Without a deterministic
resolution policy, the resulting execution state would depend on message ordering, which
varies with transport mode and scheduling. Prioritizing stop over pause reflects the
principle that safety-critical transitions should not be overridden by less severe
requests. Tie-breaking by a deterministic, transport-independent rule rather than
arrival order ensures that audit logs are reproducible across transport modes and
deployment configurations.

**Verification Expectations:**
- Pass: When one partition emits an execution state stop request and another partition
  emits an execution state pause request in the same tick, the orchestrator transitions
  to Stopped.
- Pass: When one partition emits an execution state resume request and another partition
  emits an execution state pause request in the same tick, the orchestrator transitions
  to Paused (if currently Paused, the pause wins and the state remains Paused).
- Pass: When two partitions emit execution state pause requests in the same tick, the
  same partition is deterministically logged as the primary request regardless of
  transport mode; the outcome (Paused) is the same regardless of which is designated.
- Pass: The orchestrator log for the tick includes both requests and identifies which
  was applied and which was superseded, with partition identities.
- Pass: The audit log for a given configuration is identical across synchronous, async, and
  network transport modes.
- Fail: Conflicting requests in the same tick produce non-deterministic behavior
  depending on transport mode or thread scheduling.
- Fail: A lower-priority request silently overrides a higher-priority request without
  logging.
- Fail: Equal-priority requests from different partitions produce different audit log
  entries depending on transport mode.

---

## 7. Configuration and Composition

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

## 8. State Management

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
- Pass: A state snapshot captured during a Running state produces a valid TOML file
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
current state of all partitions and entities. Dump shall be invocable during any
execution state (Running, Paused, Stopped). Load shall be invocable from the Paused or
Stopped execution states; loading while Running shall not be supported. When the
system is Running, dump shall be processed at a tick boundary (see FPA-014,
Phase 1) so that all partition contributions correspond to the same completed tick.
Both operations shall be requestable via the bus, a UI partition, and the
event system (as event actions), consistent with the uniform request mechanisms used for
execution state transitions (FPA-016, FPA-017).

**Rationale:** Dump and load are the operational interface to state snapshots. Dump
during a Running state enables capturing transient conditions without pausing;
dump while Paused or Stopped enables deliberate checkpointing. Restricting load to
non-Running states prevents mid-tick state corruption. Processing dump at tick boundaries
ensures temporal consistency across transport modes. Making dump and load available
through the same request channels as execution state transitions — bus messages, UI
controls, and event actions — keeps the operational surface uniform. An event action
for state dump with a path parameter allows configuration authors to script automatic
checkpoints at specific times or conditions without UI interaction.

**Verification Expectations:**
- Pass: A dump operation invoked while the system is Running produces a valid
  snapshot fragment without interrupting execution.
- Pass: A dump captured during Running contains state from a single completed tick —
  all partition contributions correspond to the same time.
- Pass: A dump operation invoked while Paused produces a snapshot fragment, and
  loading that fragment in a new run produces identical initial state.
- Pass: A load operation invoked while Paused replaces all entity and partition state
  with the snapshot's state; on resume, the system continues from the loaded state.
- Pass: An event defined with a state dump action and a file path parameter
  triggers at the specified condition and produces a snapshot file at the given path.
- Pass: A load request emitted while the system is Running is rejected (logged and
  ignored), and the system continues unaffected.
- Fail: Dump or load requires a dedicated API distinct from the bus message and event
  action mechanisms used for execution state control.
- Fail: A dump captured during Running contains partition states from different
  ticks.

---

## 9. Events

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
reference logical time as defined by the system clock.

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
partition's state.

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
- Pass: A condition-triggered event with a system stop action conditioned on a signal
  threshold causes the orchestrator to transition to the Stopped state on the first tick
  the condition is met (see FPA-017).
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
- Pass: A partition event armed on an internal signal fires a system stop action that
  the orchestrator applies, halting the run without UI involvement (see FPA-017).
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

Actions declared in the system-level contract crate — such as system stop, pause, and
resume actions (FPA-017) — are available at every layer because every partition
transitively depends on the system-level contract crate. Actions declared in a
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
hardcoded execution-state set — forcing partitions to encode domain logic procedurally
rather than declaratively — or maintain a single global action registry that couples all
partitions to each other's domain vocabulary. Contract-crate scoping avoids both: each
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

## 10. Verification and Testing

---

### FPA-030 — Partition-level Specifications and Documentation Structure

**Statement:** Each partition shall maintain a `docs/` directory whose structure
follows the Diataxis documentation framework and is uniform across all partitions at
all layers:

| Directory             | Diataxis Quadrant | Content                                    |
|-----------------------|-------------------|--------------------------------------------|
| `docs/tutorials/`    | Tutorial          | Learning-oriented guided walkthroughs       |
| `docs/how-to/`       | How-to guide      | Task-oriented procedural instructions       |
| `docs/reference/`    | Reference         | Information-oriented technical descriptions |
| `docs/explanation/`  | Explanation       | Understanding-oriented conceptual discussion|
| `docs/design/`       | --                | `SPECIFICATION.md` and design artifacts     |

The `docs/design/SPECIFICATION.md` shall contain requirements that individually trace
to one or more identifiers in the parent layer's specification. No requirement shall
exist without a parent requirement in the layer above. Where a partition identifies its
own independently replaceable sub-partitions (layer 2), those sub-partitions shall
maintain the same `docs/` structure and their own `SPECIFICATION.md` tracing to the
partition-level specification — perpetuating the fractal partition pattern to arbitrary
depth.

**Rationale:** The fractal partition pattern requires that specification structure and
documentation structure propagate uniformly to every layer of decomposition. A Diataxis-
aligned `docs/` folder ensures that each partition — regardless of its layer — presents
its documentation in the same four quadrants. A contributor navigating from a system-
level partition into its sub-partition finds the same documentation
layout, the same specification format, and the same traceability conventions.
Bidirectional traceability between each layer's specification and the layer above ensures
that all intents are verifiably allocated downward and no requirement is orphaned from
its parent.

**Verification Expectations:**
- Pass: Every partition's `docs/` directory contains the five subdirectories listed
  above (directories may be empty if no content exists yet, but the structure is
  present).
- Pass: Every requirement in each partition's `SPECIFICATION.md` includes a `Traces to:`
  field referencing at least one identifier in the parent layer's specification.
- Pass: Every requirement in this document is referenced by at least one partition-level
  requirement.
- Pass: Where a partition defines independently replaceable sub-partitions, each
  sub-partition maintains its own `docs/` directory with the same Diataxis structure
  and a `SPECIFICATION.md` tracing to the partition-level specification.
- Fail: A partition's `SPECIFICATION.md` contains a requirement with no `Traces to:`
  field.
- Fail: A requirement in any layer's specification exists with no corresponding child
  requirement in any applicable specification at the layer below.
- Fail: A partition's `docs/` structure differs from the Diataxis layout used by its
  parent or sibling partitions.

---

### FPA-031 — Test Coverage of Requirements

**Statement:** Each requirement in every partition SPECIFICATION.md shall be verified by
at least one test in that module's test directory. Test files shall be named using
the requirement identifier they primarily verify (e.g., `tests/fpa_001.rs` in a Rust
realization).

**Rationale:** Named test files create a direct audit trail from requirement to
verification evidence, making it straightforward to confirm coverage during design
reviews and to identify which tests must be updated when a requirement changes.

**Verification Expectations:**
- Pass: For every requirement in a partition specification, a corresponding test file
  exists in that partition's test directory and contains at least one test function.
- Pass: Running the test suite executes all requirement-linked tests and reports
  results.
- Fail: A requirement exists in a partition specification with no corresponding file
  in the test directory.
- Fail: A test file in the test directory exists with no linkage to a named requirement
  (i.e., its filename does not correspond to any requirement identifier).

---

### FPA-032 — Contract Tests at Every Layer

**Statement:** Each independently replaceable partition at every layer shall have
**contract tests**: tests that instantiate the partition implementation in isolation,
invoke it through the contract traits defined at its layer, and verify that outputs
conform to the contract's behavioral requirements. Contract tests shall not require
instantiation of peer partitions at the same layer. Where a partition defines
independently replaceable sub-partitions (layer 1 and beyond), each sub-partition shall
have its own contract tests against the sub-partition contract traits, following the
same structure.

**Rationale:** The fractal partition pattern guarantees independent replaceability at
every layer. That guarantee is only verifiable if each partition can be tested in
isolation against its contract — without its peers. Contract tests make the
replaceability guarantee concrete: if an alternative implementation passes the contract
tests, it is a valid replacement. The same testing structure propagates to every layer
because the same contract structure propagates to every layer. Contract tests use the
contract's own input and output types to supply data, not mocks of peer partitions,
ensuring that tests exercise the actual contract boundary and cannot silently diverge
from the real interface.

**Verification Expectations:**
- Pass: For each partition at layer 0, a test exists that instantiates the partition
  implementation, invokes it through the contract crate's traits, and asserts behavioral
  properties — without any other partition compiled or instantiated.
- Pass: For each independently replaceable sub-partition at layer 1, a test exists that
  instantiates the sub-partition implementation, invokes it through the partition's
  internal contract traits, and asserts behavioral properties — without sibling
  sub-partitions instantiated.
- Pass: When an alternative implementation of a partition is provided, the same contract
  test suite runs against it without modification and reports pass/fail against the
  contract.
- Fail: A contract test for a partition requires instantiation of a peer partition at
  the same layer (indicating the test is not isolated at the contract boundary).
- Fail: A partition at any layer has no tests that exercise its contract traits in
  isolation.

---

### FPA-033 — Compositor Tests at Every Layer

**Statement:** Each layer that composes partitions shall have **compositor tests**:
tests that verify the compositor correctly assembles its partitions and that the
assembled partitions interact correctly through their shared contracts. At layer 0, this
means the orchestrator composes the top-level partitions and they exchange messages
correctly through the bus. At layer 1, this means a partition's internal compositor
assembles its sub-components and they interact correctly through the partition's contract
module. Compositor tests shall assume that lower-layer contract tests pass and shall
focus on composition correctness, not on re-verifying individual partition behavior.

**Rationale:** Contract tests verify individual partitions in isolation. Compositor
tests verify that the compositor's assembly logic — selection, wiring, initialization
ordering — is correct and that the assembled partitions communicate through their
contracts as expected. Without compositor tests, composition bugs (incorrect wiring,
missing initialization, message routing errors) are only caught by expensive system-level
tests where failure localization is difficult. Scoping compositor tests to a single
layer's assembly prevents the common failure mode of integration tests that test
everything at once.

**Verification Expectations:**
- Pass: A layer 0 compositor test composes all top-level partitions using the
  orchestrator, runs at least one tick, and verifies that inter-partition messages are
  exchanged correctly (e.g., one partition produces typed output, another consumes it and
  produces its own typed output).
- Pass: For each partition that decomposes into sub-partitions, a layer 1 compositor
  test composes the sub-partitions using the partition's internal compositor and verifies
  that they interact correctly through the partition's contract module.
- Pass: When a layer 0 compositor test fails but all layer 1 contract tests pass, the
  failure is localizable to inter-partition communication or orchestrator assembly logic.
- Fail: A layer in the system that composes independently replaceable sub-partitions has
  no compositor test.
- Fail: A compositor test re-tests internal behavior of individual partitions rather
  than focusing on their composition and interaction.

---

### FPA-034 — System Tests Trace to Requirements

**Statement:** System-level tests shall exercise the full stack from
configuration to final output. Each system test shall trace to one or more
requirement identifiers from this specification. System tests shall use the same entry
points available to an operator or embedder — layer 0 composition fragments, the
orchestrator's public API, or the command-line interface — and shall not bypass
composition or initialization to reach internal partition interfaces directly.

**Rationale:** System tests verify end-to-end properties that emerge from the
interaction of all layers: an entity reaching an expected final state, an event sequence
producing the expected execution state transitions, output matching a
reference. Requiring traceability to requirements ensures that coverage analysis
is trivial — every requirement with a system test is verified end-to-end, and
requirements without system tests represent visible gaps. System tests complement, but
do not replace, contract and compositor tests at lower layers.

**Verification Expectations:**
- Pass: Each system test file or test function includes a comment or attribute
  identifying the requirement(s) it verifies.
- Pass: Every requirement that specifies observable system behavior has at least
  one system test.
- Pass: System tests use layer 0 composition fragments as input and assert against
  system outputs (final state, recorded output, event logs) — not against internal
  partition state.
- Fail: A system test bypasses the orchestrator or compositor to directly instantiate
  and invoke partition internals.
- Fail: A requirement with observable system behavior has no corresponding
  system test.

---

### FPA-035 — Transport-parameterized Compositor Tests

**Statement:** Layer 0 compositor tests shall be parameterized over transport mode. The
same compositor test configuration shall execute under in-process synchronous, asynchronous
cross-thread, and network-based transport modes, and shall produce identical final
state (within floating-point determinism limits as specified in FPA-004).

**Rationale:** Transport independence (FPA-004) is a correctness constraint that
requires verification across all three transport modes. Making this a parameterized
compositor test — rather than a separate test category — follows from the fractal
partition pattern: transport is a layer 0 composition concern, so transport verification
belongs in layer 0 compositor tests. Layer 1 tests are unaware of transport because
layer 1 partitions are unaware of transport.

**Verification Expectations:**
- Pass: A compositor test runs the same configuration under all three transport modes and
  the final state matches across modes within floating-point determinism limits.
- Pass: The parameterization requires no changes to partition code or partition-level
  test code — only the transport selection in the layer 0 composition fragment
  differs.
- Fail: A transport mode produces a different final state from the other modes
  beyond floating-point determinism limits.
- Fail: Transport-mode testing requires partition-specific test code or partition-aware
  test infrastructure.

---

### FPA-036 — Test Reference Data Ownership at Contract Boundaries

**Statement:** Each contract shall own the reference data used to verify implementations
against it. Reference data shall consist of two elements: (a) **canonical inputs** —
representative instances of the contract's input types, and (b) **expected output
properties** — invariants, tolerances, and constraints that any conforming implementation
must satisfy for those inputs. Contract tests shall assert against the contract's stated
output properties, not against exact output values captured from a specific
implementation. Where a contract defines numerical tolerances, those tolerances shall be
stated in the contract itself and referenced by the contract tests.

**Rationale:** In a fractal partition system, contracts exist at every layer and tests
exist at every layer. If test reference data is maintained independently of the contracts
it verifies, a contract change at layer N can silently invalidate reference data at
layers N through 0 — each layer's compositor and system tests may assert against stale
expected values without any test failing. By making the contract the single owner of its
reference data, the reference data changes when and only when the contract changes, and
both changes are made by the same author in the same artifact. This eliminates cascade
invalidation for contract tests and bounds the propagation of reference data changes to
the contract boundary where they originate.

**Verification Expectations:**
- Pass: Each contract defines canonical inputs as part of its test support module,
  constructed from the contract's own input types.
- Pass: Each contract defines expected output properties (invariants, tolerances,
  constraints) alongside the canonical inputs, not in a separate golden file.
- Pass: Contract tests assert against the contract-defined output properties, not
  against exact values captured from any particular implementation.
- Pass: When a contract's behavioral requirements change, the canonical inputs and
  expected output properties are updated as part of the same change.
- Fail: A contract test compares outputs against a golden file that is maintained
  separately from the contract definition.
- Fail: A contract's expected output properties reference tolerances or constraints
  not stated in the contract itself.

---

### FPA-037 — Compositor Tests Assert Compositional Properties

**Statement:** Compositor tests shall assert **compositional properties** — invariants
that must hold when partitions are correctly assembled — rather than exact output values.
Compositional properties include: messages sent by one partition are received by the
intended consumer; conserved quantities are preserved across partition boundaries within
stated tolerances; execution ordering respects the declared dependency graph; and state
that one partition publishes is visible to its declared consumers in the same tick or the
next tick as specified by the contract. Where a compositor test requires regression
baselines with exact output values, those baselines shall be generated mechanically from
the current contract-conforming implementations, not maintained by hand.

**Rationale:** Compositor tests verify composition correctness — wiring, ordering,
message delivery — not the numerical behavior of individual partitions. Asserting exact
output values in compositor tests couples them to every partition's implementation
details, so that a legitimate improvement in any partition invalidates compositor-level
golden files across the layer boundary. Compositional properties are stable across
implementation changes because they derive from the composition structure, not from any
specific partition's output values. Where exact regression baselines are unavoidable,
mechanical generation from the current implementations makes regeneration a deterministic
operation triggered by any partition change, rather than a manual cross-team coordination
effort.

**Verification Expectations:**
- Pass: Each compositor test asserts at least one compositional property (message
  delivery, conservation, ordering, or visibility).
- Pass: Compositor tests do not fail when a partition's internal implementation is
  replaced with an alternative that passes its contract tests.
- Pass: Where regression baselines with exact values exist, a documented generation
  command can regenerate them from the current implementations without manual editing.
- Fail: A compositor test asserts exact output values that were captured from a specific
  implementation and maintained as a hand-edited golden file.
- Fail: Replacing a partition with a contract-conforming alternative causes compositor
  test failures unrelated to composition correctness.

---

### FPA-038 — System Test Reference Generation

**Statement:** Where system tests require exact end-to-end reference outputs for
requirements traceability, those references shall be generated by a documented,
repeatable process that runs the full stack with known-good implementations and captures
the output. The generation process shall follow the layer structure: when a partition
implementation changes, reference regeneration shall proceed bottom-up — the changed
partition's contract test reference data is verified first, then compositor-level
references are regenerated, then system-level references are regenerated. The
regeneration command, the implementations used, and the version of each contract shall
be recorded alongside the generated reference.

**Rationale:** System tests sometimes require exact reference outputs to verify
requirements traceability (e.g., "a system started with these initial conditions
reaches this final state"). Hand-maintaining these references across a fractal structure
is fragile — a sub-component improvement at layer 2 changes outputs that propagate
through layer 1 compositor assembly to layer 0 system test references. Mechanical
generation with recorded provenance makes regeneration deterministic and auditable.
Bottom-up ordering ensures that no reference is regenerated against a partition that has
not itself been verified, preserving the diagnostic layering of the test pyramid.

**Verification Expectations:**
- Pass: Each system test that asserts exact output values has a corresponding reference
  file generated by a documented command.
- Pass: Each generated reference file records the generation command, the implementation
  versions used, and the contract versions in effect.
- Pass: After a partition implementation change, running the documented regeneration
  command produces updated references that reflect the change, and the system test passes
  against the new references.
- Fail: A system test reference file has no recorded provenance (no generation command
  or version information).
- Fail: A partition implementation change requires manual editing of system test
  reference files rather than regeneration.

---

### FPA-039 — Contract Versioning Scopes Reference Data Propagation

**Statement:** When a contract's behavioral requirements change, the change shall be
expressed as a new contract version. Each contract version shall carry its own canonical
inputs and expected output properties (per FPA-036). Alternative implementations
targeting the previous contract version shall remain testable against that version's
reference data until they migrate to the new version. The contract version boundary
shall be the propagation boundary for reference data changes — implementations targeting
an unchanged contract version shall not require reference data updates.

**Rationale:** The fractal partition pattern supports alternative implementations at
every layer. Without contract versioning, a contract change forces all alternative
implementations to update simultaneously, and their reference data becomes invalid in a
single event. Versioning bounds the propagation: a new contract version creates new
reference data, but implementations targeting the old version continue to use the old
version's reference data until they choose to migrate. This allows alternatives to
migrate on their own schedule while maintaining test coverage throughout the transition.

**Verification Expectations:**
- Pass: Each contract that has undergone a behavioral change maintains version-scoped
  canonical inputs and expected output properties for each supported version.
- Pass: An alternative implementation targeting contract version N passes contract tests
  using version N's reference data, even after version N+1 exists.
- Pass: A contract version change does not cause test failures in implementations
  targeting the previous version.
- Fail: A contract behavioral change invalidates reference data for implementations
  that have not migrated to the new contract version.
- Fail: A contract carries canonical inputs and expected output properties that are not
  scoped to a specific contract version.

---

## 11. Requirements Index

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
| FPA-014 | Compositor Tick Lifecycle                                      |
| FPA-015 | Execution State Machine                                        |
| FPA-016 | Execution State Transition Requests                            |
| FPA-017 | Execution State Change as Event Action                         |
| FPA-018 | Execution State Transition Conflict Resolution                 |
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
| FPA-030 | Partition-level Specifications and Documentation Structure     |
| FPA-031 | Test Coverage of Requirements                                  |
| FPA-032 | Contract Tests at Every Layer                                  |
| FPA-033 | Compositor Tests at Every Layer                                |
| FPA-034 | System Tests Trace to Requirements                             |
| FPA-035 | Transport-parameterized Compositor Tests                       |
| FPA-036 | Test Reference Data Ownership at Contract Boundaries           |
| FPA-037 | Compositor Tests Assert Compositional Properties               |
| FPA-038 | System Test Reference Generation                               |
| FPA-039 | Contract Versioning Scopes Reference Data Propagation          |
