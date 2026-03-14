# Phase 4 Specification Analysis

**Date:** 2026-03-13
**Scope:** Findings from Phase 4 (Cross-cutting Integration) implementation against FPA-SRS-000.
**Baseline:** 282 tests passing across 6 crates. All Phase 4 tasks complete.

---

## Validated Claims

These spec claims were strongly confirmed by the Phase 4 implementation.

### V1. Cross-strategy composition works without modification (FPA-009)

**Spec claim:** "A partition built with one execution strategy is composable into a
system using a different strategy without modification to either the partition or the
outer system."

**Evidence:** Track M wrote 5 tests nesting lock-step and supervisory compositors in
both directions (including a 3-layer lock-step → supervisory → lock-step test) with
zero changes to compositor source code. The `Partition` trait's strategy-neutral design
(`init/step/shutdown`) is the key enabler — both `Compositor` and
`SupervisoryCompositor` implement it, so they compose naturally.

**Files:** `crates/fpa-compositor/tests/fpa_009_cross_strategy.rs`

### V2. Runtime transport selection is a compositor concern, not a partition concern (FPA-004)

**Spec claim:** "Transport selection is a compositor configuration choice, not a
partition concern — the typed extension pattern preserves compile-time type safety at
the partition API while supporting `dyn Bus` for runtime transport selection."

**Evidence:** Track M2 changed compositor constructors from concrete `InProcessBus` to
`Box<dyn Bus>`. Zero partition code changes were needed. The `BusExt` blanket impl on
`T: Bus + ?Sized` means `dyn Bus` automatically gets typed `publish<M>()` and
`subscribe<M>()` methods. 13 existing test files were updated (only constructor call
sites), all 195 pre-existing tests continued to pass.

**Files:** `crates/fpa-compositor/src/compositor.rs` (lines 69–102),
`crates/fpa-compositor/src/supervisory.rs` (lines 61–102),
`crates/fpa-compositor/tests/fpa_004_runtime_transport.rs`

### V3. Contract test suites are reusable across implementations (FPA-032, FPA-036)

**Spec claim:** "Same contract test suite runs against alternative impl without
modification."

**Evidence:** Track N wrote a generic `contract_test_suite<P: Partition>()` function
and ran it against Counter, Accumulator, and a new Doubler partition. All assertions
are property-based (valid TOML table, non-empty, non-negative fields, round-trip
identity) rather than value-specific. Tests pass for all three implementations without
modification.

**Files:** `crates/fpa-contract/tests/fpa_032.rs`,
`crates/fpa-contract/tests/fpa_036.rs`,
`crates/fpa-contract/src/test_support/mod.rs` (OutputProperties, ContractTolerances)

### V4. Compositional property tests survive implementation swap (FPA-037)

**Spec claim:** "Compositor tests assert compositional properties (delivery,
conservation, ordering); don't fail when partition impl swapped."

**Evidence:** Track N wrote 14 tests using helper functions parameterized on
`Vec<Box<dyn Partition>>`. The same `assert_delivery_property`,
`assert_conservation_property`, `assert_ordering_property`, and
`full_compositional_suite` functions run with all-Counter, all-Accumulator,
all-Doubler, and mixed implementations — all pass.

**Files:** `crates/fpa-compositor/tests/fpa_037.rs`

### V5. Documentation structure is bidirectionally traceable (FPA-030, FPA-031)

**Spec claim:** Every requirement in FPA-SRS-000 should be referenced by at least one
crate-level requirement; no orphan requirements should exist.

**Evidence:** Track H added 6 tests checking bidirectional traceability, orphan
detection, cross-crate test naming, and recursive docs structure. All passed without
needing to fix any documentation gaps, validating the structural discipline maintained
from Phase 0 onward.

**Files:** `crates/fpa-testkit/tests/fpa_030.rs`, `crates/fpa-testkit/tests/fpa_031.rs`

---

## Findings Requiring Spec Attention

Each finding includes the spec text it relates to, what the implementation revealed,
and a recommended resolution.

### F1. Freshness metadata representation is underspecified (FPA-009) — RESOLVED

**Spec text (FPA-009):** "The compositor shall indicate data freshness — whether its
output was computed for the current invocation or is the most recent previously computed
result — as metadata accompanying its output on the outer bus. The freshness
representation is defined in the contract crate alongside the output type."

**What the implementation reveals:** The supervisory compositor wraps each partition's
state with an ad-hoc TOML structure:

```toml
[partition-id]
fresh = true
age_ms = 12
state = { ... actual partition state ... }
```

This structure is baked into `SupervisoryCompositor::contribute_state()` (supervisory.rs
lines 391–412) and `SupervisoryCompositor::run_tick()` (lines 273–317). It is NOT
defined in the contract crate (`fpa-contract`) — it is an implementation detail of
`fpa-compositor`. The lock-step compositor returns a completely different structure from
`contribute_state()`:

```toml
[partitions]
partition-a = { ... }
partition-b = { ... }
[system]
tick_count = 10
elapsed_time = 1.666
```

An outer compositor receiving state from an inner partition has no generic way to detect
whether the data is fresh or stale. It must know which compositor type is inside —
violating the encapsulation the spec promises.

**Recommended resolution:** Define a `FreshnessMetadata` type or schema in the contract
crate. Options:

1. A wrapper type: `pub struct FreshnessWrapped<T> { pub value: T, pub fresh: bool, pub age: Duration }`
2. A convention: all compositor `contribute_state()` output includes optional freshness
   keys at the top level, absent when data is guaranteed fresh (lock-step), present when
   it may be stale (supervisory).
3. A trait method: add `fn is_output_fresh(&self) -> bool` to `Partition` trait (default
   `true`), overridden by supervisory compositors.

Option 2 is the least invasive and consistent with the TOML-everywhere approach.

**Resolution (2026-03-13):** Resolved via `StateContribution` type defined in
`fpa-contract/src/state_contribution.rs`. Both compositor types now wrap each
partition's state in a uniform `StateContribution { state, fresh, age_ms }` envelope.
Lock-step always reports `fresh: true, age_ms: 0`. Supervisory derives from heartbeat.
Two new tests verify the type is importable from the contract crate and that both
strategies produce the same format. See commit `2e67013`.

### F2. `contribute_state()` output format diverges between compositor types (FPA-009, FPA-012, FPA-022) — RESOLVED

**Spec text (FPA-012):** "A partition that is itself a compositor shall implement the
state contribution contract by recursively invoking `contribute_state()` on its
sub-partitions and assembling their contributions into a nested TOML fragment."

**Spec text (FPA-022):** "The resulting snapshot fragment shall be a valid composition
fragment loadable by the same mechanism used for layer 0 and layer 1 fragments."

**What the implementation reveals:** The two compositor types produce structurally
incompatible output from `contribute_state()`:

- **Lock-step** (`compositor.rs:539–562`): Returns `{ partitions: { id: state, ... }, system: { tick_count, elapsed_time } }`
- **Supervisory** (`supervisory.rs:391–412`): Returns `{ id: { state: ..., fresh: bool, age_ms: i64 }, ... }`

The supervisory output wraps each partition's state in freshness metadata and omits the
`partitions`/`system` envelope entirely. This means:
- A `dump()` from a supervisory compositor is not structurally compatible with the
  composition fragment format.
- An outer lock-step compositor dumping state that includes a supervisory inner partition
  will get mixed formats in the same snapshot.
- `load_state()` implementations must understand which format they're receiving.

The cross-strategy tests (Track M) work because they only assert freshness keys exist
in the supervisory output and partition keys exist in the lock-step output — they don't
attempt to load a supervisory dump into a lock-step compositor or vice versa.

**Recommended resolution:** Standardize the `contribute_state()` envelope. Both
compositor types should produce the same top-level structure. The supervisory compositor
should nest freshness metadata within the standard envelope, e.g.:

```toml
[partitions]
[partitions.partition-a]
# Actual state fields here
count = 42
[partitions.partition-a._meta]
fresh = true
age_ms = 12

[system]
tick_count = 10
elapsed_time = 1.666
strategy = "supervisory"
```

This preserves the composition fragment compatibility the spec requires while still
carrying freshness information.

**Resolution (2026-03-13):** Resolved together with F1. The `StateContribution` type
provides the uniform envelope, and both compositor types use it. See F1 resolution
note and commit `2e67013`.

### F3. NetworkBus is a structural stub — serialization gap remains (FPA-004)

**Spec text (FPA-004):** "The system shall support... (c) network-based
publish-subscribe over a configurable endpoint."

**What the implementation reveals:** `NetworkBus` (`fpa-bus/src/network_bus.rs`) clones
messages in-memory like `InProcessBus`. It does not serialize to bytes, does not use
TCP/gRPC, and cannot communicate across process boundaries. Track M2's runtime
transport tests pass with `NetworkBus` because the tests run in the same process — the
"identical final state" verification is vacuously true.

The existing feedback file (`docs/feedback/FPA-004-network.md`) identifies this and
proposes a `NetworkMessage` subtrait. This remains unresolved.

**Impact:** The spec's verification expectation — "the same configuration executes to
completion under all three transport modes with identical final state" — is satisfied
only in a degenerate sense. A production implementation would need the `Message` trait
to either require `Serialize + Deserialize` (breaking change affecting all message
types) or use a subtrait pattern for network-eligible messages.

**Recommended resolution:** Incorporate the feedback file's Option 3 into the spec
text. Add a note to FPA-004 that the network transport mode requires messages to
implement serialization, and that this is expressed through a `NetworkMessage` subtrait
or equivalent mechanism — not by adding serde bounds to the base `Message` trait.

### F4. Contract versioning is convention-based, not type-enforced (FPA-039) — ACCEPTED

**Spec text (implied by FPA-039):** "Contract version N has own reference data; impl
targeting v N unaffected by v N+1."

**What the implementation reveals:** Track N implemented versioning via a
`ContractVersion` struct (`fpa-contract/src/test_support/mod.rs` lines 21–33) used to
scope canonical inputs and tolerances:

```rust
pub struct ContractVersion(pub u32);
impl ContractVersion {
    pub const V1: ContractVersion = ContractVersion(1);
    pub const V2: ContractVersion = ContractVersion(2);
}
```

Tests call `CanonicalInputs::standard_dt_for_version(ContractVersion::V1)` to get
version-scoped inputs. But this is purely a testing convention — the `Partition` trait
has no `contract_version()` method, and there is no type-level enforcement that a V1
implementation uses V1 reference data.

The `Message` trait does have a `VERSION: u32` constant, but this is per-message-type
versioning, not per-contract versioning. The two concepts are related but not connected
in the implementation.

**Impact for the prototype:** Adequate. Convention-based versioning is sufficient to
demonstrate the isolation principle.

**Impact for production:** A production system might need stronger guarantees. If a V1
implementation accidentally uses V2 canonical inputs, the type system won't catch it.

**Recommended resolution:** The spec should clarify that FPA-039 describes a
version-scoping discipline for reference data, not a type-enforced version contract.
Alternatively, if type enforcement is desired, the spec should define how contract
version relates to the `Partition` trait — e.g., a `contract_version()` method or an
associated type.

**Disposition (2026-03-13):** Accepted as a deliberate design choice. Convention-based
versioning is appropriate for the prototype and consistent with the spec's intent. The
`Message` trait already has `VERSION: u32` for runtime-relevant per-message versioning.
Contract-level versioning is a test discipline concern — scoping reference data so that
V1 tests don't see V2 inputs — and convention enforcement is sufficient for this. Adding
a `contract_version()` method to `Partition` would be a breaking change to the core
trait for a guarantee that has no runtime effect. A production system that needs stronger
enforcement can layer it on via a wrapper trait or procedural macro without changing the
core architecture.

### F5. Synchronous shutdown cannot confirm async task completion (FPA-009, FPA-011) — SPEC FEEDBACK

**Spec text (FPA-009):** "The compositor is always the lifecycle authority: even when
partitions self-schedule their processing, the compositor controls... when they must
stop."

**Spec text (FPA-011):** "`init()`, `load_state()`, and `shutdown()` calls shall each
have a maximum duration of 500 ms."

**What the implementation reveals:** The spec conflates two distinct concepts under
"lifecycle authority":

1. **Shutdown authority** — the compositor decides *when* shutdown happens.
2. **Shutdown confirmation** — the compositor knows shutdown *has completed*.

Under lock-step execution, these are the same thing: `shutdown()` returns, therefore it's
done. Under supervisory coordination, they diverge. The synchronous `Partition` trait's
`shutdown()` method can *signal* shutdown but cannot *confirm* that async tasks have
actually stopped — because it cannot `await` their join handles.

This is not an implementation limitation that can be worked around. It is inherent to the
architectural model: a supervisory compositor's partitions run as independent tasks
(or on separate processes/nodes). Synchronous shutdown is always a signal, never a
confirmation, in this execution strategy.

The prototype implements this honestly: `SupervisoryCompositor::shutdown()` (via the
`Partition` trait) sends oneshot shutdown signals to all spawned tasks and returns
immediately. The tasks stop asynchronously. For confirmed shutdown with task join, callers
use the separate `async_shutdown()` method, which is not callable through the `Partition`
trait.

**Implications for the spec:**

- FPA-009's claim that the compositor "controls when [partitions] must stop" is accurate
  for shutdown *authority* but misleading about shutdown *confirmation*. The compositor
  controls *when* the signal is sent, but under supervisory coordination, it cannot
  synchronously confirm that partitions have actually stopped.

- FPA-011's 500ms deadline for `shutdown()` needs to specify which concept it applies to.
  If it applies to the *signal* (sending the shutdown command), 500ms is generous. If it
  applies to *confirmation* (all tasks stopped), it is unenforceable through the
  synchronous trait — the compositor would need to busy-wait or block, coupling the
  shutdown deadline to an unrelated timeout parameter.

**Recommended spec changes:**

1. FPA-009 should distinguish between shutdown *signaling* (always available through the
   synchronous trait) and shutdown *confirmation* (requires an async or polling mechanism).
   The Partition trait's synchronous `shutdown()` is a signal. Confirmed shutdown requires
   a separate mechanism outside the core trait.

2. FPA-011 should clarify that the 500ms deadline applies to the `shutdown()` *call*
   returning, not to the guarantee that all work has ceased. Under supervisory
   coordination, work may continue briefly after `shutdown()` returns.

3. The execution strategy explainer should document this split explicitly: under
   supervisory coordination, shutdown confirmation is asynchronous, and the compositor
   detects actual termination through the same mechanisms it uses for fault detection
   (heartbeat expiry, connection state, health messages).

### F6. Compositional property tests are structural, not behavioral (FPA-037) — ACCEPTED

**Spec text (FPA-037):** "Compositor tests assert compositional properties (delivery,
conservation, ordering); don't fail when partition impl swapped."

**What the implementation reveals:** Track N's `fpa_037.rs` tests verify:
- **Delivery:** All partition IDs appear in the write buffer after N ticks.
- **Conservation:** Partition count is stable across ticks.
- **Ordering:** Tick count is monotonic; partition insertion order is preserved.
- **Dump/load roundtrip:** Structural equality after dump → load → dump.

These are useful structural invariants, but they don't test behavioral equivalence.
For example, the tests don't verify that a system-level aggregate (e.g., total energy,
accumulated dt) remains within meaningful bounds when Counter is swapped for Doubler.
The current tests would pass even if a partition returned garbage data — as long as it
returned a non-empty TOML table.

**Impact:** For the prototype, structural properties are the right level. For a
production spec, the testing discipline should distinguish between structural
properties (which the framework guarantees) and behavioral properties (which contracts
define and domain-specific tests verify).

**Recommended resolution:** The spec should clarify the taxonomy: FPA-037 governs
structural/compositional properties that the framework guarantees regardless of
implementation. Behavioral properties (e.g., output within physical bounds) belong to
domain-specific contract tests (FPA-032, FPA-036) and are outside FPA-037's scope.
This is arguably already implied, but making it explicit would prevent confusion about
what "compositional properties" means.

**Disposition (2026-03-13):** Accepted as working-as-intended. The spec already draws
this line implicitly: FPA-037 tests the compositor's framework guarantees (delivery,
conservation, ordering), while FPA-032/FPA-036 tests verify implementation-specific
behavioral properties through contract tests. The compositor doesn't know or care what
a partition computes — only that it participates correctly in the lifecycle. Structural
properties are the right level for compositor tests; behavioral properties belong to
contract tests. No spec change needed.

---

## Minor Observations

### M1. Per-crate Diataxis structure incomplete (FPA-030) — DEFERRED

Per-crate `docs/` directories use flat `docs/SPECIFICATION.md` rather than
`docs/design/SPECIFICATION.md`. The `tutorials/`, `how-to/`, `reference/`
subdirectories are empty. Existing feedback file `docs/feedback/FPA-030.md` covers
this. The fpa_030 tests accommodate the flat structure and report subdirectory gaps
without failing.

**Disposition (2026-03-13):** Deferred to Phase 7 (Documentation & Packaging).

### M2. `Box<dyn Bus>` is not `Send` (FPA-004) — ACCEPTED

The `Bus` trait does not require `Send`. This means `Box<dyn Bus>` is not `Send`, and
compositors holding one are not `Send`. The prototype runs on a single-threaded tokio
runtime so this doesn't matter, but a production multi-threaded deployment would need
`Bus: Send + Sync` or thread-local bus handles.

**Disposition (2026-03-13):** Accepted. The prototype's single-threaded runtime makes
this a non-issue. A production deployment would add `Send + Sync` bounds to the `Bus`
trait, which is a straightforward change.

### M3. Phase 1 task 1C.4 remains skipped — ACCEPTED

The property test "run 100 ticks with random step orders → identical final state" was
never implemented. Phase 4's cross-strategy and multi-transport work makes determinism
testing more relevant, not less. This gap should be addressed in Phase 6 (Track Q:
Determinism Evaluation).

**Disposition (2026-03-13):** Accepted. Already tracked for Phase 6 (Track Q:
Determinism Evaluation).

### M4. `collect_inner_signals` only handles `Compositor`, not `SupervisoryCompositor` — RESOLVED

In `compositor.rs` lines 507–516, `collect_inner_signals()` downcasts inner partitions
to `Compositor` to drain direct signals. It does not attempt to downcast to
`SupervisoryCompositor`. If a supervisory compositor emits direct signals, they would
not propagate to the outer layer. This is a minor gap since direct signals are
safety-critical and the supervisory compositor doesn't currently support them, but the
spec's FPA-013 doesn't restrict direct signals to lock-step compositors.

**Resolution (2026-03-13):** Resolved. Added `emitted_signals: Arc<Mutex<Vec<DirectSignal>>>`
to `SupervisoryCompositor`, shared with spawned partition tasks. After each step, tasks
collect direct signals from inner compositors (via `as_any_mut` downcast to `Compositor`)
and write them to the shared signal store. Added `drain_emitted_signals()` method and
`as_any_mut()` override. Updated `collect_inner_signals()` in `Compositor` to also
downcast to `SupervisoryCompositor`.
