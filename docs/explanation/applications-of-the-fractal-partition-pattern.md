# Applications of the Fractal Partition Pattern

## Who is this for?

This document is for architects and technical leads evaluating whether the fractal
partition pattern (FPA) fits their system. It walks through three domains where the
pattern's properties — uniform structural primitives at every layer, independent
replaceability, and compositional configuration — solve real problems. The domains are
different enough to expose which FPA properties matter most in each context, and where
the pattern's tradeoffs land differently.

The pattern is not domain-specific. It applies wherever a system must be modular at
multiple scales simultaneously, and where that modularity must be navigable by people
who don't understand the entire system.

---

## The recurring problem

Most large systems need modularity, but they need it at different granularities. A
rendering engine needs swappable shaders, but also swappable render backends, and
ideally the engine itself should be embeddable in someone else's application. A robotics
stack needs swappable sensor processors, but also swappable perception pipelines, and
the stack as a whole must run identically in simulation and on the real robot.

The typical response is to build different mechanisms at each scale: a plugin system for
small components, a service interface for medium ones, and an embedding API for the
system as a whole. Each mechanism has its own conventions for configuration, error
handling, testing, and documentation. Contributors must learn a new set of abstractions
at each level. The cognitive cost grows with system depth, and so does the maintenance
burden.

The fractal partition pattern eliminates this by making the structural primitives
identical at every scale. Contracts, compositors, events, composition fragments, and
testing structure work the same way whether you're looking at the whole system or a
sub-component three layers deep. A contributor who understands how things work at one
layer understands how they work at every layer. The number of concepts stays constant as
the system grows deeper.

The three domains below all benefit from this, but they stress different aspects of the
pattern.

---

## Flight and vehicle simulation

### The challenge

A simulation framework must support multiple fidelity levels (simplified models for rapid
prototyping, high-fidelity models for validation), multiple deployment modes (interactive
with visualization, headless for batch runs, distributed across machines), and
contributions from people with varying expertise (students implementing control
algorithms, researchers adding environment models, operators configuring scenarios).

Conventional simulation frameworks address these needs with distinct mechanisms:
configuration files for fidelity selection, plugin APIs for model extension, network
protocols for distribution, and custom scripting for scenario authoring. Each mechanism
has its own learning curve.

### How FPA applies

**Layer 0** decomposes the system into functional domains: physics and plant modeling,
guidance/navigation/control, visualization, and user interface. Each is an independently
replaceable partition with contracts defined in a shared contract crate.

**Layer 1** decomposes each domain further. The physics partition composes atmosphere,
gravity, aerodynamics, and propulsion as independently replaceable sub-partitions. A
"fidelity level" is not a special concept — it is a named composition fragment that
selects a specific set of layer 1 implementations.

**Layer 2+** continues where the domain warrants it. An atmosphere model might compose a
base atmosphere, a turbulence model, and a wind model.

The pattern means that a student implementing a control algorithm interacts with the same
structural primitives (contracts, composition fragments, events) as a researcher adding
a new physics sub-model or an operator configuring a batch run. Configuration at every
scope uses the same TOML inheritance and override semantics. Events at every layer use
the same trigger/action schema. Testing at every layer uses the same contract test /
compositor test structure.

Transport independence — the same results across in-process, async, and network modes —
falls out of the layer-scoped bus design and double-buffered tick lifecycle, enabling
the same configuration to run interactively on a laptop or distributed across a cluster
without code changes.

### What the pattern primarily solves here

**Fidelity without a fidelity system.** The composition fragment mechanism that exists
for every other purpose also handles fidelity selection. No special-purpose machinery
is needed.

**Contributor onboarding.** Students, researchers, and operators all work within the same
structural primitives. A student who learns how to implement a contract for a control
algorithm already knows how contract tests work, how composition fragments select their
implementation, and how events interact with their partition — because those are the
same constructs at every layer.

**Deployment flexibility.** The same partition implementations run interactively,
headless, or distributed. Transport mode is a configuration choice in the layer 0
composition fragment.

---

## Robotics and autonomous systems

### The challenge

A robotics software stack typically includes perception (sensor fusion, object
detection), planning (path planning, task planning), control (motor controllers,
actuator drivers), and operator interface. These domains evolve at different rates —
perception algorithms change weekly, motor controllers change rarely — and are often
developed by different teams or sourced from different vendors. The system must also run
in multiple contexts: on the physical robot, in a software-in-the-loop simulation, and
in a hardware-in-the-loop test bench.

Existing frameworks (ROS, for example) provide a flat graph of nodes communicating over
topics. This works well for horizontal communication but does not naturally express
hierarchical decomposition. A perception stack that internally composes a LiDAR
processor, a camera processor, and a fusion node looks the same on the topic graph as
three unrelated nodes. There is no structural distinction between "these three nodes are
the perception partition" and "these three nodes happen to be running."

### How FPA applies

**Layer 0** partitions the system into perception, planning, control, and operator
interface. Contracts in the system-level contract crate define the typed messages between
them: fused world model from perception to planning, trajectory commands from planning
to control, operator directives from the interface to all partitions.

**Layer 1** decomposes each partition. The perception partition composes LiDAR processing,
camera processing, and sensor fusion as independently replaceable sub-partitions with
their own contracts. Each sub-partition can be swapped — a different LiDAR processor for
a different sensor — without touching camera processing or fusion.

The compositor at each layer owns a layer-scoped bus. The perception compositor publishes
a fused world model on the layer 0 bus; the internal LiDAR and camera messages stay on
the layer 1 bus. Replacing the perception partition's internal structure does not change
the messages visible to planning or control. This is the encapsulation guarantee that a
flat topic graph does not provide.

Transport independence means the same partition implementations can run in-process on
the robot, across threads in a simulation, or over a network for remote monitoring —
selected by configuration, not code changes.

Composition fragments configure the system at every scope. A field deployment fragment
selects production implementations with real sensor drivers. A simulation fragment
selects simulated sensor drivers and a physics-based environment. A test bench fragment
mixes real and simulated components. All use the same override and inheritance
semantics.

### What the pattern primarily solves here

**Hierarchical encapsulation that flat topic graphs lack.** The layer-scoped bus is the
key differentiator. In ROS, replacing the perception stack's internal decomposition
changes the topic graph visible to every other node. In FPA, the layer 1 bus is invisible
to layer 0 — planning and control see only what the perception compositor publishes on
the layer 0 bus, regardless of how perception is decomposed internally. This is a
structural guarantee, not a convention.

**Sim/real configuration as composition.** The difference between running on a physical
robot and running in simulation is a composition fragment override — swap real sensor
drivers for simulated ones, swap the motor controller for a simulated actuator model.
The same inheritance mechanism handles field deployment, simulation, and test bench
configurations without separate configuration systems for each context.

**Multi-vendor integration.** When perception, planning, and control come from different
vendors, the contract boundary is the integration boundary. Each vendor implements
against the contract at their layer. Contract tests verify conformance. Replacing a
vendor's component requires only that the replacement passes the contract tests.

### How this differs from simulation

Simulation's primary complexity is fidelity management and contributor diversity.
Robotics' primary complexity is **encapsulation across team and vendor boundaries** and
**configuration variants across deployment contexts** (real hardware, simulation, test
bench). The layer-scoped bus and compositor relay — which are useful but not central in
simulation — become the most important FPA properties in robotics, because they provide
the hierarchical encapsulation that existing flat frameworks lack.

Transport independence also serves a different purpose. In simulation, it enables
distributed execution for performance. In robotics, it enables the same code to run
on-robot (in-process for low latency) and off-robot (network transport for remote
monitoring or simulation) — a deployment flexibility concern rather than a performance
concern.

---

## Game engines and interactive applications

### The challenge

A game engine composes rendering, physics, audio, input handling, networking, and game
logic. Each domain has its own internal complexity — the renderer composes a scene graph,
material system, lighting pipeline, and post-processing chain. Modders and content
creators need to replace or extend functionality at multiple scales: a new shader, a new
rendering backend, a new physics solver, or a total conversion that replaces game logic
entirely.

Existing engines typically provide a plugin or extension API at one or two scales, with
different mechanisms at each. Unity's component system works at the entity level but
engine subsystem replacement requires source access. Unreal's module system works at the
subsystem level but has different conventions than its Blueprint scripting system.

### How FPA applies

**Layer 0** partitions the engine into rendering, physics, audio, input, networking, and
game logic. Each is independently replaceable — a different physics engine can be
substituted by providing a new implementation of the physics contract.

**Layer 1** decomposes each domain. The rendering partition composes scene management,
material processing, lighting, and post-processing. Each is independently replaceable,
enabling a modder to replace the lighting pipeline without touching materials or
post-processing.

The compositor at each layer manages execution ordering and inter-partition data flow.
The tick lifecycle ensures deterministic behavior: all partitions read from the previous
tick's outputs and write to the current tick's buffers. This eliminates the ordering
sensitivity that plagues many game engines (where the behavior changes depending on which
system runs first).

Composition fragments serve as the configuration surface at every scope. A game's
configuration is a layer 0 fragment selecting which partitions and presets to use. A
mod is a composition fragment that extends the base configuration and overrides specific
partitions or sub-partitions. A graphics preset is a named layer 1 fragment within the
rendering partition. All use the same inheritance and override semantics.

### What the pattern primarily solves here

**Deterministic execution without manual ordering.** The double-buffered tick lifecycle
is the key differentiator. Game engines are notorious for subtle bugs caused by system
execution order — physics reading input state that hasn't been updated yet, rendering
reading physics state mid-integration. The FPA tick lifecycle eliminates this entire
class of bugs by construction: every partition reads the previous tick's outputs, period.
The result is the same regardless of which partition steps first.

**Mods as composition fragments.** A mod is not a special concept requiring a mod API —
it is a composition fragment that extends the base configuration and overrides specific
partitions or sub-partitions. A total conversion overrides layer 0 partitions. A graphics
mod overrides layer 1 rendering sub-partitions. A configuration tweak overrides
individual parameters. All use the same inheritance and override semantics, and the
engine doesn't need to distinguish between "a mod" and "a configuration variant."

**Multi-scale extensibility with one mechanism.** Existing engines offer different
extension mechanisms at different scales (entity components vs. engine modules vs.
scripting). FPA provides one mechanism — contract-conforming partition implementations —
at every scale. A new shader is a layer 2 implementation within the material
sub-partition. A new rendering backend is a layer 1 implementation within the rendering
partition. A new physics engine is a layer 0 implementation. The contributor learns one
set of concepts.

### How this differs from simulation and robotics

The tick lifecycle matters in all three domains, but in game engines it solves a problem
that is **chronic and pervasive** — ordering sensitivity affects virtually every
inter-system interaction, every frame. In simulation, ordering sensitivity exists but is
typically managed by the physics integrator's structure. In robotics, real-time
constraints often dictate execution order explicitly.

Composition fragments serve different roles across the three domains. In simulation, they
primarily manage fidelity and contributor configurations. In robotics, they primarily
manage deployment context (real vs. simulated hardware). In game engines, they primarily
serve as the **modding and user configuration surface** — the mechanism by which end
users (not just developers) customize the system. This is a different audience with
different expectations, and the fact that composition fragments are human-readable,
inheritable, and overridable at every scope maps well to the modding use case.

The event system also serves a distinct role. In simulation, events drive mission
timelines (staging, failure injection). In robotics, events drive operational responses
(fault handling, mode transitions). In game engines, events drive **gameplay logic** —
a condition-triggered event that spawns enemies when the player enters a region, a
time-triggered event that changes the weather. The event mechanism is the same; the
vocabulary and the audience authoring events are fundamentally different.

---

## Choosing which FPA properties matter for your domain

The three domains above highlight that while FPA provides the same structural primitives
everywhere, different domains lean on different subsets:

| Property | Simulation | Robotics | Game Engines |
|---|---|---|---|
| Composition fragments | Fidelity selection, contributor config | Deployment context (real/sim/test) | Mods, user presets |
| Layer-scoped bus | Useful for encapsulation | **Critical** — hierarchical encapsulation flat frameworks lack | Useful for encapsulation |
| Tick lifecycle | Important for reproducibility | Important for real-time correctness | **Critical** — eliminates ordering sensitivity |
| Transport independence | Distributed execution for performance | Same code on-robot and in simulation | Less central (typically in-process) |
| Events | Mission timelines | Operational fault handling | Gameplay logic |
| Contract tests | Verifying student/researcher contributions | Verifying multi-vendor integration | Verifying mod compatibility |

The pattern earns its keep in systems that are heterogeneous, hierarchical, and must
support independent work at multiple scales simultaneously. If your system's primary
challenge appears in one of the rows above, the corresponding domain example shows how
FPA addresses it. If your system's challenges span multiple rows, the pattern addresses
them with the same set of primitives — that is the point.
