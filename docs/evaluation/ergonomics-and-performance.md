# Track R: Ergonomics & Performance Evaluation

Phase 6 evaluation of the FPA prototype's developer experience and runtime
performance characteristics.

## 6R.1 — Boilerplate Measurement

| Task | LOC (approx) | Files touched |
|------|-------------|---------------|
| New partition (minimal, no bus) | ~70 | 1 (partition impl) |
| New partition (bus-aware) | ~164 | 1 (partition impl) + message type |
| New message type | ~15 | 1 (messages.rs) |
| New layer (compositor-as-partition) | ~30 | 0 (inline setup) |

Boilerplate is dominated by the `Partition` trait's five required methods
(`id`, `init`, `step`, `shutdown`, `contribute_state`) plus the optional
`load_state`. Bus-aware partitions add subscription setup but no new concepts.

## 6R.2 — Performance Benchmarks

Run with `cargo bench -p fpa-testkit`:

### Tick overhead (bench_tick)
- Measures compositor `run_tick` cost scaling with partition count (10, 100, 1000)
- Measures relay depth cost for nested compositors (depth 1-3)

### Bus throughput (bench_bus)
- Publish+read cycle for InProcessBus, AsyncBus, NetworkBus
- Type erasure overhead: `dyn Bus` vs concrete `InProcessBus`

### Buffer swap (bench_buffer)
- DoubleBuffer swap + refill cost at 10, 100, 1000 partitions

## 6R.3 — Fractal Uniformity

### API identity across layers
The same `Partition` trait methods (`init`, `step`/`run_tick`, `contribute_state`,
`shutdown`) work identically on a flat layer-0 compositor and a nested layer-1
compositor-as-partition. No layer-specific APIs exist.

### Conceptual footprint
10 unique concepts: Partition, Message, Bus, Compositor, StateContribution,
SharedContext, DoubleBuffer, DeliverySemantic, ExecutionState, CompositionFragment.

This count does not grow with layer depth. The same 10 concepts apply at every
layer of nesting.

## Findings

1. **Boilerplate is reasonable** — ~70 LOC for a minimal partition is competitive
   with trait-based frameworks. The `contribute_state`/`load_state` pair adds
   serialization boilerplate that could be reduced with a derive macro.

2. **Message types are lightweight** — ~15 LOC per message type (struct + Message impl).
   A derive macro could reduce this to ~5 LOC.

3. **Fractal nesting adds zero new concepts** — The compositor-as-partition pattern
   requires no additional API surface. Layer depth is metadata, not a structural change.

4. **Performance baselines established** — Benchmark suite provides regression
   detection for tick overhead, bus throughput, and buffer swap cost.

## Recommendations

- Consider a `#[derive(Partition)]` macro for the common case (state struct with
  known serialization format) to reduce boilerplate from ~70 to ~20 LOC.
- Consider a `#[derive(Message)]` macro to reduce message type boilerplate.
- Monitor benchmark regressions as the framework evolves.
