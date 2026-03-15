# FPA-014 Finding: Flush-on-Error Semantics for DeferredBus

## Status: Open — context document created, pending recovery phase design

See `docs/design/FLUSH_ON_ERROR_DESIGN.md` for full design context and candidate
approaches.

## Finding

When Phase 2 faults (a partition's `step()` returns an error), the compositor
currently extracts deferred-mode stepping into a helper that guarantees
`end_deferred()` runs regardless of whether all partitions
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

Deferred bus messages are always flushed, even when Phase 2 faults. However,
SharedContext is NOT published on a faulted tick — the compositor returns
early before the SharedContext publish block. This creates an asymmetry:

- **Bus messages:** flushed on error (partially-completed tick's messages
  reach subscribers)
- **SharedContext:** not published on error (subscribers see the previous
  tick's context)

Successfully-stepped partitions still contribute state to the write buffer
via `contribute_state()`, but that state is never assembled into SharedContext
and published on the bus when the tick faults.

## Spec Implication

FPA-014 should eventually specify flush-on-error semantics explicitly, aligned
with the recovery model. The current asymmetry between bus messages (flushed)
and SharedContext (not published) on faulted ticks may need resolution — either
both should flush, or neither should. Until recovery semantics are designed
(FPA-011 scope), the current behavior is acceptable.
