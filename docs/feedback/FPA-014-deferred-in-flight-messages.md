# FPA-014 Finding: In-Flight Message Asymmetry at Run Boundaries

## Status: Open — spec clarification needed

## Finding

Deferred delivery creates an in-flight message asymmetry at run boundaries.
In a 10-tick run with a Sensor→Follower→Recorder pipeline (scale=1.5,
threshold=5.0):

- Follower sends **6 commands** (ticks 5–10, reading previous-tick values)
- Recorder receives **5 commands** — tick 10's command is flushed to the bus
  but never consumed because no further tick executes

This parallels SharedContext's double-buffer boundary: Recorder sees 9 of 10
ticks' SharedContext entries because the first tick has no previous context
to read.

## Nature of the Asymmetry

Both communication channels exhibit one-tick-delay semantics, which means:

1. **First tick:** consumer reads nothing (no prior-tick data exists)
2. **Last tick:** producer's output is flushed but never consumed

This is an inherent property of the one-tick-delay model, not a bug. However,
the spec's message conservation requirement (FPA-037 compositor tests) could
be interpreted as requiring "all published messages are eventually consumed,"
which would be violated by the boundary condition.

## Spec Implication

FPA-037's message conservation property should clarify scope:

- **Option A:** Conservation applies within steady-state ticks only. Boundary
  conditions (first and last tick) are explicitly excluded. This matches the
  current behavior and is the simplest to specify.

- **Option B:** The compositor provides a "drain" phase after the last tick
  where consumers process any remaining flushed messages. This would require
  an additional step after the run loop, adding complexity for marginal benefit.

- **Option C:** Conservation is defined per-tick: "every message flushed at
  the end of tick N is available for consumption in tick N+1." The last tick
  trivially satisfies this because there is no tick N+1 to violate it.

Option C is the most precise and requires no behavioral change. The spec
should adopt this framing to prevent confusion about boundary conditions.
