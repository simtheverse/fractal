# FPA Prototype Research Plan

## Approach

Test-driven development against the spec's Verification Expectations. Each task
is scoped for a single agent. Tasks within a phase that share no dependency can
run in parallel. Dependencies are marked explicitly.

**Language:** Rust (Cargo workspace)
**Test framework:** `#[test]` + `proptest` for invariants
**Config parsing:** `toml` crate
**Async runtime:** `tokio` (for transport modes)
**Test location:** Per-crate `tests/fpa_NNN.rs` (e.g., `crates/fpa-bus/tests/fpa_007.rs`), keeping contract tests compilable against their crate's dependencies only
**Reserved IDs:** FPA-015 through FPA-018 are reserved in the spec and not addressed by this plan

## Testing Discipline (applies to all phases)

These constraints govern how tests are written from Phase 1 onward, not as a
late-stage validation. Violations discovered during implementation are fixed
immediately rather than deferred.

- **Contract tests** assert output properties, not exact values (FPA-036)
- **Canonical inputs** live in the contract crate's test support module (FPA-036)
- **Tolerances** are stated in the contract, not in test code (FPA-036)
- **Contract test suites** are generic over `impl Partition` so alternative
  implementations run without modification (FPA-032)
- **Compositor tests** assert compositional properties (delivery, conservation,
  ordering), not implementation-specific outputs (FPA-037)
- **No peer instantiation** in contract tests (FPA-032)

## Spec Feedback Protocol

This is a prototype validating an architecture spec. When implementation reveals
that a spec requirement is ambiguous, contradictory, or impractical:

1. Record the finding in `docs/feedback/FPA-NNN.md` (requirement ID, issue,
   proposed resolution)
2. Continue implementation using the best available interpretation
3. Address feedback as it accumulates — do not defer to a later phase

Do not defer implementation waiting for spec clarification — the prototype's
purpose is to surface these issues.

**Note:** A systematic feedback review was conducted after Phase 3, addressing
11 feedback items. Spec edits, bus redesign, SharedContext relocation, and
subscriber lifecycle fixes were applied. See Phase 3b below.

---

## Phase 0 — Workspace Scaffold

> No dependencies. Single agent.

- [x] **0.1** Create Cargo workspace root at `/` with `Cargo.toml` (workspace members list)
- [x] **0.2** Create initial crate stubs (empty `lib.rs` files, `Cargo.toml` per crate):
  - `crates/fpa-contract` — contract traits, message types, delivery semantics
  - `crates/fpa-bus` — bus abstraction and in-process transport
  - `crates/fpa-compositor` — compositor assembly and runtime
  - `crates/fpa-events` — event system
  - `crates/fpa-config` — composition fragment parsing
  - `crates/fpa-testkit` — shared test utilities and canonical inputs
- [x] **0.3** Create `docs/design/SPECIFICATION.md` traceability stub for the prototype (traces to FPA-SRS-000)
- [x] **0.4** Create `docs/` Diataxis structure for each crate (directories may be empty):
  - `docs/tutorials/`, `docs/how-to/`, `docs/reference/`, `docs/explanation/`, `docs/design/`
  - `docs/design/SPECIFICATION.md` per crate with `Traces to:` fields referencing FPA-SRS-000
- [x] **0.5** Create `docs/feedback/` directory for spec feedback protocol
- [x] **0.6** Verify `cargo check` passes on empty workspace

---

## Phase 1 — Core Primitives

> Depends on: Phase 0.
> Five parallel tracks. Each track is independent until Phase 2 integrates them.

### Track A: Contract & Partition Traits

**Requirements covered:** FPA-001, FPA-002, FPA-003, FPA-005, FPA-040

- [x] **1A.1** Write failing tests in `crates/fpa-contract/tests/`:
  - `fpa_001.rs`: Uniform contract/implementation/compositor structure exists
  - `fpa_002.rs`: Substitute alternative partition impl, no peer source changes
  - `fpa_003.rs`: Dependency graph shows no direct partition→partition edges
  - `fpa_005.rs`: All inter-partition data is named typed messages from contract crate
  - `fpa_040.rs`: Contract crate naming convention, docs structure present
- [x] **1A.2** Implement in `fpa-contract`:
  - `Partition` trait: `init()`, `step(dt)`, `shutdown()`, `contribute_state()`, `load_state()`
  - Typed message trait/struct with version field
  - Delivery semantic enum (`LatestValue`, `Queued`) declared per message type
  - Canonical input builder in test support module (FPA-036)
- [x] **1A.3** Implement two trivial partitions (e.g., `Counter`, `Accumulator`) in separate modules that depend only on `fpa-contract` — verify FPA-002 by compiling both against same contract tests
- [x] **1A.4** Write contract test harness: a generic test function parameterized over `impl Partition` that asserts output properties (not exact values) — validates FPA-032, FPA-036
- [x] **1A.5** All tests in track A pass; `cargo test -p fpa-contract` green

### Track B: Bus (In-process Transport)

**Requirements covered:** FPA-004 (partial), FPA-007, FPA-008

- [x] **1B.1** Write failing tests in `crates/fpa-bus/tests/`:
  - `fpa_004.rs`: Bus trait abstraction, transport mode selectable without recompilation
  - `fpa_007.rs`: Latest-value retains only most recent; queued retains all in order; delivery semantic declared per message type
  - `fpa_008.rs`: Layer-scoped bus instances are independent; sub-partition publishes only to its layer's bus
- [x] **1B.2** Define `Bus` trait in `fpa-bus`:
  - Object-safe core: `Bus` trait with `publish_erased()`, `subscribe_erased()`, `transport()`, `id()` — supports `dyn Bus` for runtime transport selection
  - Typed extension: `BusExt` trait with `publish<M>()` and `subscribe<M>()` via blanket impl for `T: Bus + ?Sized`
  - `CloneableMessage` trait bridging type erasure and Clone for multi-subscriber delivery
  - `TypedReader<M>` wrapping `Box<dyn ErasedReader>` to restore compile-time types
  - `Transport` enum (`InProcess`, `Async`, `Network`)
- [x] **1B.3** Implement `InProcessBus` — channel-based, supports both delivery semantics
- [x] **1B.4** Test latest-value: publish 3 times, consumer reads once → gets last value
- [x] **1B.5** Test queued: publish 3 times, consumer reads → gets all 3 in order
- [x] **1B.6** Test layer scoping: two bus instances, message on bus A not visible on bus B
- [x] **1B.7** All tests in track B pass; `cargo test -p fpa-bus` green

### Track C: Double-Buffer Tick Lifecycle

**Requirements covered:** FPA-014 (from FPA-CON-000)

- [x] **1C.1** Write failing tests in `crates/fpa-compositor/tests/`:
  - `fpa_014.rs`:
    - Partition A's tick N output not visible to partition B during tick N
    - Partition B reads A's tick N output during tick N+1
    - Step order does not affect final result (run with reversed order, compare)
    - Buffer swap occurs between pre-tick and partition stepping
- [x] **1C.2** Implement `DoubleBuffer<M>` struct:
  - `read_buffer` / `write_buffer` with swap method
  - Type-safe per-partition write slots
  - Read returns previous tick's values
- [x] **1C.3** Implement `TickLifecycle` struct orchestrating the three phases:
  - Phase 1: assemble shared context, swap buffers
  - Phase 2: step partitions (sequential), read from read buffer, write to write buffer
  - Phase 3: evaluate events (stub), collect outputs, process requests
- [ ] **1C.4** Property test: run 100 ticks with random step orders → identical final state
- [x] **1C.5** All tests in track C pass

### Track F-core: Event Engine (Standalone)

**Requirements covered:** FPA-024 (partial), FPA-025, FPA-026, FPA-027, FPA-029

> The event engine's core logic — condition evaluation, trigger types, action
> dispatch, snapshot semantics — is independent of the compositor. Build it as
> a standalone library in `fpa-events` now; integrate into the compositor in
> Phase 2b.

- [x] **1F.1** Write failing tests in `crates/fpa-events/tests/`:
  - `fpa_025.rs`: Time-triggered events fire at specified time
  - `fpa_026.rs`: Condition-triggered events fire when condition met on snapshot state
  - `fpa_027.rs`: Partition-scoped arming — event armed on partition-internal signal, not visible to outer scope
  - `fpa_029.rs`: Event action vocabulary scoped to declaring contract crate's hierarchy; reject action used outside scope
- [x] **1F.2** Implement in `fpa-events`:
  - `EventDefinition` struct: trigger (time or condition), action, parameters
  - `EventEngine` struct: evaluates conditions against a provided state snapshot, collects triggered events, applies actions in config order
  - Snapshot semantics: no cascading — action side effects not visible to other event conditions within same evaluation pass
  - Partition-scoped arming: arm/disarm API on partition-internal signals
- [x] **1F.3** Implement event action vocabulary:
  - Actions declared in contract crate with scope identifier
  - Dispatcher routes actions based on contract crate hierarchy
  - Validation at definition time: reject action if used outside its scope
- [x] **1F.4** Test no-cascade invariant: event action modifies signal → second event conditioned on that signal does NOT fire in same evaluation pass
- [x] **1F.5** All standalone event engine tests pass; `cargo test -p fpa-events` green

### Track H-structure: Documentation Scaffolding

**Requirements covered:** FPA-030, FPA-031 (structural setup only)

> The Diataxis directory structure and `SPECIFICATION.md` scaffolding is created
> in Phase 0 (task 0.4). This track adds the structural validation test that
> enforces it going forward.

- [x] **1H.1** Write structural validation test in `crates/fpa-testkit/tests/`:
  - `fpa_030.rs`: Every crate has `docs/` with 5 Diataxis subdirectories; every requirement in each crate's `SPECIFICATION.md` has `Traces to:` field
  - `fpa_031.rs`: Every requirement ID in a crate's `SPECIFICATION.md` has a corresponding test file in that crate's `tests/` directory
- [x] **1H.2** Implement the validation as a test or build script that walks the workspace
- [x] **1H.3** All structural validation tests pass (they will need to be kept passing as subsequent phases add requirements and tests)

---

## Phase 2 — Compositor (Single Layer)

> Depends on: Phase 1 Tracks A, B, C.
> Two tracks: Compositor core and Config parsing run in parallel.
> Track E's compositor-integration tasks (dump/load) are sequenced after Track D.

### Track D: Compositor Core

**Requirements covered:** FPA-006, FPA-009, FPA-011

- [x] **2D.1** Write failing tests in `crates/fpa-compositor/tests/`:
  - `fpa_006.rs`: Shared state machine — single owner, bus-mediated requests, invalid transition rejected
  - `fpa_009.rs`: Compositor runtime role — lifecycle coordination, bus ownership, shared context publication, request arbitration
  - `fpa_011.rs`: Fault handling — error from `step()` caught and propagated with context; panic caught; fallback activation; timeout enforcement (50ms step, 500ms init)
- [x] **2D.2** Implement `Compositor` struct in `fpa-compositor`:
  - Assembly: accept list of `Box<dyn Partition>` + composition config
  - Lifecycle: `init()` → `run_tick(dt)` → `shutdown()` delegating to partitions
  - Bus owner: creates and holds concrete `InProcessBus` for its layer (Phase 4 task to change to `Box<dyn Bus>` for runtime transport selection)
  - Shared context: `SharedContext` defined in `fpa-contract` (framework type), published on bus each tick via `BusExt`
  - Request arbitration: receives typed requests, applies state machine transition rules
- [x] **2D.3** Implement shared state machine:
  - State enum + transition rules defined in contract crate
  - Single owner (compositor or designated partition)
  - Request/reject cycle via bus
- [x] **2D.4** Implement fault handling:
  - Catch `Result::Err` from partition trait calls
  - Catch panics via `std::panic::catch_unwind`
  - Timeout enforcement via `tokio::time::timeout` or thread deadline
  - Error context: partition identity, layer depth, operation name
  - Fallback activation when configured
- [x] **2D.5** All compositor core tests pass

### Track E-parse: Composition Fragment Parsing

**Requirements covered:** FPA-019, FPA-020, FPA-021

> Config parsing is independent of the compositor. Runs in parallel with Track D.

- [x] **2E.1** Write failing tests in `crates/fpa-config/tests/`:
  - `fpa_019.rs`: TOML fragment selects partition implementations at a given scope
  - `fpa_020.rs`: `extends` inheritance — child overrides parent, deep merge
  - `fpa_021.rs`: Named fragments (presets) referenced by name
- [x] **2E.2** Implement in `fpa-config`:
  - `CompositionFragment` struct parsed from TOML
  - `extends` field resolution: load parent, deep-merge child overrides
  - Named fragment registry (map of name → fragment)
  - Fragment validation: all referenced implementations exist
- [x] **2E.3** All config parsing tests pass; `cargo test -p fpa-config` green

---

## Phase 2b — Integration (Events + Config into Compositor)

> Depends on: Phase 2 Tracks D and E-parse; Phase 1 Track F-core.
> Two parallel tracks.

### Track E-integration: State Dump/Load

**Requirements covered:** FPA-022, FPA-023

- [x] **2b-E.1** Write failing tests in `crates/fpa-compositor/tests/`:
  - `fpa_022.rs`: State snapshot is a valid composition fragment; loadable, inheritable, overridable
  - `fpa_023.rs`: Dump invokes `contribute_state()` on all partitions; load restores via `load_state()`; round-trip identity
- [x] **2b-E.2** Implement dump/load in `fpa-compositor`:
  - `dump()` → calls `contribute_state()` on each partition → assembles TOML
  - `load(fragment)` → decomposes fragment → calls `load_state()` per partition
  - Round-trip test: run N ticks, dump, load into fresh compositor, run M more ticks, compare with continuous run
- [x] **2b-E.3** All dump/load tests pass

### Track F-integration: Event Engine in Compositor

**Requirements covered:** FPA-024 (full), FPA-028

- [x] **2b-F.1** Write failing tests:
  - `fpa_024.rs` (in `fpa-compositor`): Event system architecture — uniform mechanism at every layer, integrated into compositor Phase 3
  - `fpa_028.rs` (in `fpa-config`): Events defined in TOML configuration; same schema at all layers
- [x] **2b-F.2** Integrate event engine into compositor's Phase 3 (post-tick processing):
  - Compositor passes pre-step state snapshot to `EventEngine::evaluate()`
  - Triggered actions applied in config order
  - Event definitions loaded from composition fragments
- [x] **2b-F.3** All event integration tests pass

---

## Phase 3 — Fractal Depth and Transport Independence

> Depends on: Phase 2b.
> Five parallel tracks. Multi-layer, transport modes, and execution strategies
> are independent concerns that all build on the single-layer compositor.

### Track G: Compositor-as-Partition (Vertical Composition)

**Requirements covered:** FPA-001 (fractal depth), FPA-008, FPA-010, FPA-012, FPA-013

- [x] **3G.1** Write failing tests in `crates/fpa-compositor/tests/`:
  - `fpa_010.rs`: Relay authority — compositor relays, transforms, suppresses, aggregates inner requests
  - `fpa_012.rs`: Recursive state contribution — nested TOML fragment, outer layer sees one contribution per partition
  - `fpa_013.rs`: Direct signals — bypass relay chain within contract crate scope; scoped to declaring crate; logged with identity and depth
  - `fpa_008_multilayer.rs`: Layer 1 message not visible on layer 0 bus
- [x] **3G.2** Implement compositor-as-partition:
  - `Compositor` implements `Partition` trait (its `step()` runs a full inner tick lifecycle)
  - Inner bus distinct from outer bus
  - Relay gateway: receive inner requests, apply relay policy (forward/transform/suppress/aggregate), emit on outer bus
- [x] **3G.3** Implement recursive state contribution:
  - Compositor's `contribute_state()` calls `contribute_state()` on each sub-partition
  - Assembles nested TOML fragment
  - `load_state()` decomposes and delegates to sub-partitions
- [x] **3G.4** Implement direct signals:
  - `DirectSignal` type: signal identifier, reason, emitter identity
  - Registration in contract crate (small, stable set)
  - Bypass path: signal reaches declaring crate's orchestrator without relay chain
  - Scope enforcement: does not propagate beyond system boundary when embedded
  - Logging on every emission
- [x] **3G.5** Build a two-layer test scenario:
  - Layer 0: orchestrator with partition A (simple) + partition B (compositor over sub-partitions B1, B2)
  - Verify: replacing B's internal decomposition does not change layer 0 bus messages
  - Verify: B1 emitting a request → B's compositor relays (or suppresses) → orchestrator sees (or doesn't see) it
  - Verify: B1 emitting direct signal → orchestrator receives it without B's relay involvement
- [x] **3G.6** All multi-layer tests pass

### Track I: Async Transport

**Requirements covered:** FPA-004 (async), FPA-035 (parameterized)

- [x] **3I.1** Write failing transport-parameterized tests in `crates/fpa-bus/tests/`:
  - `fpa_035.rs`: Same compositor test config runs under InProcess and Async → identical final state within f64 tolerance
- [x] **3I.2** Implement `AsyncBus` in `fpa-bus`:
  - `tokio::sync::broadcast` or `mpsc` channels
  - Same `Bus` trait as `InProcessBus`
  - Delivery semantics enforced identically
- [x] **3I.3** Run all existing compositor tests and contract tests parameterized over `InProcess` and `Async` — all pass with identical results
- [x] **3I.4** All async transport tests pass

### Track J: Network Transport (Stub)

**Requirements covered:** FPA-004 (network)

- [x] **3J.1** Implement `NetworkBus` stub in `fpa-bus`:
  - TCP-based or gRPC-based pub/sub
  - Same `Bus` trait
  - Serialization via `serde` for message types crossing process boundary
- [x] **3J.2** Run parameterized compositor tests with `Network` transport — identical final state
- [x] **3J.3** Test: layer 0 bus on Network, layer 1 bus on InProcess — both work in same run (FPA-004, FPA-008)
- [x] **3J.4** All network transport tests pass

### Track K: Multi-rate Execution

**Requirements covered:** FPA-009 (multi-rate)

- [x] **3K.1** Write failing tests in `crates/fpa-compositor/tests/`:
  - Multi-rate: fast partition steps 4x per slow partition 1x; results correct
  - Shared context published once per outer tick; fast partitions write to separate write-buffer slots per sub-step
- [x] **3K.2** Implement multi-rate scheduling in compositor:
  - Rate multiplier per partition in composition fragment
  - Compositor sub-steps fast partitions within a single outer tick
- [x] **3K.3** All multi-rate tests pass

### Track L: Supervisory Coordination

**Requirements covered:** FPA-009 (supervisory)

- [x] **3L.1** Write failing tests in `crates/fpa-compositor/tests/`:
  - Partitions run own processing loops (not called by compositor)
  - Compositor manages lifecycle boundaries (start/stop)
  - Compositor detects fault via heartbeat/timeout
  - Data freshness metadata on output (fresh vs stale)
- [x] **3L.2** Implement supervisory compositor variant:
  - Partition spawned as task/thread with own loop
  - Compositor publishes shared context; partitions consume via bus
  - Heartbeat monitoring with configurable timeout
  - Freshness metadata on compositor output to outer bus
- [x] **3L.3** All supervisory tests pass

---

## Phase 3b — Feedback Review & Architecture Refinement

> Depends on: Phase 3.
> Systematic review of 11 feedback items collected during Phases 0–3.

- [x] **3b.1** Review all feedback files in `docs/feedback/` for credibility and validity
- [x] **3b.2** Bus redesign (FPA-004): Refactor `Bus` trait to object-safe core + `BusExt` typed extension
  - `CloneableMessage` trait for type-erased multi-subscriber delivery
  - `TypedReader<M>` wrapping `Box<dyn ErasedReader>` for compile-time type restoration
  - Updated `InProcessBus`, `AsyncBus`, `NetworkBus` to implement object-safe `Bus`
- [x] **3b.3** Subscriber lifecycle fix (FPA-004): Switch to `Weak` subscriber refs with lazy pruning during publish
- [x] **3b.4** SharedContext relocation (FPA-009): Move `SharedContext` from `fpa-compositor` to `fpa-contract` as framework type
- [x] **3b.5** Spec edits: Apply 8 clarifications to `SPECIFICATION.md`:
  - FPA-004 (runtime transport), FPA-006 (domain-specific state vocabulary), FPA-007 (late-subscriber semantics)
  - FPA-009 (partition guarantees, strategy definitions, multi-rate clarification)
  - FPA-011 (fault-wrap obligation, Error state, layer depth, fallback identity)
  - FPA-023 (idle precondition), FPA-025 (cumulative time, time-agnostic engine), FPA-026 (exact float equality)
- [x] **3b.6** Explainer doc: `docs/explanation/bus-performance-and-data-paths.md` covering double-buffer vs bus data paths, type erasure costs, and mitigations
- [x] **3b.7** Update all test files for `BusExt`/`BusReader` imports (8 test files across `fpa-bus` and `fpa-compositor`)
- [x] **3b.8** All tests pass after refactoring; `cargo test` green

---

## Phase 4 — Cross-cutting Integration

> Depends on: Phase 3b.
> Four parallel tracks, with dependencies noted.

### Track M: Cross-strategy Composition

**Requirements covered:** FPA-009 (strategy adapter)

> Depends on: Tracks K and L (needs both multi-rate and supervisory to compose).

- [x] **4M.1** Write failing tests in `crates/fpa-compositor/tests/`:
  - Lock-step outer compositor embeds supervisory inner compositor — works without modification
  - Supervisory outer embeds lock-step inner — works without modification
  - Freshness metadata correctly indicates stale data at strategy boundary
- [x] **4M.2** Implement strategy adapter in compositor:
  - When inner strategy differs from outer, compositor adapts at boundary
  - Present expected interface to outer layer
  - Freshness metadata attached when output is from cache
- [x] **4M.3** All cross-strategy tests pass

### Track H-validation: Documentation Structure Validation

**Requirements covered:** FPA-030, FPA-031 (full validation)

> Depends on: Phase 3 Track G (multi-layer structure must exist to validate
> recursive docs structure). Builds on the structural validation test from
> Phase 1 Track H-structure.

- [x] **4H.1** Extend structural validation tests:
  - Verify bidirectional traceability: every requirement in FPA-SRS-000 is referenced by at least one crate-level requirement
  - Verify recursive structure: sub-partitions (from multi-layer) maintain their own `docs/` and `SPECIFICATION.md`
  - Verify test file naming matches requirement IDs across all crates
  - Check for orphan requirements (no parent trace)
- [x] **4H.2** All structure validation tests pass across the full workspace

### Track M2: Runtime Transport in Compositor

**Requirements covered:** FPA-004 (runtime transport selection)

> Depends on: Phase 3b (bus redesign must be complete).

- [x] **4M2.1** Change `Compositor` and `SupervisoryCompositor` to accept `Box<dyn Bus>` instead of concrete `InProcessBus`
- [x] **4M2.2** Update compositor construction to receive bus via dependency injection
- [x] **4M2.3** Test: same compositor config runs with `InProcessBus`, `AsyncBus`, `NetworkBus` — identical results
- [x] **4M2.4** All runtime transport tests pass

### Track N: Contract Test Reusability & Reference Data

**Requirements covered:** FPA-032, FPA-036, FPA-037, FPA-039

> Can start once Phase 2b is complete (single-layer compositor with tests).
> Does not depend on Phase 3 tracks.

- [x] **4N.1** Write failing tests:
  - `fpa_032.rs`: Same contract test suite runs against alternative impl without modification
  - `fpa_036.rs`: Contract tests assert output properties, not exact values; canonical inputs in contract's test module; tolerances stated in contract
  - `fpa_037.rs`: Compositor tests assert compositional properties (delivery, conservation, ordering); don't fail when partition impl swapped
  - `fpa_039.rs`: Contract version N has own reference data; impl targeting v N unaffected by v N+1
- [x] **4N.2** Implement contract versioning:
  - Version field on contract trait
  - Version-scoped canonical inputs and output properties
  - Alternative impl targets specific version
- [x] **4N.3** Verify: swap partition impl → contract tests still pass without modification
- [x] **4N.4** Verify: swap partition impl → compositor tests still pass (compositional properties stable)
- [x] **4N.5** All reference data tests pass

---

## Phase 5 — System Test Infrastructure

> Depends on: Phase 4 (all tracks).
> Two parallel tracks.

### Track O: System Test Harness

**Requirements covered:** FPA-033, FPA-034, FPA-038

- [ ] **5O.1** Write failing tests:
  - `fpa_033.rs`: Compositor test at each layer that composes partitions exists; failure localizable to composition when contract tests pass
  - `fpa_034.rs`: System tests use public entry points (fragments, API); trace to requirement IDs; don't bypass composition
  - `fpa_038.rs`: System test references generated by documented command; provenance recorded; bottom-up regeneration after partition change
- [ ] **5O.2** Implement system test harness:
  - Entry point: `System::from_fragment("config.toml")` → `.run()` → assert outputs
  - Traceability: test attribute or comment with requirement ID
  - Reference generation command: `cargo run --bin generate-refs` → captures output with provenance metadata
- [ ] **5O.3** Implement bottom-up regeneration:
  - Script that: runs contract tests → regenerates compositor refs → regenerates system refs
  - Each ref file records: generation command, impl versions, contract versions
- [ ] **5O.4** All system test infrastructure tests pass

---

## Phase 6 — Evaluation

> Depends on: Phase 5.
> Three parallel tracks.

### Track P: Replaceability & Isolation Evaluation

- [ ] **6P.1** For each prototype crate, attempt to swap every partition with an alternative impl:
  - Verify: no peer source changes required (FPA-002)
  - Verify: contract tests pass on alternative (FPA-032)
  - Verify: compositor tests pass with alternative (FPA-037)
  - Record: lines of code changed, files touched, compilation errors
- [ ] **6P.2** Measure test isolation:
  - Verify: every contract test runs without instantiating peer partitions
  - Measure: test execution time for contract tests vs compositor tests vs system tests
  - Record: test pyramid shape (count at each tier)
- [ ] **6P.3** Write evaluation findings to `docs/evaluation/replaceability.md`

### Track Q: Determinism & Transport Evaluation

- [ ] **6Q.1** Run full test suite across all 3 transport modes:
  - Compare final state across modes (within f64 tolerance)
  - Record any transport-dependent failures
- [ ] **6Q.2** Run tick-lifecycle determinism tests:
  - 1000 ticks × 10 random step orderings → compare all final states
  - Concurrent vs sequential stepping → compare results
- [ ] **6Q.3** Run cross-strategy composition tests:
  - Lock-step ↔ supervisory boundary combinations
  - Verify data freshness metadata accuracy
- [ ] **6Q.4** Write evaluation findings to `docs/evaluation/determinism-and-transport.md`

### Track R: Ergonomics & Performance Evaluation

- [ ] **6R.1** Measure boilerplate:
  - Lines of code to add a new partition (contract + impl + tests)
  - Lines of code to add a new message type
  - Lines of code to add a new layer of decomposition
- [ ] **6R.2** Measure performance:
  - Tick overhead (empty partitions): time per tick at 10, 100, 1000 partitions
  - Bus throughput: messages/sec for each transport mode
  - Type erasure overhead: Box allocation + clone cost per publish, `dyn Bus` vs concrete bus dispatch
  - Double-buffer swap cost
  - Compositor relay overhead per layer depth
- [ ] **6R.3** Assess fractal uniformity:
  - Are patterns at layer 0 identical to layer 1+?
  - Conceptual footprint: unique concepts a contributor must learn vs system depth
- [ ] **6R.4** Write evaluation findings to `docs/evaluation/ergonomics-and-performance.md`

---

## Phase 7 — Synthesis

> Depends on: Phase 6.

- [ ] **7.1** Compile evaluation findings into `docs/evaluation/SUMMARY.md`:
  - Which FPA claims are validated by the prototype?
  - Which claims need spec revision based on implementation experience?
  - Recommended changes to FPA-SRS-000 or FPA-CON-000
- [ ] **7.2** Final spec feedback pass — most feedback addressed in Phase 3b; review any remaining items from Phases 4–6
- [ ] **7.3** List open questions and areas for further prototyping
- [ ] **7.4** Architecture decision records for any spec changes surfaced during prototyping

---

## Parallelism Map

```
Phase 0  ────────────────────────────────────────────►
                                                      │
Phase 1  ┌─ Track A (Contract) ────────────────────► │
         ├─ Track B (Bus) ─────────────────────────► │
         ├─ Track C (Double-buffer) ───────────────► ├──►
         ├─ Track F-core (Event engine) ───────────► │
         └─ Track H-structure (Docs validation) ───► │
                                                      │
Phase 2  ┌─ Track D (Compositor) ──────────────────► │
         └─ Track E-parse (Config parsing) ────────► ├──►
                                                      │
Phase 2b ┌─ Track E-integration (Dump/load) ───────► │
         └─ Track F-integration (Events in comp.) ─► ├──►
                                                      │
Phase 3  ┌─ Track G (Multi-layer) ─────────────────► │
         ├─ Track I (Async transport) ─────────────► │
         ├─ Track J (Network transport) ───────────► ├──►
         ├─ Track K (Multi-rate) ──────────────────► │
         └─ Track L (Supervisory) ─────────────────► │
                                                      │
Phase 3b ── Feedback review & arch refinement ─────► ├──►
                                                      │
Phase 4  ┌─ Track M (Cross-strategy; needs K+L) ──► │
         ├─ Track M2 (Box<dyn Bus> in compositor) ─► │
         ├─ Track H-validation (Full docs check) ──► ├──►
         └─ Track N (Reference data; needs 2b+) ───► │
                                                      │
Phase 5  ── Track O (System tests) ────────────────► ├──►
                                                      │
Phase 6  ┌─ Track P (Replaceability eval) ─────────► │
         ├─ Track Q (Determinism eval) ────────────► ├──►
         └─ Track R (Ergonomics eval) ─────────────► │
                                                      │
Phase 7  ── Synthesis ─────────────────────────────► ▪
```

**Critical path:** 0 → 1(A,B,C) → 2(D) → 2b → 3(G) → 3b → 4(M) → 5(O) → 6 → 7

**Max concurrent tracks:** 5 (Phase 1), 5 (Phase 3), 4 (Phase 4)

## Requirements Traceability Matrix

| Requirement | Phase | Track | Crate | Test File(s) |
|-------------|-------|-------|-------|--------------|
| FPA-001 | 1, 3 | A, G | `fpa-contract`, `fpa-compositor` | `fpa_001.rs` |
| FPA-002 | 1 | A | `fpa-contract` | `fpa_002.rs` |
| FPA-003 | 1 | A | `fpa-contract` | `fpa_003.rs` |
| FPA-004 | 1, 3 | B, I, J | `fpa-bus` | `fpa_004.rs`, `fpa_035.rs` |
| FPA-005 | 1 | A | `fpa-contract` | `fpa_005.rs` |
| FPA-006 | 2 | D | `fpa-compositor` | `fpa_006.rs` |
| FPA-007 | 1 | B | `fpa-bus` | `fpa_007.rs` |
| FPA-008 | 1, 3 | B, G | `fpa-bus`, `fpa-compositor` | `fpa_008.rs`, `fpa_008_multilayer.rs` |
| FPA-009 | 2, 3, 4 | D, K, L, M | `fpa-compositor` | `fpa_009.rs` |
| FPA-010 | 3 | G | `fpa-compositor` | `fpa_010.rs` |
| FPA-011 | 2 | D | `fpa-compositor` | `fpa_011.rs` |
| FPA-012 | 3 | G | `fpa-compositor` | `fpa_012.rs` |
| FPA-013 | 3 | G | `fpa-compositor` | `fpa_013.rs` |
| FPA-014 | 1 | C | `fpa-compositor` | `fpa_014.rs` |
| FPA-019 | 2 | E-parse | `fpa-config` | `fpa_019.rs` |
| FPA-020 | 2 | E-parse | `fpa-config` | `fpa_020.rs` |
| FPA-021 | 2 | E-parse | `fpa-config` | `fpa_021.rs` |
| FPA-022 | 2b | E-integration | `fpa-compositor` | `fpa_022.rs` |
| FPA-023 | 2b | E-integration | `fpa-compositor` | `fpa_023.rs` |
| FPA-024 | 1, 2b | F-core, F-integration | `fpa-events`, `fpa-compositor` | `fpa_024.rs` |
| FPA-025 | 1 | F-core | `fpa-events` | `fpa_025.rs` |
| FPA-026 | 1 | F-core | `fpa-events` | `fpa_026.rs` |
| FPA-027 | 1 | F-core | `fpa-events` | `fpa_027.rs` |
| FPA-028 | 2b | F-integration | `fpa-config` | `fpa_028.rs` |
| FPA-029 | 1 | F-core | `fpa-events` | `fpa_029.rs` |
| FPA-030 | 1, 4 | H-structure, H-validation | `fpa-testkit` | `fpa_030.rs` |
| FPA-031 | 1, 4 | H-structure, H-validation | `fpa-testkit` | `fpa_031.rs` |
| FPA-032 | 4 | N | `fpa-contract` | `fpa_032.rs` |
| FPA-033 | 5 | O | `fpa-testkit` | `fpa_033.rs` |
| FPA-034 | 5 | O | `fpa-testkit` | `fpa_034.rs` |
| FPA-035 | 3 | I | `fpa-bus` | `fpa_035.rs` |
| FPA-036 | 1, 4 | A, N | `fpa-contract` | `fpa_036.rs` |
| FPA-037 | 4 | N | `fpa-compositor` | `fpa_037.rs` |
| FPA-038 | 5 | O | `fpa-testkit` | `fpa_038.rs` |
| FPA-039 | 4 | N | `fpa-contract` | `fpa_039.rs` |
| FPA-040 | 1 | A | `fpa-contract` | `fpa_040.rs` |
