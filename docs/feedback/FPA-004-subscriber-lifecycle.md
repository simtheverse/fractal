# FPA-004: Bus Subscriber Lifecycle Management

**Requirement:** FPA-004 defines bus transport abstraction; FPA-007 defines delivery semantics; FPA-008 defines layer-scoped bus ownership.

**Issue:** All three bus implementations (`InProcessBus`, `NetworkBus`, `AsyncBus`) retain
subscriber entries indefinitely after `subscribe()` is called. When a `BusReader` is dropped,
its subscriber slot is never removed from the bus's internal list. This causes:

1. **Memory growth:** Dead subscriber entries accumulate over time
2. **Publish cost growth:** Every `publish()` iterates all subscribers including dead ones
   - In `InProcessBus`/`NetworkBus`, this means locking dead `Mutex<SubscriberState>` entries and cloning messages into them
   - In `AsyncBus`, `send()` on closed channels returns `Err` (currently ignored via `let _ =`)

**Impact:** Low in current prototype usage (short-lived buses in tests), but would matter for
long-running simulations or dynamic subscription patterns.

**Proposed Resolution:** Either:
1. Store `Weak<Mutex<SubscriberState>>` instead of `Arc`, and prune dead entries during `publish()` via `retain(|w| w.strong_count() > 0)`
2. Implement `Drop` on `BusReader` to deregister from the bus's subscriber list
3. For `AsyncBus`, retain only senders where `send()` succeeds (or check `is_closed()`) during publish

**Recommendation:** Option 1 — `Weak` references with lazy pruning during `publish()`. This is
the simplest change, requires no coordination between reader and bus, and naturally cleans up
when readers are dropped. Option 3 should additionally be applied to `AsyncBus` since it already
has the information (failed `send` return value).
