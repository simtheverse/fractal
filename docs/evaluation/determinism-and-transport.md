# Track Q: Determinism & Transport Evaluation

Phase 6 evaluation of transport equivalence, tick-lifecycle determinism,
and cross-strategy composition boundaries.

## 6Q.1 Transport Equivalence

Non-bus-communicating Counter partitions produce identical final state across
all three transport implementations.

| Comparison | Ticks | Partitions | Result |
|---|---|---|---|
| InProcessBus vs AsyncBus | 100 | 3 Counters | Identical (tol=1e-12) |
| InProcessBus vs AsyncBus vs NetworkBus | 50 | 3 Counters | All identical (tol=1e-12) |

NetworkBus without registered codecs falls back to clone-based delivery,
producing the same results as InProcessBus and AsyncBus for partitions
that do not communicate over the bus.

## 6Q.2 Tick-Lifecycle Determinism

### Ordering independence (1000 ticks, 10 orderings)

All 6 permutations of 3 Counter partitions (plus 4 repeats = 10 orderings)
produce identical final state after 1000 ticks at dt=1.0. Counter partitions
are order-independent since each steps its own count without reading bus
messages.

**Result:** All 10 orderings produce byte-identical TOML state dumps.

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

1. **Transport transparency confirmed.** For non-bus-communicating partitions,
   all three transports are perfectly equivalent. This validates FPA-004
   (transport abstraction) -- partitions genuinely never know which transport
   is in use.

2. **Determinism is partition-property, not framework-property.** Lock-step
   compositors guarantee deterministic tick ordering, but whether the final
   state is ordering-independent depends on the partition implementations.
   Counter partitions are trivially order-independent. Bus-communicating
   partitions may not be, depending on message consumption patterns.

3. **Cross-strategy composition works without modification.** All 4 boundary
   combinations (LS/LS, LS/SV, SV/LS, SV/SV) work through the Partition
   trait alone. No special-case code is needed at strategy boundaries. This
   validates FPA-009's claim that both compositors implement Partition for
   fractal nesting.

4. **Freshness metadata is accurate and timely.** The StateContribution
   envelope provides meaningful freshness information at strategy boundaries,
   enabling consumers to distinguish fresh from stale data without knowing
   the inner compositor's execution strategy.

5. **NetworkBus clone fallback.** Without codec registration, NetworkBus
   uses clone-based delivery identical to InProcessBus. This is correct
   behavior for in-process use but means transport parameterization only
   exercises serialization when codecs are registered. Tests that validate
   serialization fidelity should use registered codecs (as fpa_035_transport
   does).
