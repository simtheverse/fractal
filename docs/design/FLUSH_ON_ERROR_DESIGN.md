# Flush-on-Error Design Context

## Status: Open — pending recovery phase design

## The Asymmetry

When Phase 2 faults (a partition's `step()` returns an error), the two inter-partition
communication channels behave differently:

- **Bus messages:** The deferred queue completes — messages from successfully-stepped
  partitions are flushed and delivered to subscribers.
- **SharedContext:** Not published — the compositor transitions to Error before reaching
  the SharedContext publish step, so subscribers see the previous tick's context.

Successfully-stepped partitions still contribute state to the write buffer via
`contribute_state()`, but that state is never assembled into SharedContext and published
on the bus when the tick faults.

## Why It Matters

For flat (non-nested) compositors, flush-on-error is benign: the tick either completes or
the compositor halts/falls back. But for nested compositors where an outer layer recovers
from an inner compositor's fault, partially-completed tick messages may violate
all-or-nothing tick semantics.

**Example scenario:** Inner compositor has partitions A, B, C. A and B step successfully
and publish messages. C faults. The inner compositor flushes A and B's messages before
reporting the fault. The outer compositor's recovery logic now sees messages from a tick
that didn't complete — potentially violating "all-or-nothing" tick semantics.

## Counter-argument

Discarding messages on error is also problematic. If the fault handler retries C and
succeeds, discarding A and B's messages loses legitimate work. The right behavior depends
on recovery strategy (retry, skip, rollback), which isn't designed yet.

## The Design Space

Four candidate approaches:

**(a) Discard bus messages on error (all-or-nothing):** Align bus messages with
SharedContext — neither is published when a tick faults. Provides clean all-or-nothing
semantics but loses legitimate work from successfully-stepped partitions.

**(b) Publish SharedContext on error too:** Align in the other direction — both channels
flush on error. Provides maximum information to recovery logic but publishes partial tick
state that may mislead downstream consumers.

**(c) Make flush behavior configurable per recovery strategy (FlushPolicy):** Let the
domain configure whether bus messages are flushed or discarded on error, per compositor.
Maximally flexible but adds configuration complexity and may lead to subtle behavioral
differences across deployments.

**(d) Align both channels once recovery semantics are designed:** Defer the decision
until the recovery model (retry, skip, rollback) is designed. The correct flush behavior
depends on what the recovery strategy does with the partial results — a decision that
can't be made in isolation.

## Dependencies

Resolution depends on the recovery model design. The recovery model must answer:
- Does recovery retry the faulted partition within the same tick?
- Does recovery skip the faulted partition and continue?
- Does recovery roll back the entire tick?

Each answer implies different flush-on-error behavior. Designing flush semantics before
recovery semantics risks choosing an approach that conflicts with the eventual recovery
model.

## Current Behavior

The prototype flushes bus messages on error but does not publish SharedContext. This
asymmetry is documented as-is, not yet specified in the spec. The feedback file
`docs/feedback/FPA-014-deferred-flush-on-error.md` tracks this finding.

## References

- FPA-014 (Compositor Tick Lifecycle) — defines Phase 2 stepping and tick barrier
- FPA-011 (Compositor Fault Handling) — defines fault detection and propagation
- `docs/feedback/FPA-014-deferred-flush-on-error.md` — original finding
