# Bus Performance and Data Paths

This document explains how data flows between partitions in a Fractal Partition
Architecture system, the performance characteristics of each path, and the
design rationale behind the bus's type-erased runtime transport selection.

## Two data paths

An FPA system has two distinct paths for inter-partition data:

### 1. Double buffer (hot path)

Each tick, partitions write their output state to a **write buffer** slot. Other
partitions read from the **read buffer**, which contains the previous tick's
state. The buffer swap at the start of each tick makes the previous tick's
writes available for reading.

This path is:
- **Direct memory access** — no allocation, no serialization, no bus involvement.
- **Zero overhead per message** — the compositor writes to a pre-allocated slot.
- **Deterministic** — step order does not affect results because readers always
  see the previous tick's state.

This is where the high-frequency data lives: partition output (positions,
velocities, sensor readings, model state). It flows every tick for every
partition.

### 2. Bus (event path)

The bus carries:
- **SharedContext** — aggregated partition state, published once per tick by the
  compositor (LatestValue delivery).
- **Transition requests** — state machine transitions, commands (Queued delivery).
- **Inter-layer relay** — messages forwarded by the compositor between layers.

These are inherently lower-frequency than partition output. A transition request
might fire once per mode change. SharedContext is published once per tick (not
per partition).

## Bus type erasure: costs and mitigations

The bus uses an **object-safe core** (`dyn Bus`) to support runtime transport
selection (FPA-004). This requires type erasure — messages are boxed as
`Box<dyn CloneableMessage>` internally. The typed extension trait (`BusExt`)
restores compile-time types at the partition API.

### Per-message costs

| Operation | Cost | Notes |
|-----------|------|-------|
| Boxing (`Box::new(msg)`) | ~20-50ns | Heap allocation for the message |
| Clone per subscriber | ~20-50ns each | One clone per subscriber during publish |
| Virtual dispatch | ~2-5ns | Vtable call for `publish_erased` |
| Downcast on read | ~1ns | `TypeId` comparison |

### Typical workload

At 1000Hz tick rate with 20 partitions, SharedContext is published once per tick
with ~20 subscribers:

- 1000 publishes/second x 20 clones = 20,000 clone operations/second
- At 50ns each: **1ms/second** (~0.1% overhead)

This is negligible compared to the work partitions do in their `step()` methods.

### Built-in mitigations

**Latest-value slot reuse**: For LatestValue delivery, each subscriber has a
single slot. Publishing overwrites the previous value in place (the old Box is
dropped, the new one replaces it). There is no unbounded queue growth. All bus
implementations (`InProcessBus`, `AsyncBus`, `NetworkBus`) use this pattern.

**Dead subscriber cleanup**: `InProcessBus` and `NetworkBus` use `Weak`
references; `AsyncBus` prunes closed senders. In all cases, when a reader is
dropped, its subscriber entry is automatically pruned during the next publish.
This prevents memory growth from accumulated dead subscribers.

**Queued message consumption**: Queued subscribers drain their queue on read.
Under normal operation, queues stay shallow (typically 0-1 items between reads).

### When bus throughput matters

If a specific use case requires high-throughput bus messages (unlikely given the
double-buffer path handles the hot data), consider:

1. **SmallBox optimization** — Store messages <= 64 bytes inline, avoiding heap
   allocation. Most domain messages (a few f64 fields) fit in 64 bytes.

2. **Concrete bus type** — For performance-critical deployments where the
   transport mode is known at build time, the compositor can hold a concrete bus
   type instead of `dyn Bus`. The `BusExt` trait works identically on both
   concrete types and `dyn Bus`. This eliminates virtual dispatch and allows the
   compiler to inline and optimize.

## Design rationale

The split between object-safe `Bus` and typed `BusExt` follows a well-established
Rust pattern (similar to how `Any` works). The key properties:

1. **Partition code is transport-agnostic** — partitions call `bus.publish(msg)`
   and `bus.subscribe::<M>()` regardless of the underlying transport. No
   transport-specific imports or branches in partition code.

2. **Runtime transport selection** — the compositor accepts `Arc<dyn Bus>` at
   construction, enabling transport selection from configuration at startup.
   One binary supports all transport modes.

3. **Compile-time type safety** — `BusExt::publish<M>` and
   `BusExt::subscribe<M>` are fully generic. Type mismatches are caught at
   compile time. The type erasure is internal infrastructure, invisible to
   partition authors.

4. **Mixed transports per layer** — because compositors accept `Arc<dyn Bus>`,
   different layers can use different transports in the same run (e.g., network
   at layer 0, in-process at layer 1).
