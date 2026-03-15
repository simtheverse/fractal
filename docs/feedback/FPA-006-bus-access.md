# FPA-006: Partition Bus Access Gap

## Finding

FPA-006 requires bus-mediated transition requests, and FPA-023 requires
bus-mediated dump/load requests. However, the original design used
`Box<dyn Bus>` (exclusive ownership) in the compositor, making it impossible
for partitions to hold bus references at construction time.

Without bus access, partitions cannot publish `TransitionRequest`,
`DumpRequest`, or `LoadRequest` messages — the bus-mediated request
paths specified by the spec are unreachable.

## Resolution

Changed `Box<dyn Bus>` to `Arc<dyn Bus>` in both `Compositor` and
`SupervisoryCompositor`. This enables shared ownership: callers create
an `Arc<dyn Bus>`, clone it for any partitions that need bus access,
then pass both the partitions and the `Arc` to the compositor constructor.
For partitions spawned later (via lifecycle ops), `Compositor::bus_arc()`
provides access to the shared bus reference.

The `Partition` trait remains strategy-neutral — bus access is opt-in at
the implementation level. Partitions that need bus access accept an
`Arc<dyn Bus>` in their constructor. Partitions that don't need it are
unaffected.

The compositor now subscribes to `TransitionRequest`, `DumpRequest`, and
`LoadRequest` at construction and drains them during its tick lifecycle:
- `DumpRequest` and `LoadRequest` are processed in Phase 1 (pre-tick,
  before any partition stepping)
- `TransitionRequest` is processed in Phase 3 (post-tick)

## FPA-023 tension: load idle precondition

FPA-023 requires load to occur "when no partition lifecycle methods are
in flight AND the execution state machine is in a non-processing state."
Bus-mediated `LoadRequest`s are drained in Phase 1 of `run_tick`, where
the state machine is Running (a processing state). However:

- No partition lifecycle methods are in flight during Phase 1 — stepping
  hasn't started yet.
- The spec notes that for lock-step compositors, "`load()` and `step()`
  cannot execute concurrently" — Phase 1 satisfies this by construction.
- The existing programmatic `request_load()` path uses the same Phase 1
  mechanism.

The formal state (Running) doesn't match the spec's "non-processing"
requirement, but the operational invariant (no concurrent partition
activity) is satisfied. This same tension exists for the programmatic
`request_load()` path. The external `load()` API enforces the stricter
Paused/Uninitialized check for callers outside the tick lifecycle.

## Spec implications

The spec should clarify that bus access is an implementation concern, not
a trait concern. The `Partition` trait intentionally does not include a
`set_bus()` method — this preserves strategy neutrality and avoids
coupling all partitions to the bus abstraction.

The spec may also benefit from distinguishing between "idle by state
machine" (Paused/Uninitialized) and "idle by construction" (Phase 1 tick
boundary where no partition methods are in flight). Both satisfy the
intent of FPA-023's load precondition.
