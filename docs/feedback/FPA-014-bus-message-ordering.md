# FPA-014 Finding: Bus Messages Are Not Isolated by the Double Buffer

## Finding

FPA-014 guarantees intra-tick isolation: "no partition sees another partition's
current-tick output." The double buffer enforces this for SharedContext —
partition A's `contribute_state()` output from tick N is only visible to
partition B during tick N+1, regardless of stepping order.

However, direct bus messages published during `step()` bypass the double buffer
entirely. When Sensor publishes a SensorReading on the bus during its step,
Follower can read it immediately if Follower steps later in the same tick.
This creates a **stepping-order dependence** for direct bus communication:

- **Sensor-first order:** Follower sees current-tick reading → reacts immediately
- **Follower-first order:** Follower sees previous-tick reading → one-tick delay

With scale=1.5, threshold=5.0, and 10 ticks, Sensor-first produces 7 commands
while Follower-first produces 6. The double buffer's order-independence guarantee
does not extend to direct bus messages.

## Why This Matters

All four reference domain applications require intra-tick bus communication:
- Industrial controller: SafetyInterlock must react to current-tick SensorReadings
- Kiosk: OrderBuilder must see current-tick MenuSelections
- Flight sim: ControlLaw must read current-tick AircraftState
- Document editor: Renderer must see current-tick DocumentState

These patterns are correct and necessary. The question is whether FPA-014's
isolation guarantee should be understood as applying only to SharedContext
(the compositor's state observation mechanism) or to all inter-partition
communication including direct bus messages.

## Analysis

The double buffer solves the problem it was designed for: making `contribute_state()`
output available as a consistent snapshot. Direct bus messages serve a different
purpose — they are real-time communication within a tick, not historical state
observation. The reference domains treat these as distinct channels:

1. **SharedContext** (via double buffer): "what was everyone's state last tick?"
2. **Direct bus messages** (real-time): "what is happening right now?"

Isolating direct bus messages would require queuing all messages published during
Phase 2 and delivering them only in Phase 3 or the next tick. This would break
every reference domain's communication pattern and add latency where the domains
require immediacy.

## Recommendation

Clarify FPA-014 to distinguish two communication channels:

1. **State observation** (SharedContext, double buffer): order-independent, one-tick
   delay guaranteed. No partition sees another's current-tick `contribute_state()`.

2. **Direct bus messages** (publish/subscribe during `step()`): order-dependent,
   same-tick delivery to partitions stepped later. Stepping order is deterministic
   (BTreeMap by ID for composed systems, vector order for direct construction).

The deterministic stepping order means results are reproducible — the same
configuration always produces the same behavior. The order dependence is not
nondeterminism; it is a consequence of sequential execution that operators control
through partition naming.

This distinction should be explicit in FPA-014 rather than left implicit.

## Evidence

See `fpa_033_bus::stepping_order_affects_bus_communication` which demonstrates
the behavioral difference between the two orderings.
