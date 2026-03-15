# FPA-014 Resolution: Bus Message Isolation via DeferredBus

## Original Finding

FPA-014 guarantees intra-tick isolation: "no partition sees another partition's
current-tick output." The double buffer enforced this for SharedContext, but
direct bus messages published during `step()` were immediately visible to
partitions stepped later in the same tick. This created a stepping-order
dependence that violated FPA-014's guarantee.

With scale=1.5, threshold=5.0, and 10 ticks, Sensor-first produced 7 commands
while Follower-first produced 6 — demonstrating that partition order changed
observable behavior.

## Resolution: DeferredBus

`DeferredBus` is a Bus wrapper that queues messages published during Phase 2
(partition stepping) and flushes them after the tick barrier. This gives bus
messages the same one-tick-delay as SharedContext via the double buffer.

### How it works

1. Before Phase 2, the compositor sets `deferred(true)` on the bus
2. All `publish_erased` calls during stepping queue messages instead of
   delivering them
3. After all partitions have stepped and contributed state, the compositor
   sets `deferred(false)` and calls `flush()`
4. Queued messages are delivered to the inner bus in publish order
5. SharedContext is published after flush (non-deferred, direct delivery)

### Result

Both stepping orders now produce **6 commands** — Follower always reads the
previous tick's SensorReading regardless of whether it steps before or after
Sensor. FPA-014's isolation guarantee now holds for all inter-partition
communication, not just SharedContext.

## Impact on Reference Domains

All four reference domains benefit from this change:

- **Industrial controller:** SafetyInterlock reads previous-tick SensorReadings.
  The one-tick delay is acceptable because the compositor's tick rate is the
  system's reaction-time guarantee — if the tick is fast enough for safety,
  one-tick-delayed bus messages are too.
- **Kiosk:** OrderBuilder reads previous-tick MenuSelections. UI responsiveness
  is bounded by tick rate, not intra-tick ordering.
- **Flight sim:** ControlLaw reads previous-tick AircraftState. Flight dynamics
  simulations already operate on discrete timesteps; one-tick delay matches
  the simulation model.
- **Document editor:** Renderer reads previous-tick DocumentState. Rendering
  is inherently one step behind editing in any frame-based system.

## Spec Implication

FPA-014's intra-tick isolation now applies uniformly to both communication
channels:

1. **State observation** (SharedContext, double buffer): one-tick delay, as before
2. **Direct bus messages** (publish/subscribe during `step()`): one-tick delay
   via DeferredBus — messages published in tick N are readable in tick N+1

The previous recommendation to distinguish "real-time" vs "historical" channels
is superseded. All inter-partition communication has uniform one-tick-delay
semantics, which is simpler, more predictable, and satisfies the spec's
order-independence requirement.
