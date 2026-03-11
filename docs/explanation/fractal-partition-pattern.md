# The Fractal Partition Pattern

## What problem does it solve?

Complex systems face a tension between modularity and coherence. A system that lets you
swap out one sub-component should also let you swap out the entire partition that contains
it, and ideally let someone swap out *the system itself* as a component in their larger
pipeline. But modularity at multiple scales typically requires multiple mechanisms —
plugin systems for small components, service interfaces for large ones, embedding APIs
for the framework as a whole — and each mechanism has its own conventions for
configuration, events, composition, and documentation.

The fractal partition pattern resolves this by making the answer the same at every scale.

## The core idea

The system is decomposed into **layers** and **partitions**. At each layer, partitions
are independently replaceable units that conform to contracts defined by the layer above.
The defining property — the **layer and partition uniformity principle** — is that the
same structural primitives appear at every layer:

- **Contracts** define what a partition must provide and what it may consume.
- **Compositors** select and assemble partition implementations at startup.
- **Events** are defined, armed, and triggered using the same schema at every layer.
- **Composition fragments** configure partition selections using the same TOML format,
  override semantics, and inheritance rules at every scope.
- **Specifications** follow the same requirement format with bidirectional traceability
  to the layer above.
- **Documentation** follows the same Diátaxis structure at every partition.
- **Testing** follows the same contract test / compositor test structure at every layer.

The pattern is called "fractal" because zooming into any partition reveals the same
structure as the system as a whole, and zooming out reveals that the system itself is a
partition in someone else's system.

## Layers in practice

A concrete example with three layers:

- **Layer 0 (system):** The top-level partitions — partition A, partition B, partition C,
  partition D — composed by the orchestrator (the application shell). Contracts live in
  the system core. Configuration at this layer is the **session** — transport mode,
  timestep, which scenarios to compose.

- **Layer 1 (partition):** Sub-components within each partition — subsystem X, subsystem
  Y, subsystem Z within partition A; subsystem P, subsystem Q, subsystem R within
  partition B. Configuration at this layer is the **scenario** — entity definitions,
  sub-model selections, environment models, events, initial conditions.

- **Layer 2+ (sub-partition):** Where a layer 1 sub-component is itself complex enough
  to warrant decomposition, the pattern continues. Subsystem X might compose a base
  model, a perturbation model, and an environmental model as independently replaceable
  units with their own contracts.

The pattern does not prescribe a fixed number of layers. Decomposition continues as deep
as the domain requires, and stops where it doesn't.

## What emerges from the pattern

Several properties that would otherwise need to be designed and maintained separately
fall out naturally from the uniformity principle.

### Fidelity is not a special concept

A "fidelity tier" is a named composition fragment that selects a specific set of layer 1
sub-models within a partition. `"low"` selects simplified models with fewer degrees of
freedom. `"high"` selects full-fidelity models with detailed dynamics. There is no
fidelity-specific machinery — it is the same named composition fragment mechanism
available to any partition's presets or configurations. Fidelity emerges from composition
rather than being engineered into the system.

### Configuration collapses to one concept

Sessions, scenarios, entity definitions, and partition presets are all **composition
fragments** — TOML blocks that select partition implementations at some
scope. They all support the same `extends` inheritance, the same inline override
semantics, and the same named-reference resolution. A session is a composition fragment
at layer 0 that references scenario fragments at layer 1. A scenario is a composition
fragment that references entity and partition fragments at narrower scopes.

An outer-layer fragment can override any field defined by an inner-layer fragment it
composes. This means a session file can fully define all scenario content inline —
useful for simple cases or self-contained examples — or it can reference separate
scenario files and override only the parameters relevant to a specific run. The same
mechanism works at every scope: a scenario can override individual sub-model selections
within a partition preset, an entity definition can override specific initial conditions
from a template.

### Events work everywhere

The event system — time-triggered, condition-triggered, declaratively defined in TOML —
is the same at every layer. A system-level event that pauses execution at wall-clock T+30s
uses the same schema as a partition B event that fires a mode transition at scenario time
T+60s, which uses the same schema as a partition A event that halts execution when a
monitored value exceeds a threshold.

The event **mechanism** is uniform. The event **vocabulary** is contract-crate-scoped.
All event actions are declared in contract crates, and the set of actions available at a
given scope is determined by the contract-crate dependency graph — the same mechanism
that determines which typed messages and traits are available. The system core declares
`system_pause`, `system_stop`, and `system_resume`; these are available everywhere
because every partition depends on the system core. Each partition's contract crate
declares actions for its own domain: partition A's contract crate defines
`"inject_fault"` and `"switch_model"`, partition B's contract crate defines
`"engage_mode"`, partition C's contract crate defines `"transition_view"`. All use the
same TOML schema,
the same trigger types, and the same arming lifecycle. What varies is the vocabulary, not
the mechanism.

This parallels how every other fractal primitive works. The bus is uniform in mechanism
but carries different typed messages at each layer. Contracts are uniform in structure
but specify different obligations per partition. Events are uniform in schema but invoke
different actions per scope. The mechanism is the fractal primitive; the vocabulary is
the contract-crate-scoped semantic content.

### Documentation and specifications propagate

Each partition maintains a `docs/` directory with the same Diátaxis quadrants (tutorials,
how-to, reference, explanation, design) and a `SPECIFICATION.md` with the same format.
A contributor navigating from a system-level partition into one of its sub-partitions
finds the same documentation layout, the same specification structure, and
the same traceability conventions. Specifications at each layer nucleate specifications
at the next layer — this document nucleates partition specs, which nucleate sub-partition
specs where warranted — propagating traceability and structural discipline to arbitrary
depth.

### Testing mirrors the partition structure

The fractal partition pattern produces a testing strategy without requiring one to be
designed independently. At each layer, **contract tests** verify that each partition
satisfies its contract traits in isolation — without instantiating peer partitions.
**Compositor tests** verify that the compositor at that layer correctly assembles its
partitions and that they interact through their contracts. The same contract test /
compositor test pair repeats at every layer of decomposition.

Contract tests are possible because the contract boundary is the isolation boundary. If
a partition can be replaced without modifying its peers, it can be tested without
instantiating its peers. The structural guarantee that makes the system modular is the
same guarantee that makes the tests fast and independent. When an alternative
implementation is provided — a third-party plugin or an alternative model — the existing
contract tests apply to it unchanged. If it passes, it is a valid replacement.

System-level tests exist only at the outermost layer. They exercise the full stack from
session configuration to final output and trace to system-level requirements. They are
expensive and slow relative to contract tests, and their role is not to catch bugs in
individual partitions but to verify end-to-end properties that emerge from the
interaction of all layers. The testing pyramid — many contract tests, fewer compositor
tests, few system tests — falls out naturally from the layer structure.

### The system is a partition in your system

The pattern applies outward. The system exposes a library interface that accepts a session
fragment, executes its processing, and returns control without assuming process ownership.
An automation pipeline can generate composition fragments, invoke the system as one stage
in a larger workflow, and consume its outputs. The system is a partition within that
pipeline's layer 0, swappable for an alternative engine through the same replaceability
guarantee the system provides to its own sub-partitions.

## Companion explanations

The fractal partition pattern is a single structural idea, but its consequences in
specific domains are detailed enough to warrant their own treatment. Each companion
document takes one aspect of the pattern — an aspect summarized in the sections above —
and explores it in full depth: the problems it creates, the design choices it forces,
and the tradeoffs involved.

### Communication

The contract-centric design summarized above creates a communication architecture with
two dimensions: horizontal (peer partitions at the same layer) and vertical (compositors
and their child partitions across layers). Both use the same principles — contracts in
one place, typed messages, transport independence — but the mechanisms differ because the
relationships differ.

[Communication in the Fractal Partition Pattern](communication-in-the-fractal-partition-pattern.md)
establishes the shared principles, draws the distinction between horizontal and vertical
communication, and points to the two detailed companion documents:

- [Inter-partition Communication](inter-partition-communication.md) — the horizontal
  model: contract crate structure, typed message channels, transport modes, shared state
  machines, and the request/arbitration pattern.

- [Inter-layer Communication](inter-layer-communication.md) — the vertical model:
  layer-scoped buses, compositor runtime role, downward bus broadcast, upward request
  relay, compositor relay authority, recursive state contributions, fault handling, and
  direct signals.

### Events

The event summary above describes a system where the mechanism is uniform but the
vocabulary is contract-crate-scoped. This raises questions about how actions are
declared, how the dependency graph determines action visibility, how the compositor
boundary affects event handling, and what happens when event-driven behavior at one
layer needs to influence another.

[Events as a Fractal Primitive](events-as-a-fractal-primitive.md) develops the full
event model: how actions are declared in contract crates, how the dependency graph
determines availability, how event handling respects the compositor boundary, and the
design rationale for scoping vocabulary rather than mechanism.

### Testing

The testing structure summarized above (contract tests, compositor tests, system tests
mirroring the layer structure) raises its own set of questions. What exactly is a
contract test isolating against? Why not use mocks? How do compositor tests avoid the
brittleness of integration tests? How do system tests relate to requirements traceability?

[Testing in the Fractal Partition Pattern](testing-in-the-fractal-partition-pattern.md)
develops the full testing methodology: precise definitions of the three test types, how
test isolation follows from contract isolation, why alternative implementations are
testable by construction, and the design rationale for each test category.

### Test reference data

The testing methodology in turn creates a data management problem. Tests at every layer
need reference inputs and expected outputs. In a fractal system, a contract change at any
layer can propagate upward through compositor and system test references across every
layer above it. How is reference data maintained without cascade invalidation?

[Test Reference Data in the Fractal Partition Pattern](test-reference-data-in-the-fractal-partition-pattern.md)
explains the reference data architecture: contract-owned canonical inputs and output
properties, compositional invariants for compositor tests, mechanically generated system
test baselines with bottom-up regeneration, and contract versioning as a propagation
boundary.

## Design choices and tradeoffs

### Why uniformity over specialization

Specialized mechanisms are often more ergonomic for their specific case. A purpose-built
fidelity system could offer a cleaner API for selecting model complexity levels. A
purpose-built event system for execution control could avoid the generality of condition
predicates.

The fractal partition pattern trades this local ergonomic advantage for global
consistency. A contributor who learns how composition fragments work in one partition's configuration
immediately understands how they work in any other partition's configuration, scenario
authoring, and session setup. A contributor who learns how events work at the
system level immediately knows how to define events within a partition. The number of
concepts in the system stays constant as the system grows deeper.

### Why layers over a flat plugin architecture

A flat plugin system (everything is a plugin at one level) avoids the complexity of
layers but sacrifices the ability to reason about scope and override direction.
Configuration override semantics depend on knowing which scope is "outer" and which is
"inner" — a session overrides a scenario, not the reverse. Event time semantics depend
on knowing which layer you are at — wall-clock at layer 0, scenario time at layer 1.
The layer concept makes these relationships explicit.

### Why not enforce a fixed depth

The pattern does not mandate that every partition decompose further. A partition with
a single implementation and no meaningful sub-components maintains the `docs/` structure
and specification format but has nothing to decompose. Forcing decomposition where the
domain doesn't warrant it produces empty abstractions. The pattern provides the mechanism
for decomposition and lets the domain determine the depth.

### Why specifications perpetuate

Requiring each layer's spec to nucleate the next layer's specs is more overhead than a
single system-level spec with all requirements. The tradeoff is traceability at scale:
when a system has dozens of independently replaceable sub-components across multiple
layers, a single flat spec becomes unmanageable. Layered specs with bidirectional
traceability make it possible to verify allocation (every system intent reaches an
implementor) and coverage (every implementation traces to an intent) at each layer
boundary independently. And besides, a single system-level spec can be generated by hydrating it with a recursion of inner-layer specs.
