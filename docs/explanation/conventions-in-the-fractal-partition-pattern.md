# Conventions in the Fractal Partition Pattern

## What this document covers

The fractal partition pattern defines structural primitives — layers, partitions,
contracts, compositors, composition fragments, layer-scoped buses — from which certain
properties emerge: distributable execution, hot-swappable functional units, managed
complexity, independent development, and structural testability. These are documented in
[Applications of the Fractal Partition Pattern](applications-of-the-fractal-partition-pattern.md).

This document covers conventions that are not inherent to the pattern but fit naturally
within it. They are design choices that leverage the pattern's structure to produce
additional valuable properties — properties that do not fall out of the pattern alone but
that the pattern makes straightforward to achieve.

A convention is distinct from an emergent property. An emergent property is unavoidable
given the structural primitives: if partitions are independently replaceable, they are
independently testable — there is no design choice involved. A convention is a deliberate
choice that could have been made differently: the tick lifecycle could use a different
synchronization model, the traceability discipline could use a different documentation
framework. The conventions described here are not the only valid choices, but they are
choices that the pattern's structure supports particularly well.

---

## Tick lifecycle and double-buffered execution

### The convention

The compositor at each layer executes each tick as a three-phase lifecycle:

1. **Pre-tick processing:** Process pending lifecycle operations (spawn, despawn, state
   dump/load), assemble shared context from the previous tick's outputs, publish shared
   context into the write buffer, and swap the read/write buffers.

2. **Partition stepping:** Each partition reads inter-partition messages from the read
   buffer (previous tick's outputs) and writes its outputs to the write buffer (current
   tick's outputs). No partition observes another partition's current-tick output during
   this phase.

3. **Post-tick processing:** Evaluate event conditions, collect outputs, process bus
   requests, and relay qualified requests to the outer bus.

The key mechanism is double-buffering: partitions read from one buffer and write to
another. The buffers are swapped between ticks. This means every partition sees a
consistent snapshot of the previous tick's state, regardless of execution order.

### What this convention produces

**Deterministic reproducibility.** Because no partition sees another partition's
current-tick output, the result is identical regardless of which partition steps first.
Reordering partitions, running them sequentially or concurrently, stepping them on
different threads or different machines — the output is the same. A bug can be reproduced
from a configuration file alone, without recreating the exact deployment topology,
thread schedule, or execution order.

This is qualitatively different from systems where inter-partition communication is
immediate. In such systems, the result depends on execution order: if partition A reads
input before partition B updates it, the behavior differs from the case where partition B
updates first. This ordering sensitivity is a chronic source of bugs, particularly in
game engines and real-time simulations. The double-buffered tick lifecycle eliminates it
by construction.

**Safe concurrent execution.** The double-buffered approach makes concurrent stepping
safe without locks on the read path. Each partition reads from the read buffer (which is
not being written to) and writes to the write buffer (which is not being read from by
other partitions). Write paths can be isolated per partition. This means a compositor can
step partitions concurrently as an optimization — particularly valuable under network
transport where sequential stepping would impose serialized round-trip latency — without
any additional synchronization beyond the tick barrier (all partitions complete before
Phase 3).

**Transport-independent results.** The transport independence guarantee (same results
across in-process, async, and network transport) is architecturally enabled by the
layer-scoped bus — the bus abstraction makes distribution *possible*. But without the
tick lifecycle, different transport modes could produce different results due to message
timing. The double-buffered approach makes distribution *deterministic*: because
visibility is governed by buffer swaps at tick boundaries rather than by message arrival
time, the transport mode cannot affect which messages a partition sees during a given
tick.

### Why this is a convention, not an emergent property

The pattern's structural primitives — layer-scoped buses, typed messages, compositors
that drive partition execution — do not mandate double-buffered ticks. An FPA-conforming
system could use immediate message visibility, or event-driven execution without a fixed
tick, or a different synchronization model entirely. The tick lifecycle is a specific
execution model that *leverages* the compositor's control over partition execution and the
bus's message routing to achieve determinism. It is a powerful choice, but it is a choice.

### The full specification

The tick lifecycle is specified in detail in FPA-014 (Compositor Tick Lifecycle). The
companion explanation document
[Tick Lifecycle and Synchronization](tick-lifecycle-and-synchronization.md) develops the
design rationale, the concurrency model, and the interaction with direct signals and
event evaluation.

---

## Traceability discipline

### The convention

Each layer's specification nucleates the next layer's specifications. Every requirement
in a child specification includes a `Traces to:` field referencing identifiers in the
parent specification. Test files are named by the requirement they verify (e.g.,
`tests/fpa_001.rs` for requirement FPA-001). Each partition maintains a `docs/` directory
with a uniform structure following the Diataxis documentation framework: tutorials,
how-to guides, reference, explanation, and design (containing the specification).

### What this convention produces

**Auditable verification.** When a requirement changes, the affected child specifications
are identifiable by their `Traces to:` fields. The affected tests are identifiable by
their filenames. The affected documentation is locatable by the uniform directory
structure. This makes it possible to answer "what is the verification evidence for this
requirement?" and "what is affected if this requirement changes?" by following links
rather than searching.

**Coverage analysis at layer boundaries.** Bidirectional traceability — every parent
requirement referenced by at least one child, every child requirement referencing at
least one parent — makes it possible to verify allocation (every system intent reaches
an implementor) and coverage (every implementation traces to an intent) at each layer
boundary independently. Gaps are visible structurally.

**Navigable documentation at scale.** A contributor moving from the system level into a
partition into a sub-partition finds the same documentation layout at every stop. The
Diataxis structure (tutorials for learning, how-to guides for tasks, reference for
information, explanation for understanding) is the same regardless of layer. This reduces
the cost of navigating an unfamiliar part of the system.

### Why this is a convention, not an emergent property

The pattern produces layers, partitions, and contracts — natural boundaries to trace
along. But the traceability itself is manual work. Someone has to write the `Traces to:`
fields. Someone has to name the test files. Someone has to create and maintain the
documentation directories. The pattern provides the structure that makes traceability
tractable; the convention provides the discipline that makes it happen.

A different traceability approach — a traceability matrix in a spreadsheet, a tagging
system in a test framework, a different documentation structure — could be used with the
same pattern. The Diataxis layout and requirement-named test files are choices that fit
the pattern well, not consequences of it.

### The full specification

The traceability discipline is specified in FPA-030 (Partition-level Specifications and
Documentation Structure) and FPA-031 (Test Coverage of Requirements). The testing
methodology is specified in FPA-032 through FPA-039 and explored in the companion
documents [Testing in the Fractal Partition Pattern](testing-in-the-fractal-partition-pattern.md)
and [Test Reference Data in the Fractal Partition Pattern](test-reference-data-in-the-fractal-partition-pattern.md).

---

## Identifying further conventions

The two conventions above are the most consequential — the tick lifecycle produces
determinism, and the traceability discipline produces auditability. But the boundary
between "emergent from the pattern" and "convention that fits the pattern" is worth
revisiting as the pattern is applied to new domains. A property that feels inherent in
one context may reveal itself as a convention when the pattern is applied in a context
where that property is not needed or where a different choice would be better.

The test for whether something is emergent or conventional: if you could build a
conforming FPA system that does not have the property, it is a convention. If the
property is unavoidable given the structural primitives, it is emergent.
