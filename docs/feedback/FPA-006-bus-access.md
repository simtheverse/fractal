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
`SupervisoryCompositor`. This enables shared ownership: the compositor
holds an `Arc<dyn Bus>`, and partitions can receive a clone of the same
`Arc` at construction time via `Compositor::bus_arc()`.

The `Partition` trait remains strategy-neutral — bus access is opt-in at
the implementation level. Partitions that need bus access accept an
`Arc<dyn Bus>` in their constructor. Partitions that don't need it are
unaffected.

The compositor now subscribes to `TransitionRequest`, `DumpRequest`, and
`LoadRequest` at construction and drains them during its tick lifecycle:
- `DumpRequest` and `LoadRequest` are processed in Phase 1 (pre-tick)
- `TransitionRequest` is processed in Phase 3 (post-tick)

## Spec implications

The spec should clarify that bus access is an implementation concern, not
a trait concern. The `Partition` trait intentionally does not include a
`set_bus()` method — this preserves strategy neutrality and avoids
coupling all partitions to the bus abstraction.
