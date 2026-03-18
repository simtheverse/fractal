# Track R: Ergonomics & Performance Evaluation

Phase 6 evaluation of the FPA prototype's developer experience and runtime
performance characteristics.

## 6R.1 — Boilerplate Measurement

All LOC values measured from source at test time.

| Task | LOC (measured) | Files touched |
|------|---------------|---------------|
| New partition (minimal, no bus) | 70 | 1 (partition impl) |
| New partition (bus-aware) | 145 | 1 (partition impl) + message type |
| New message type | ~14 per type | 1 (messages.rs, 73 LOC for 5 types) |
| New layer (compositor-as-partition) | ~30 | 0 (inline setup in 112-line nesting test) |

Boilerplate is dominated by the `Partition` trait's six required methods
(`id`, `init`, `step`, `shutdown`, `contribute_state`, `load_state`).
Bus-aware partitions add subscription setup and state serialization for
bus-specific fields but introduce no new concepts.

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
- Keys pre-computed to isolate buffer operations from allocation

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

1. **Boilerplate is reasonable** — 70 LOC for a minimal partition is competitive
   with trait-based frameworks. The `contribute_state`/`load_state` pair accounts
   for roughly half the boilerplate (TOML serialization). A derive macro could
   reduce this significantly.

2. **Bus-aware partitions cost ~2x minimal** — 145 LOC for Sensor vs 70 for Counter.
   The additional cost comes from subscription setup, message publishing, and
   bus-specific state fields — not from new concepts.

3. **Message types are lightweight** — 14 LOC per message type (struct + Message impl
   + doc comment). A derive macro could reduce this to ~5 LOC.

4. **Fractal nesting adds zero new concepts** — The compositor-as-partition pattern
   requires no additional API surface. Layer depth is metadata, not a structural change.

5. **Performance baselines established** — Benchmark suite provides regression
   detection for tick overhead, bus throughput, and buffer swap cost.

## Recommendations

- Consider a `#[derive(Partition)]` macro for the common case (state struct with
  known serialization format) to reduce boilerplate from ~70 to ~20 LOC.
- Consider a `#[derive(Message)]` macro to reduce message type boilerplate.
- Monitor benchmark regressions as the framework evolves.
