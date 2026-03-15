# FPA-014 Finding: Flush-on-Error Semantics for DeferredBus

## Status: Open — deferred to recovery phase

## Finding

When Phase 2 faults (a partition's `step()` returns an error), the compositor
currently extracts deferred-mode stepping into a helper that guarantees
`set_deferred(false)` and `flush()` run regardless of whether all partitions
stepped successfully. This means messages from successfully-stepped partitions
are flushed to the bus even when the tick is considered faulted.

## Tension

For flat (non-nested) compositors, flush-on-error is benign: the tick either
completes or the compositor halts/falls back. But for nested compositors where
an outer layer recovers from an inner compositor's fault, partially-completed
tick messages reaching the bus may be misleading:

- **Scenario:** Inner compositor has partitions A, B, C. A and B step
  successfully and publish messages. C faults. The inner compositor flushes
  A and B's messages before reporting the fault. The outer compositor's
  recovery logic now sees messages from a tick that didn't complete —
  potentially violating "all-or-nothing" tick semantics.

- **Counter-argument:** Discarding messages on error is also problematic.
  If the fault handler retries C and succeeds, discarding A and B's messages
  loses legitimate work. The right behavior depends on recovery strategy
  (retry, skip, rollback), which isn't designed yet.

## Current Behavior

Flush always runs. This is the simpler, more predictable choice. It matches
the SharedContext double buffer, which also publishes after a partially-faulted
tick (successfully-stepped partitions still contribute state).

## Spec Implication

FPA-014 should eventually specify flush-on-error semantics explicitly, aligned
with the recovery model. Until recovery semantics are designed (FPA-011 scope),
the current flush-always behavior is acceptable.
