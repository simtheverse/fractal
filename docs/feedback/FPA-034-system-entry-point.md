# FPA-034 Gap: Standard Operator Entry Point

## Finding

FPA-034 says system tests must "use the same entry points available to an operator
or embedder" but the spec doesn't define what those entry points are. The prototype
fills this gap with a `System` type in fpa-testkit that:

1. Takes a `CompositionFragment` + `PartitionRegistry` + `Bus`
2. Creates partitions from config via registry lookup
3. Builds and runs a compositor through the standard lifecycle

## Implication

Without a defined entry point, every FPA application will invent its own
composition-from-config pattern, leading to fragmentation. The `System` type
establishes the canonical pattern: config-driven partition creation through
a registry, with the bus injected for transport selection.

## Recommendation

Add a spec requirement (or clarify FPA-034) defining the standard operator
entry point: a function that takes a composition fragment, a partition registry,
and a bus, and produces a running compositor. This makes FPA-034 testable
against a concrete API rather than an implicit convention.
