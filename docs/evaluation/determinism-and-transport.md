# Track Q: Determinism & Transport Evaluation

Phase 6 evaluation of transport equivalence, tick-lifecycle determinism,
and cross-strategy composition boundaries.

## 6Q.1 Transport Equivalence

### Non-communicating partitions

Counter partitions (no bus usage) produce identical final state across
all three transport implementations.

| Comparison | Ticks | Partitions | Result |
|---|---|---|---|
| InProcessBus vs AsyncBus | 100 | 3 Counters | Identical (tol=1e-12) |
| InProcessBus vs AsyncBus vs NetworkBus | 50 | 3 Counters | All identical (tol=1e-12) |

### Bus-communicating partitions

Sensor/Follower/Recorder pipeline with DeferredBus, 10 ticks, across
all three transports. NetworkBus uses registered codecs (SensorReading,
TestCommand, SharedContext) for real serialization round-trips.

| Comparison | Ticks | Partitions | Result |
|---|---|---|---|
| InProcess vs Async vs Network (with codecs) | 10 | Sensor+Follower+Recorder | All identical (tol=1e-12) |

This confirms transport transparency holds for bus-communicating partitions
with real message serialization, not just the trivial non-communicating case.

## 6Q.2 Tick-Lifecycle Determinism

### Ordering independence with bus communication (6 permutations)

All 6 permutations of [Sensor, Follower, Recorder] with DeferredBus produce
identical final partition state after 100 ticks. DeferredBus queues messages
published during stepping and flushes after the tick barrier, making stepping
order irrelevant for all inter-partition communication.

**Result:** All 6 orderings produce identical inner state values for all three
partitions (verified via StateContribution envelope unwrapping).

This extends `fpa_014.rs`'s double-buffer isolation proof (SharedContext only)
to include typed bus messages (SensorReading, TestCommand), confirming that
DeferredBus provides complete intra-tick isolation.

### Sequential vs supervisory structural comparison

Lock-step (50 ticks) and supervisory compositors both produce valid state
with the same partition keys (p1, p2). Structural properties verified:

- Both contain entries for all partition IDs
- Lock-step wraps each partition in a StateContribution envelope (state, fresh, age_ms)
- Supervisory wraps each partition in a StateContribution envelope with real freshness data
- All count values are positive integers

Exact values differ because supervisory timing is non-deterministic (task
scheduling determines step count), while lock-step always steps exactly
once per run_tick call. This is expected and by design.

## 6Q.3 Cross-Strategy Composition

### Boundary matrix (4 combinations)

| Outer | Inner | Valid state? | Nesting correct? | Notes |
|---|---|---|---|---|
| Lock-step | Lock-step | Yes | Partitions key present in inner | Standard fractal composition |
| Lock-step | Supervisory | Yes | Inner partition entries with freshness | Inner spawns own tasks |
| Supervisory | Lock-step | Yes | Inner wrapped in freshness envelope | Inner stepped by outer's task |
| Supervisory | Supervisory | Yes | Inner wrapped in freshness envelope | Both spawn independent tasks |

All four combinations produce valid, correctly nested state without any
code modifications. The Partition trait provides the uniform interface that
makes this possible.

### Freshness metadata accuracy

After running a supervisory compositor with 2 Counter partitions:

- `fresh == true` for all running partitions
- `age_ms < 5000` for all entries (typically < 100ms in practice)

Freshness metadata accurately reflects partition liveness under
supervisory coordination.

## Findings and Spec Implications

1. **Transport transparency confirmed under real communication.** All three
   transports produce identical state for both non-communicating and
   bus-communicating partitions. NetworkBus with registered codecs exercises
   real serialization round-trips and still produces equivalent results.
   This validates FPA-004 (transport abstraction).

2. **DeferredBus provides complete ordering independence.** With DeferredBus,
   all 6 permutations of bus-communicating partitions produce identical state.
   This extends FPA-014's intra-tick isolation from SharedContext to all
   inter-partition messages, confirming that stepping order is never observable.

3. **Cross-strategy composition works without modification.** All 4 boundary
   combinations (LS/LS, LS/SV, SV/LS, SV/SV) work through the Partition
   trait alone. No special-case code is needed at strategy boundaries. This
   validates FPA-009's claim that both compositors implement Partition for
   fractal nesting.

4. **Freshness metadata is accurate and timely.** The StateContribution
   envelope provides meaningful freshness information at strategy boundaries,
   enabling consumers to distinguish fresh from stale data without knowing
   the inner compositor's execution strategy.

5. **Determinism is a framework guarantee, not just a partition property.**
   Lock-step + DeferredBus guarantees deterministic outcomes regardless of
   partition implementation, stepping order, or transport. Supervisory
   compositors intentionally trade determinism for autonomy.
