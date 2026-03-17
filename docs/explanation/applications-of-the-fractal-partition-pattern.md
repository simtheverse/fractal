# Applications of the Fractal Partition Pattern

## Who is this for?

This document is for architects and technical leads evaluating whether the fractal
partition pattern fits their system. (The Fractal Partition Architecture — FPA — is the
specification of this pattern.) It describes the class of system that the
pattern produces, the properties that emerge from it, and the domains where those
properties are most valuable.

---

## What kind of system does the pattern produce?

The fractal partition pattern is not a domain-specific architecture. It is a structural
discipline that, when applied, produces a system with a specific set of emergent
properties. These properties are not features that need to be designed and maintained
independently — they fall out of the pattern's core constraint: the same structural
primitives (contracts, events, configuration, composition, specification, documentation
structure, and testing structure) apply identically at every layer of decomposition.

Five properties define the class of system that results. These are representative, not
exhaustive — additional emergent properties include fault detection and propagation
(FPA-011), safety-critical signal bypass (FPA-013), and declarative event-driven
behavior (FPA-024 through FPA-029).

### Distributable execution

Every partition at every layer communicates through a layer-scoped bus with typed
messages. The bus abstraction supports multiple transport modes — in-process, async
cross-thread, and network — selectable per layer by configuration. A partition does not
know or care which transport mode is active. It reads typed messages from the bus and
writes typed messages to the bus.

This means the system's deployment topology is a configuration choice, not an
architectural one. The same partition implementations that run in a single process on a
developer's laptop can run across threads for parallel execution, across separate processes on the same
machine, or across machines over a network — without code changes. Different layers can use different transport modes
simultaneously: layer 0 over a network for distributed execution while layer 1 runs
in-process for low latency.

### Hot-swappable functional units

Every partition at every layer is independently replaceable. This is not a plugin system
bolted onto an existing architecture — it is the architecture itself. A partition
conforms to a contract defined at its layer. Any implementation that satisfies the
contract can be substituted without modifying any peer partition's source code.

The compositor at each layer selects and assembles partition implementations at startup
based on composition fragments, then coordinates their lifecycle at runtime — owning
the layer's bus, arbitrating requests, relaying inter-layer messages, and handling
faults. Changing which implementation is active requires only a configuration change. The same mechanism works at every scale: swapping a top-level
subsystem, swapping a sub-component within a subsystem, or swapping a sub-sub-component
three layers deep.

Layer-scoped buses ensure that swapping a partition's internal structure is invisible to
the outer layer. A partition that was monolithic yesterday and is decomposed into three
sub-partitions today looks identical on the outer bus — the compositor publishes the same
typed messages regardless of internal structure. This means replaceability holds not just
for leaf implementations but for entire sub-hierarchies.

### Managed complexity at scale

As a system grows, the fractal partition pattern keeps the conceptual footprint constant.
A contributor working on a sub-component three layers deep encounters the same structural
primitives as a contributor working at the system level: the same contract/compositor
structure, the same composition fragment format, the same event schema.

This is qualitatively different from a system that accumulates distinct mechanisms at each
scale. In such systems, understanding the whole requires learning every mechanism. In a system built on the
Fractal Partition Architecture, understanding one layer is understanding every layer. The depth of the system
can grow — new layers of decomposition can be added where the domain warrants it —
without increasing the number of concepts a contributor must hold in their head.

Composition fragments provide a unified configuration surface at every scope. The same
inheritance and override semantics that configure the system at layer 0 also configure
sub-components at layer 1 and beyond. There is no per-domain configuration mechanism.
A named fragment at any scope is a portable, version-controllable, shareable artifact
that can be extended and overridden without forking. State snapshots are also composition
fragments — a checkpoint captured mid-run is a valid configuration input, loadable,
inheritable, overridable, and editable with a text editor. A modified checkpoint is an
`extends` with overrides. State management is not a separate system bolted onto
configuration — it is configuration, using the same mechanisms as everything else.

### Independent development of connected pieces

The contract boundary is the independence boundary. Two partitions at the same layer can
be developed by different teams, different organizations, or different contributors who
never communicate — provided both implement against the shared contract. The contract
crate is the single source of truth for what each side must provide and what each side
may consume.

This independence is structural, not conventional. The compiler enforces that no
partition imports types or traits from another partition's implementation — only from
the contract crate. A contributor can develop, test, and validate their partition in
complete isolation, confident that if it satisfies the contract, it will integrate
correctly.

The same independence holds at every layer. A team developing a sub-component at layer 1
depends only on the layer 1 contract defined by their parent partition — not on sibling
sub-components, not on the layer 0 contract, not on the system as a whole. They can work
in their own repository, with their own release cadence, and integrate at the contract
boundary.

Contract versioning bounds the propagation of changes across these boundaries. When a
contract changes, the change is expressed as a new version. Implementations targeting
the previous version remain testable and valid until they choose to migrate. This allows
independent teams to absorb contract changes on their own schedule.

### Structural testability

If a partition is independently replaceable, it is independently testable. This is not
a testing strategy bolted onto the architecture — it is a consequence of the partition
structure. The contract boundary is the isolation boundary: a partition can be
instantiated, invoked through its contract traits, and verified in isolation without
its peers. Contract tests are possible because the architecture makes isolation possible.

When an alternative implementation is provided — a new vendor's component, a student's
submission, an experimental algorithm — the existing contract tests apply to it without
modification. If it passes, it is a valid replacement. Testability scales with the
system: every new partition or sub-partition automatically has a well-defined test
surface defined by its contract.

The same structure produces a natural testing pyramid. Contract tests verify individual
partitions in isolation (many, fast). Compositor tests verify that the compositor
correctly assembles partitions and that they interact through their contracts (fewer,
slower). System tests verify end-to-end properties (few, slowest). This pyramid is not
designed independently — it mirrors the layer structure.

---

## Where these properties converge

The five properties above are independently useful, but their value compounds when a
system needs all of them at once. A system that must distribute execution across machines
*and* swap functional units *and* remain navigable as it grows *and* support independent
teams developing against shared interfaces *and* remain testable at every layer — that
system is paying for five separate architectural concerns. In a fractal partition system,
these are not five separate concerns. They are five consequences of one structural
discipline.

This convergence is what makes the pattern particularly well suited to a family of
domains that share a common shape: heterogeneous functional domains that must interact
through well-defined interfaces, operate across varying deployment topologies, support
substitution of components at multiple scales, and remain tractable as they grow in depth
and contributor count.

### Simulation systems

Simulation systems — flight simulation, vehicle dynamics, multi-body physics, training
platforms — compose physics modeling, control algorithms, environment models,
visualization, and operator interfaces. They must run interactively on a workstation,
headless in a batch farm, or distributed across a cluster. They must support fidelity
selection (swapping simplified models for high-fidelity ones), contributor diversity
(students, researchers, and operators working at different layers), and multi-vendor
integration (a third-party physics engine alongside an in-house control system).

These are not separate requirements needing separate solutions. Fidelity selection is
hot-swappable functional units — a named composition fragment selects which sub-model
implementations are active. Batch and distributed execution is distributable execution —
transport mode is a configuration choice. Contributor diversity is managed complexity —
everyone works with the same structural primitives regardless of which layer they touch.
Multi-vendor integration is independent development — each vendor implements against the
contract at their layer. And when a student submits a new control algorithm, the existing
contract tests verify it without custom test effort — structural testability.

### Cyber-physical systems

Cyber-physical systems — robotics, autonomous vehicles, hardware-in-the-loop test
platforms, drones — compose software that operates across the boundary between
computation and the physical world. The same software must run on the real platform, in
a software-in-the-loop simulation, and on a hardware-in-the-loop test bench. Components
come from different vendors and evolve on different schedules. The system must be
navigable by teams spanning mechanical engineering, electrical engineering, control
theory, and software engineering.

The boundary between real and simulated is a partition swap — a real sensor driver
replaced with a simulated sensor model, a real actuator controller replaced with a
simulated plant. The system does not distinguish between "real" and "simulated" — it
composes whatever implementations the composition fragment selects. A test bench that
mixes real and simulated components is just another configuration. On the real platform,
partitions run in-process for minimal latency; in a distributed test setup, they run over
a network. The same code, the same contracts, the same tests — the contract tests that
verify a component in simulation verify it on the real platform, because the contract
boundary is the same.

### Game engines and interactive applications

Game engines compose rendering, physics, audio, input handling, networking, and game
logic, each with deep internal structure. They must support extension by engine
developers, studio teams, and end-user modders — all at different scales. A modder
replaces the lighting pipeline. A studio replaces the physics engine. A user selects a
graphics preset. These are the same operation at different layers, expressed through
composition fragments.

Mods, presets, and total conversions are all composition fragments with the same
inheritance and override semantics. There is no separate mod API, no separate plugin
system, no separate preset mechanism — the architecture *is* the extension system. And
when a modder submits a new rendering sub-partition, the existing contract tests verify
that it conforms to the rendering contract without the engine team writing mod-specific
tests.

---

## Conventions that complement the pattern

The emergent properties above follow from the pattern's structural primitives alone. In
practice, FPA systems benefit from additional conventions that are not inherent to the
pattern but fit naturally within it. These conventions are documented separately:

- **[Tick Lifecycle and Synchronization](tick-lifecycle-and-synchronization.md):**
  A double-buffered execution model where every partition reads the previous tick's
  outputs and writes to the current tick's buffers. This convention, layered onto FPA's
  bus and compositor structure, produces deterministic reproducibility (results are
  identical regardless of partition execution order) and makes concurrent and distributed
  execution safe by eliminating ordering sensitivity. It is a design choice — not a
  consequence of the pattern — but it leverages the pattern's layer-scoped buses and
  compositor-driven execution to powerful effect. The core architecture does not mandate
  tick-based execution; systems may use multi-rate, fully asynchronous, or other execution
  strategies while remaining FPA-conforming.

- **[Testing in the Fractal Partition Pattern](testing-in-the-fractal-partition-pattern.md):**
  A testing methodology (contract tests, compositor tests, system tests) and a
  traceability discipline (Diataxis documentation, bidirectional requirement tracing,
  test files named by requirement). The testing *pyramid* emerges from the partition
  structure, but the specific methodology and traceability practices are conventions
  that the structure supports well.

---

## When the pattern is not the right fit

The fractal partition pattern carries structural overhead: contracts, compositors,
layer-scoped buses, and composition fragments. This overhead is justified when the system
is heterogeneous, hierarchical, and must support independent work at multiple scales.

It is less justified when:

- **The system is flat.** If all components are peers at a single level with no
  meaningful hierarchical decomposition, the layer machinery adds cost without benefit.
  A flat plugin architecture may be simpler and sufficient.

- **The system is small.** If the entire system can be understood by one person and
  rarely changes, the independence guarantees are solving a problem that doesn't exist.

- **Performance dominates structure.** If the system's primary constraint is raw
  throughput and every abstraction boundary is a potential bottleneck, the bus abstraction
  and compositor indirection may be unacceptable. The pattern is designed for systems
  where correctness, modularity, and development scalability are at least as important as
  raw performance.

- **The domain is truly homogeneous.** If every component is the same kind of thing (a
  collection of identical workers, a uniform pipeline of identical stages), the
  partition/contract/compositor structure adds unnecessary differentiation. A simpler
  model — a pool, a chain, a dataflow graph — may be a better fit.
