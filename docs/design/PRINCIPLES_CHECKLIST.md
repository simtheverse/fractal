# FPA Principles Checklist

Use this checklist when evaluating any framework API, implementation change,
or design decision. Each principle is traced to spec requirements with a
concrete violation criterion.

---

## 1. Structural Principles

### 1.1 Fractal uniformity (FPA-001)
**Principle:** The same structural primitives (contracts, compositors, events,
configuration, documentation) are available at every layer without modification.

**Violation:** A construct exists at layer 0 that cannot be used identically
at layer 1 or deeper. A partition at layer 2 cannot use the same event system,
bus pattern, or configuration inheritance as the system level.

### 1.2 Independent replaceability (FPA-002)
**Principle:** Any partition can be replaced by an alternative implementation
conforming to the same contract, without modifying peer partitions or the
compositor.

**Violation:** Replacing an implementation requires changes to another
partition's source code. A partition has a compile-time dependency on a
sibling partition rather than on the shared contract.

### 1.3 Contract-centric coupling (FPA-003)
**Principle:** All inter-partition interfaces are defined in contract crates.
Partitions depend on contracts, never on each other.

**Violation:** A partition imports types from another partition's implementation
crate. Message types are defined outside the contract crate.

### 1.4 Typed messages (FPA-005)
**Principle:** Messages are named, versioned, statically typed, and declare
their delivery semantic in the contract.

**Violation:** A message type lacks a name, version, or delivery semantic.
Message contracts are implicit rather than declared in code.

### 1.5 Contract crate naming (FPA-040)
**Principle:** Contract crates follow `<partition>-contract` naming. Each
maintains a `docs/` directory with Diataxis structure and a SPECIFICATION.md
tracing to parent requirements.

**Violation:** A contract crate uses an ad-hoc name. Requirements don't
trace to the parent spec. The documentation structure is missing.

---

## 2. Communication Principles

### 2.1 Transport abstraction (FPA-004)
**Principle:** The same configuration produces identical results under all three
transport modes (in-process, async, network). Transport selection is a compositor
concern, not a partition concern.

**Violation:** A partition contains transport-specific code or imports. Switching
transport requires source code changes. Results differ across transport modes
beyond floating-point tolerance.

### 2.2 Delivery semantics (FPA-007)
**Principle:** LatestValue retains only the most recent value. Queued retains
all messages in order with no silent drops. Semantics are enforced identically
across all transports.

**Violation:** Queued messages are dropped or reordered. LatestValue returns
stale values after a newer publish. Delivery behavior differs between transports.

### 2.3 Layer-scoped buses (FPA-008)
**Principle:** Each compositor owns an independent bus for its layer. Messages
published on one layer's bus are not visible on another layer's bus.
Inter-layer communication flows through compositor relay.

**Violation:** A partition publishes to or subscribes on a bus at a different
layer. Messages leak between layers without going through the compositor.

### 2.4 Shared state machine synchronization (FPA-006)
**Principle:** Shared state machines have type and transition rules defined
in the contract crate. Exactly one owner holds the authoritative value.
All other partitions observe read-only. Transitions are requested via bus.

**Violation:** A partition directly mutates a shared state machine without
a bus request. The synchronization mechanism differs between layers.

### 2.5 Compositor relay authority (FPA-010)
**Principle:** The compositor has full authority over its layer boundary: relay
as-is, transform, suppress, or aggregate. Encapsulation is preserved — the
outer layer sees only what the compositor's contract promises.

**Violation:** An inner partition's message appears on the outer bus without
passing through the compositor's relay policy. The relay policy is bypassed.

---

## 3. Lifecycle Principles

### 3.1 Compositor runtime authority (FPA-009)
**Principle:** The compositor is active at runtime — it controls when partitions
start, stop, and transition. It publishes SharedContext each tick. Execution
strategy (lock-step, multi-rate, supervisory) is the compositor's choice.

**Violation:** A partition controls its own lifecycle (self-init, self-shutdown).
SharedContext is not published after a tick. The execution strategy leaks into
partition code.

### 3.2 Three-phase tick (FPA-014)
**Principle:** Phase 1: signals, lifecycle ops, dump/load, buffer swap.
Phase 2: step partitions, collect output, publish SharedContext.
Phase 3: evaluate events, process requests, final signal check.

**Violation:** Events are evaluated against post-step state instead of pre-step
snapshot. Dump/load happens during Phase 2. Buffer swap happens after stepping.

### 3.3 Intra-tick isolation (FPA-014)
**Principle:** No partition sees another partition's current-tick output. The
double buffer ensures partition A's tick N output is only visible to partition B
during tick N+1.

**Violation:** A partition reads a peer's current-tick output during stepping.
The write buffer is accessible to readers before the swap.

### 3.4 Deterministic stepping (FPA-014)
**Principle:** Identical results regardless of partition step order within a
tick. Concurrent stepping is allowed as optimization provided invariants hold.

**Violation:** Reordering partitions within a compositor changes the output.
A partition's step depends on the order other partitions were stepped.

---

## 4. State Principles

### 4.1 State as composition fragment (FPA-022)
**Principle:** State snapshots are valid TOML composition fragments with the
same structure, inheritance, and override semantics as configuration.

**Violation:** A state dump produces output that cannot be loaded as a
composition fragment. State format differs from configuration format.

### 4.2 Dump/load preconditions (FPA-023)
**Principle:** Dump works while active or idle. Load requires the compositor
to be idle (Paused or Uninitialized). All contributions in a dump are from
the same completed tick.

**Violation:** Load succeeds while the compositor is Running. A dump includes
contributions from different ticks.

### 4.3 Uniform state contribution (FPA-009)
**Principle:** StateContribution envelope wraps all `contribute_state()` output
with freshness metadata (state, fresh, age_ms). The outer layer sees the same
format regardless of execution strategy.

**Violation:** A compositor produces state without the envelope. The outer layer
must know the inner execution strategy to interpret state.

### 4.4 Recursive state (FPA-012)
**Principle:** Compositors recursively invoke `contribute_state()` on
sub-partitions. The outer layer sees a single contribution without knowledge
of internal decomposition.

**Violation:** Inner partition state is directly visible to the outer layer
without passing through the compositor's `contribute_state()`.

---

## 5. Fault Handling Principles

### 5.1 Fault detection (FPA-011)
**Principle:** The compositor catches panics, errors, and timeouts from all
partition lifecycle calls (init, step, shutdown, contribute_state, load_state).

**Violation:** A partition panic crashes the compositor. An error from step()
is silently ignored. A timeout is not detected.

### 5.2 Fault propagation (FPA-011)
**Principle:** When a sub-partition faults, the compositor propagates the error
to the outer layer. Recovery (fallback, retry) is the responsibility of the
partition itself or the orchestrator, not the compositor.

**Violation:** The compositor silently absorbs a fault. The compositor attempts
recovery logic (e.g., activating a fallback) instead of propagating.

### 5.3 Direct signals (FPA-013)
**Principle:** Safety-critical signals bypass the relay chain within the
declaring contract crate's hierarchy. They cannot be suppressed. Every
emission is logged with identity and layer depth.

**Violation:** A direct signal is suppressed by a relay policy. A signal
propagates beyond the declaring contract crate's boundary. An emission is
not logged.

---

## 6. Event Principles

### 6.1 Uniform event mechanism (FPA-024)
**Principle:** The same event mechanism (trigger types, arming lifecycle,
configuration schema) is used at every layer.

**Violation:** A layer uses a different event format or trigger type than
other layers. System-level events have different schema than partition-level.

### 6.2 Time triggers (FPA-025)
**Principle:** Layer 0 uses wall-clock time. Layer 1+ uses logical time
(cumulative dt sum). Events fire at or after the trigger time.

**Violation:** A layer 1 event uses wall-clock time. An event fires before
its trigger time.

### 6.3 Condition triggers (FPA-026)
**Principle:** Boolean predicates over named signals. Observable signals
include bus values and partition state fields.

**Violation:** A condition references a signal that cannot be observed.
Conditions cascade (one event's action affects another event's condition
in the same evaluation pass).

### 6.4 Snapshot semantics (FPA-026, FPA-014)
**Principle:** All event conditions are evaluated against the pre-step state
snapshot. No cascading — action side effects are not visible to other
conditions in the same pass.

**Violation:** An event condition sees post-step state. One event's action
changes a signal that another event evaluates in the same tick.

### 6.5 Partition-scoped event arming (FPA-027)
**Principle:** Each partition can define and arm events scoped to its own
domain. Partition-scoped events are evaluated against that partition's
internal signals. The definition mechanism is identical to system-level events.

**Violation:** All events must be defined at the system level. Partition
events use a different schema. Arming requires modifying the system contract.

### 6.6 Event definition in configuration (FPA-028)
**Principle:** Events are declaratively defined in TOML. System events in
`[[events]]`, partition events in `[[partition.events]]`. The schema is
identical at both levels.

**Violation:** Events can only be defined programmatically. The schema
differs between layers. Partition events require system-level code changes.

### 6.7 Action scope (FPA-029)
**Principle:** Action vocabulary is scoped to the declaring contract crate's
dependency graph. System-level actions are available everywhere. Partition-level
actions are available only within that partition's hierarchy.

**Violation:** A partition-level action is invoked from a sibling partition's
scope. An action identifier is used without being declared in a visible
contract crate.

---

## 7. Testing Principles

### 7.1 Contract tests (FPA-032)
**Principle:** Every independently replaceable partition is tested in isolation
against its contract. No peer partitions, no mocks. Alternative implementations
pass identical tests.

**Violation:** A contract test requires a peer partition to be present. A test
uses mocks instead of the actual contract types. An alternative implementation
has different tests.

### 7.2 Test coverage (FPA-031)
**Principle:** Each requirement in every partition SPECIFICATION.md has at
least one test. Test files are named after the requirement they verify
(e.g., `tests/fpa_001.rs`).

**Violation:** A requirement has no corresponding test. A test file has
no linkage to a named requirement.

### 7.4 Compositor tests (FPA-033)
**Principle:** Each layer that composes partitions has tests verifying assembly
correctness and inter-partition interaction. Compositor tests assume contract
tests pass. Failure is localizable to composition.

**Violation:** A composing layer has no compositor test. A compositor test
re-verifies individual partition behavior. Failure cannot be localized to
composition vs. partition.

### 7.5 System tests (FPA-034)
**Principle:** System tests exercise the full stack from configuration to
output. Each traces to requirement IDs. Tests use the same entry points
available to operators — never bypass composition.

**Violation:** A system test directly instantiates partitions without going
through composition. A test has no requirement traceability. Internal
partition state is accessed directly.

### 7.6 Transport parameterization (FPA-035)
**Principle:** The same compositor test runs under all three transport modes
and produces identical final state.

**Violation:** A transport mode is excluded from parameterized testing.
Results differ across transports.

### 7.7 Output properties (FPA-036)
**Principle:** Contract tests assert output properties, not exact values.
Canonical inputs and tolerances are defined in the contract, scoped by version.

**Violation:** A test asserts exact floating-point values. Tolerances are
hardcoded in the test rather than defined in the contract.

### 7.8 Compositional properties (FPA-037)
**Principle:** Compositor tests assert compositional properties (delivery,
conservation, ordering) that hold regardless of partition implementation.
Regression baselines are generated mechanically, not hand-maintained.

**Violation:** Compositor tests assert exact output values from a specific
implementation. Replacing a conforming partition breaks compositor tests.

### 7.9 Reference generation (FPA-038)
**Principle:** Reference outputs are generated by a documented, repeatable
command. Provenance (command, versions, contract versions) is recorded.
Regeneration is bottom-up: contract → compositor → system.

**Violation:** Reference files are hand-maintained. Provenance is missing.
Regeneration doesn't follow bottom-up ordering.

### 7.10 Contract versioning (FPA-039)
**Principle:** When a contract's behavioral requirements change, a new
version is declared with its own canonical inputs and output properties.
Implementations targeting a previous version remain testable against that
version's reference data.

**Violation:** A contract change invalidates reference data for previous-
version implementations. Canonical inputs are not scoped to a version.

---

## 8. Configuration Principles

### 8.1 Composition fragments (FPA-019)
**Principle:** TOML fragments select implementations, configure parameters,
and define events at any scope. Uniform structure at every layer.

**Violation:** Configuration format differs between layers. A parameter
is only configurable via code, not TOML.

### 8.2 Fragment inheritance (FPA-020)
**Principle:** `extends` allows fragments to be minimal diffs against a
base. Deep merge: tables merge recursively, scalars and arrays are replaced.
Circular extends is detected as error.

**Violation:** A child fragment must repeat all parent fields to work.
Circular extends is not detected.

### 8.3 Named fragments (FPA-021)
**Principle:** Fragments are resolvable by name from a registry. Overrides
can be applied on top of a named fragment.

**Violation:** A fragment can only be loaded by file path, not by name.

### 8.4 Runtime configurability
**Principle:** Transport mode, partition implementations, event definitions,
and system parameters are determined by the composition fragment at runtime,
not compiled in.

**Violation:** Changing transport requires recompilation. An implementation
choice is hardcoded rather than driven by the fragment.

---

## How to Use This Checklist

Before merging any framework change, review against the reference domain
applications (docs/design/REFERENCE_DOMAINS.md) and check:

1. Does this change violate any principle above?
2. Does it work for the kiosk (event-driven, no dt)?
3. Does it work for the flight sim (multi-rate, nested, network)?
4. Does it work for the document editor (queued ordering, variable rate)?
5. Does it work for the controller (deterministic timing, safety signals)?

If the answer to any question is "no" or "I'm not sure," the change needs
more design work before implementation.
