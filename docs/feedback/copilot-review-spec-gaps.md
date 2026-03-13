# Specification Gaps Identified During Copilot Code Review

**Source:** Four rounds of GitHub Copilot automated review on PR #1 (proto_1 branch),
covering the Phase 0–3 prototype implementation.

**Context:** Each issue below was discovered when Copilot flagged an implementation
pattern that was technically correct per the spec's literal text, but produced
incorrect or ambiguous behavior. The prototype fixes are committed; these
recommendations target the specification itself so that future implementations
don't repeat the same ambiguities.

---

## 1. FPA-025: Logical time must be accumulated, not derived

**Spec text (lines 997–1001):**
> At layer 1 (partition level), time-triggered events shall reference logical time
> as defined by the system clock.

**Gap:** The spec does not define how logical time is tracked. The prototype initially
computed `current_time = tick_count as f64 * dt`, which is only correct when `dt` is
constant across all ticks. If `dt` varies between ticks — which nothing in the spec
prohibits, and which multi-rate scheduling (FPA-009) and time-scaling naturally
produce — this formula yields incorrect time that can jump or rewind relative to
actual accumulated simulation time.

**Evidence:** Copilot flagged `compositor.rs:412` where `tick_count * dt` was used
for event evaluation. Fixed by accumulating `elapsed_time += dt` each tick.

**Recommendation:** Add to FPA-025: *"Logical time shall be tracked as the cumulative
sum of dt values passed to the compositor's step invocations, not derived from tick
count multiplied by the current dt. Implementations shall maintain an accumulated
time field that is incremented by dt on each tick."*

---

## 2. FPA-011: Propagated errors must carry layer depth metadata

**Spec text (lines 520–526):**
> ...the compositor at that layer shall catch the fault, log it with the faulting
> sub-partition's identity and layer depth, and propagate the error to the outer layer
> by returning an error from the compositor's own trait method call.

**Gap:** The spec requires *logging* with layer depth but is silent on whether the
*propagated error object* must also carry layer depth. The prototype initially
propagated raw `PartitionError`s from `safe_init`/`safe_step` without attaching
`layer_depth`, even though the error type has a `with_layer_depth()` method. A
caller receiving the error had no way to determine which layer faulted without
parsing log output.

**Evidence:** Copilot flagged `compositor.rs:284` and `compositor.rs:369` where
errors were returned without `.with_layer_depth()`. Fixed by attaching layer depth
on all error propagation paths.

**Recommendation:** Amend FPA-011 line 590–591 verification expectation to read:
*"The error returned by the compositor includes context identifying the faulting
sub-partition's identity, layer depth, and the operation that faulted — both in
logged output and in the error value returned to the outer layer."* The distinction
between "logged" and "returned in the error" matters for callers that need to
programmatically inspect fault origin.

---

## 3. FPA-011: Compositor must fault-wrap ALL partition calls

**Spec text (lines 520–522):**
> When a sub-partition faults during any lifecycle invocation — including `step()`,
> `init()`, `shutdown()`, `contribute_state()`, and `load_state()` (or their equivalents
> under the active invocation mechanism)...

**Gap:** The spec places the requirement on the *invocation* (what happens when a
fault occurs during a listed method) but does not explicitly require that the
compositor *always* uses fault-wrapped calls. The prototype initially called
`partition.contribute_state()` directly in `dump()` and `partition.load_state()`
directly in `load()`, bypassing the `fault::safe_*` wrappers used elsewhere. A
panicking partition would unwind past the compositor despite FPA-011's intent.

**Evidence:** Copilot flagged `compositor.rs:530` (`dump()` calling
`contribute_state()` directly) and `compositor.rs:571` (`load()` calling
`load_state()` directly). Fixed by adding `safe_load_state` wrapper and using
`safe_contribute_state` in `dump()`.

**Recommendation:** Add an explicit obligation statement to FPA-011: *"The compositor
shall invoke all sub-partition lifecycle methods through fault-handling wrappers that
catch panics, enforce per-invocation deadlines, and enrich errors with compositor
context. No compositor code path shall call a sub-partition lifecycle method without
these protections."* This shifts the requirement from reactive ("when a fault occurs")
to proactive ("always use wrappers").

---

## 4. FPA-011 + FPA-006: Fault without fallback must transition to Error state

**Spec text (FPA-011, lines 548–554):**
> (1) propagate the error to the outer layer by returning an error from the
> compositor's own trait method call... If no fallback is configured, the compositor
> shall always propagate the error (option 1).

**Gap:** FPA-011 requires error propagation but is silent on whether the compositor
should transition its own execution state machine to `Error` (defined in FPA-006)
before returning. The prototype initially left the state machine in `Running` on the
no-fallback path, which allowed subsequent `run_tick()` calls after a fault — a
logically inconsistent state. Meanwhile, the fallback-failure path *did* transition
to `Error`, creating an inconsistency within the same method.

**Evidence:** Copilot flagged `compositor.rs:382` where the no-fallback error return
skipped `force_state(ExecutionState::Error)`. Fixed by adding the state transition.
Note the contrast: lines 349, 360, and 365 (fallback failure paths) already called
`force_state(ExecutionState::Error)`.

**Recommendation:** Add to FPA-011: *"When a sub-partition fault is propagated to
the outer layer (option 1), the compositor shall transition its execution state
to Error before returning, preventing further lifecycle invocations in an
inconsistent state. When a fallback is activated (option 2), the compositor remains
in its current execution state."* This makes the asymmetry between propagation and
fallback explicit.

---

## 5. FPA-007: Late-subscriber semantics are undefined

**Spec text (lines 316–318):**
> **Latest-value:** The bus retains only the most recent published value for the
> message type. A consumer that reads slower than the producer publishes will see
> only the most recent value, not intermediate values.

**Gap:** The word "retains" is ambiguous about whether a subscriber created *after*
a message was published can observe the retained value. The spec describes the
behavior of an existing consumer reading slower than the producer, but is silent on
new subscribers. Two interpretations are reasonable:

1. **Global retention:** The bus holds the latest value globally; new subscribers
   see it immediately on first read.
2. **Per-subscriber retention:** Each subscriber only sees messages published after
   its subscription. The bus "retains" in the sense that it doesn't queue
   intermediate values for that subscriber.

The prototype implements interpretation 2 — and the implementation initially had dead
code (`ChannelState.latest`/`queue` fields) that stored channel-level retained values
but never seeded new subscribers from them, producing inconsistent semantics.

**Evidence:** Copilot flagged `in_process.rs` and `network_bus.rs` noting that
channel-level retained state was maintained but never used to initialize late
subscribers. Fixed by removing the dead channel-level fields and clarifying the
`DeliverySemantic` docs to say "after subscription."

**Recommendation:** Clarify FPA-007 latest-value definition: *"A consumer that
subscribes after a value has been published shall not observe that value; retention
applies only to messages published after subscription. The bus does not provide
replay of historical messages to late subscribers."* If the opposite behavior is
intended, state so explicitly and add a verification expectation for it.

---

## 6. FPA-011: Fallback identity invariant is unspecified

**Spec text (lines 551–554):**
> (2) if a fallback implementation is configured for the faulting sub-partition,
> switch to the fallback, log the fault and the fallback activation, and continue
> processing...

**Gap:** The spec does not require that a fallback partition's `id()` match the
primary partition's `id()`. The compositor records partition output keyed by
`partition_id` (captured before stepping), and after fallback activation the
partition slot is replaced. If the fallback has a different `id()`, state is
recorded under the wrong key in the double buffer, and the published
`SharedContext` becomes inconsistent.

**Evidence:** Copilot flagged `compositor.rs:342` where `partition_id` was captured
before stepping and reused after fallback replacement at line 378. Fixed by adding
an `assert_eq!(fallback.id(), partition_id)` in `register_fallback`.

**Recommendation:** Add to FPA-011: *"A fallback implementation configured for a
sub-partition shall have the same partition identity (`id()`) as the primary
partition it replaces. The compositor shall reject registration of a fallback
whose identity does not match the target partition."* This invariant is necessary
because the compositor and outer layer identify partition state by ID.

---

## 7. FPA-026: Equality predicate comparison semantics are unspecified

**Spec text (lines 1023–1028):**
> A condition shall be expressible as a boolean predicate over one or more named
> signals (e.g., `value_a < 100.0`, `value_b > 1.0 && value_c > 500.0`).

**Gap:** The spec defines `<` and `>` predicates by example but does not address
equality comparison (`==`), which is problematic for floating-point signals. The
prototype initially used `(value - threshold).abs() < f64::EPSILON`, which is so
strict (~2.2e-16) that it's effectively exact equality for most values, but
misleadingly suggests tolerance-based comparison. The spec's verification
expectations (lines 1037–1049) only test `<` predicates, leaving equality untested.

**Evidence:** Copilot flagged `event.rs:29` noting the `f64::EPSILON` comparison.
Fixed by using exact `==` equality, which is more honest.

**Recommendation:** Either:
1. Add to FPA-026: *"Equality predicates (`==`) shall use exact floating-point
   comparison. Configuration authors requiring tolerance-based comparison should
   express this using compound predicates (e.g., `value > threshold - epsilon &&
   value < threshold + epsilon`)."*
2. Or, if approximate equality is desired: *"Equality predicates shall use an
   implementation-defined tolerance. The tolerance shall be documented and
   consistent across all transport modes."*

Add a verification expectation: *"Pass: An event conditioned on `value_a == 100.0`
triggers when the signal is exactly 100.0 and does not trigger at 100.0 ±
any non-zero offset."* (Adjust if option 2 is chosen.)
