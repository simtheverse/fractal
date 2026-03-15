# Fractal Partition Architecture — Conventions
## Design Choices That Complement the Core Architecture

---

| Field         | Value                                      |
|---------------|--------------------------------------------|
| Document ID   | FPA-CON-000                                |
| Version       | 0.1.0 (draft)                              |
| Status        | Draft                                      |
| Parent        | FPA-SRS-000 (Fractal Partition Architecture)|

---

## Table of Contents

1. Purpose and Scope
2. Tick Lifecycle
3. Verification and Testing Discipline
4. Requirements Index

---

## 1. Purpose and Scope

This document defines conventions that complement the core FPA architecture defined in
FPA-SRS-000. These are valuable design choices that leverage the pattern's structural
primitives to produce additional properties — deterministic reproducibility, auditable
verification, structured testing — but they are not inherent to the pattern. An
FPA-conforming system could adopt different conventions in these areas while satisfying
all core requirements in FPA-SRS-000.

The companion explanation documents provide conceptual discussion of the conventions
defined here:

- [Tick Lifecycle and Synchronization](../explanation/tick-lifecycle-and-synchronization.md)
- [Testing in the Fractal Partition Pattern](../explanation/testing-in-the-fractal-partition-pattern.md)
- [Test Reference Data in the Fractal Partition Pattern](../explanation/test-reference-data-in-the-fractal-partition-pattern.md)
- [Conventions in the Fractal Partition Pattern](../explanation/conventions-in-the-fractal-partition-pattern.md)

The conventions are grouped into two areas:

- **Tick lifecycle** (FPA-014): A double-buffered execution model that produces
  deterministic, ordering-insensitive results. This is one of several valid execution
  strategies; the core architecture does not mandate tick-based execution. See the
  companion explanation:
  [Tick Lifecycle and Synchronization](../explanation/tick-lifecycle-and-synchronization.md).

- **Verification and testing discipline** (FPA-030 through FPA-039): A testing
  methodology and traceability discipline that leverages the partition structure. The
  *possibility* of contract testing is emergent from the architecture (structural
  testability); the specific methodology is a convention. See the companion explanations:
  [Testing in the Fractal Partition Pattern](../explanation/testing-in-the-fractal-partition-pattern.md)
  and [Conventions in the Fractal Partition Pattern](../explanation/conventions-in-the-fractal-partition-pattern.md).

---

## 2. Tick Lifecycle

The core architecture (FPA-SRS-000) permits any execution strategy — lock-step ticks,
multi-rate execution, or fully asynchronous processing with partitions on separate
processes, cores, or compute nodes — provided the chosen strategy satisfies the core
communication, fault handling, and transport requirements. The tick lifecycle convention
defined below is a specific strategy that produces deterministic reproducibility and
ordering-insensitive results. Systems that require synchronized, deterministic execution
(e.g., flight simulation, physics modeling) adopt this convention. Systems that prioritize
throughput, latency, or event-driven responsiveness may use a different execution strategy
while remaining FPA-conforming.

---

### FPA-014 — Compositor Tick Lifecycle

**Statement:** When a system adopts the tick lifecycle convention, the compositor at each
layer shall execute each processing cycle as a three-phase lifecycle. All transport modes
shall enforce this lifecycle identically. A partition that is itself a compositor shall
execute its own complete three-phase tick lifecycle within the outer compositor's Phase 2
`step()` call for that partition — the fractal structure nests tick lifecycles
recursively.

**Phase 1 — Pre-tick processing (between tick N-1 and tick N):**

1. Check for pending direct signals (FPA-013) and process them.
2. Process pending lifecycle operations (e.g., spawn and despawn requests as defined by
   the domain-specific specification). Spawned entities become active; despawned entities
   are removed and their resources released.
3. Process pending dump and load requests (FPA-023). Dump invokes
   `contribute_state()` on all partitions using post-tick-N-1 state. Load replaces
   partition state via `load_state()`.
4. Swap the read/write buffers: the **new read buffer** (the buffer that was the
   write buffer prior to this step) now contains tick N-1 partition outputs; the **new
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
- **Tick barrier:** All partition `step()` calls shall complete before shared context
  assembly or Phase 3 begins.
- **Determinism:** The result shall be identical to that produced by sequential
  stepping in any order — the double-buffered approach guarantees this provided the
  invariants above are upheld.

After all partition `step()` calls complete (the tick barrier), the compositor assembles
shared context from the current tick's partition outputs (the write buffer) and publishes
it on the bus along with the current execution state. Shared context thus reflects the
complete, consistent state of all partitions after tick N — no partition's output is
missing or partial. Partitions do not read shared context during Phase 2; it is available
on the bus for external consumers and for the compositor's own Phase 3 event evaluation.

The intra-tick isolation guarantee applies uniformly to both inter-partition communication
channels. State observation via shared context is isolated by the double buffer:
partitions read from the read buffer containing tick N-1 outputs. Bus messages published
by partitions during Phase 2 shall not be visible to other partitions until after all
Phase 2 `step()` calls have completed. The compositor shall ensure that any bus messages
published during Phase 2 are held until after the tick barrier, then made available for
consumption starting in tick N+1. Both channels thus exhibit one-tick-delay semantics:
output produced by partition A during tick N is available to other partitions during tick
N+1, never during tick N.

**Phase 3 — Post-tick processing:**

1. Evaluate all event conditions against the partition state as it existed at the
   beginning of the tick (pre-step state), before any event actions have been applied
   (FPA-024 through FPA-028). Collect the set of events whose conditions are
   satisfied. Apply triggered event actions in configuration declaration order. An
   action's side effects are not visible to other event conditions until the following
   tick.
2. Collect tick N outputs from all partitions.
3. Process bus requests (execution state transition requests, etc.) received during this
   tick. Arbitrate conflicting requests using a deterministic, transport-independent
   priority rule defined by the domain-specific specification.
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

Without explicit isolation of bus messages during Phase 2, messages published by a
partition stepped early in the sequence would be immediately visible to partitions
stepped later, creating a stepping-order dependence. The result would differ depending on
which partition steps first, violating the ordering-insensitivity guarantee. Ensuring bus
messages are held until after the tick barrier gives them the same one-tick-delay
semantics as the double-buffered shared context, making both communication channels
uniformly ordering-insensitive.

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

Systems that do not require deterministic reproducibility — or that operate in fully
asynchronous, event-driven, or multi-rate modes — may use different execution strategies
while remaining FPA-conforming. The core architecture requires only that the compositor
coordinates partition lifecycle, owns the bus, arbitrates requests, and handles faults
(FPA-009, FPA-011). How it schedules partition execution is an implementation choice.

**Verification Expectations (applicable when the tick lifecycle convention is adopted):**
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
- Pass: A bus message published by partition A during tick N's Phase 2 is not readable
  by partition B during the same Phase 2, regardless of stepping order.
- Pass: Under any stepping order, the result of a multi-partition tick is identical —
  bus message isolation eliminates ordering sensitivity for both communication channels.
- Fail: Shared context is assembled before all partitions have completed their current
  tick.

---

## 3. Verification and Testing Discipline

---

### FPA-030 — Partition-level Specifications and Documentation Structure

**Statement:** Each partition and each contract crate shall maintain a `docs/` directory
whose structure follows the Diataxis documentation framework and is uniform across all
partitions and contract crates at all layers:

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
aligned `docs/` folder ensures that each partition and contract crate — regardless of
its layer — presents its documentation in the same four quadrants. A contributor
navigating from a system-level partition into its sub-partition, or from a partition
into its contract crate, finds the same documentation layout, the same specification
format, and the same traceability conventions.
Bidirectional traceability between each layer's specification and the layer above ensures
that all intents are verifiably allocated downward and no requirement is orphaned from
its parent.

**Verification Expectations:**
- Pass: Every partition's and contract crate's `docs/` directory contains the five
  subdirectories listed above (directories may be empty if no content exists yet, but
  the structure is present).
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
points available to an operator or embedder — the standard composition function
(FPA-015), the orchestrator's public API, or the command-line interface — and shall not
bypass composition or initialization to reach internal partition interfaces directly.

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
Compositional properties include: every message delivered at the end of tick N is
available for consumption by the intended consumer during tick N+1; conserved quantities
are preserved across partition boundaries within stated tolerances; execution ordering respects the declared dependency graph; and state
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
specific partition's output values.

Message conservation is defined per-tick rather than over the lifetime of a run. Under
one-tick-delay semantics (FPA-014), messages produced during the last tick of a run are
delivered but never consumed because no subsequent tick executes. This is an inherent
property of the one-tick-delay model, not a conservation violation. Per-tick
conservation — "every message delivered at end of tick N is available in tick N+1" —
cannot be violated at the last tick because no tick N+1 exists in which the invariant
could fail. Where exact regression baselines are unavoidable,
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
- Pass: In a multi-tick run, every bus message delivered at the end of tick N is readable
  by its intended consumer during tick N+1.
- Pass: A run's last tick producing messages that are never consumed is not treated as a
  conservation violation.
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

## 4. Requirements Index

| ID      | Title                                                          |
|---------|----------------------------------------------------------------|
| FPA-014 | Compositor Tick Lifecycle                                      |
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
