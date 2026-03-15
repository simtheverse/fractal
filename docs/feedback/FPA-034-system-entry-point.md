# FPA-034 Gap: Standard Operator Entry Point

## Finding

FPA-034 says system tests must "use the same entry points available to an operator
or embedder" but the spec doesn't define what those entry points are. The prototype
fills this gap with `fpa_compositor::compose::compose()`:

1. Takes a `CompositionFragment` + `PartitionRegistry` + `Bus`
2. Creates partitions from config via registry lookup
3. Wires events from the fragment
4. Returns a ready-to-use `Compositor`

The `System` type in fpa-testkit wraps `compose()` with a batch `run()` method
for test and reference generation use cases. Interactive and event-driven
applications use `compose()` directly and drive the compositor from their own
event loop.

## Implication

Without a defined entry point, every FPA application will invent its own
composition-from-config pattern, leading to fragmentation. The `compose()`
function establishes the canonical pattern: config-driven partition creation
through a registry, with the bus injected for transport selection.

## Recommendation

Add a spec requirement (or clarify FPA-034) defining the standard operator
entry point: a function that takes a composition fragment, a partition registry,
and a bus, and produces a compositor. This makes FPA-034 testable against a
concrete API rather than an implicit convention.
