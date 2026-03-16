# The Compositor in the Fractal Partition Pattern

The compositor is the most complex component in an FPA system. It appears at every layer
of the hierarchy, and at every layer it plays the same set of roles. This document
describes all of them in one place.

Other explainers cover specific aspects in depth. This document is the overview — it
explains what the compositor is, what it does, and why each role exists. Cross-references
point to the detailed treatments.

## What is a compositor?

A compositor is a component that sits at a layer boundary. It has two faces:

- **Inward**, it is the authority over its sub-partitions. It assembles them, coordinates
  their execution, owns their bus, arbitrates their requests, handles their faults, and
  publishes shared context for them to consume.

- **Outward**, it is a partition. It implements the contract traits defined at the outer
  layer, receives lifecycle invocations from the outer compositor (or orchestrator), and
  publishes its output on the outer bus. The outer layer does not know or care that the
  compositor has sub-partitions — it sees an opaque partition that conforms to a contract.

At layer 0, the compositor is called the orchestrator. It has no outer layer — it is the
top-level coordinator. At layer 1 and beyond, each compositor is simultaneously a
partition on the layer above and an authority over the layer below. This dual identity
is what makes the fractal structure work: the same component that coordinates execution
at one scale participates as a peer at the next scale up.

## Role 1: Assembly

At startup, the compositor reads composition fragments (FPA-019, FPA-020, FPA-021) to
determine which partition implementations to instantiate at its layer. A layer 0
orchestrator reads a layer 0 composition fragment and selects top-level partitions. A
layer 1 compositor reads a layer 1 composition fragment and selects sub-partitions. The
mechanism is the same at every layer — composition fragments with inheritance and
override semantics. Assembly uses the standard composition entry point (FPA-015), which
accepts a composition fragment, a partition registry mapping implementation names to
constructors, and a bus instance.

The compositor resolves `extends` chains, applies inline overrides, and instantiates the
selected implementations via registry lookup. It connects each partition to the layer's
bus and verifies that all contract dependencies are satisfied. Assembly is complete before
any runtime processing begins.

This role is the least controversial — most component frameworks have an assembly phase.
What distinguishes FPA is that the assembly mechanism is uniform across layers: the same
composition fragment format, the same override semantics, and the same named-reference
resolution at every scale.

## Role 2: Lifecycle authority

The compositor controls when partitions may initialize, process, and shut down. No
partition at a given layer begins or ends its lifecycle without the compositor's
involvement. This is the core invariant that holds across all execution strategies
(FPA-009).

The degree of control varies:

- **Direct invocation.** The compositor calls each partition's lifecycle methods (`init`,
  `step`, `shutdown`) and waits for completion. It controls ordering and timing. This is
  the strongest form of lifecycle authority.

- **Supervisory coordination.** Partitions run their own processing loops. The compositor
  manages lifecycle boundaries — it signals when partitions may start, when they must
  stop, and monitors them for faults. It does not call `step()`, but it controls the
  conditions under which processing is permitted.

- **Multi-rate.** The compositor calls `step()` on different partitions at different
  rates, or groups partitions into rate tiers. It controls ordering within each rate
  group.

The execution strategy is an implementation choice, scoped to the compositor's layer.
Different compositors at different layers may use different strategies. See
[Execution Strategies in the Fractal Partition Pattern](execution-strategies-in-the-fractal-partition-pattern.md)
for the full treatment.

## Role 3: Bus owner

Each compositor owns a bus instance for its layer's partitions (FPA-008). The bus is the
sole communication channel between partitions at that layer. The compositor creates the
bus, connects partitions to it, and selects the transport mode (in-process, async, or
network) via configuration.

As bus owner, the compositor publishes **shared context** — aggregated state, execution
state, environment context — as typed messages on the bus. Partitions consume shared
context using the same subscription mechanism they use for peer data. From a partition's
perspective, there is no distinction between data published by the compositor and data
published by a peer — both arrive as typed messages on the bus.

This uniformity is deliberate. It means partitions do not need separate consumption
mechanisms for compositor-originated and peer-originated data. It also means the
compositor can change what it publishes (e.g., adding a new shared context type) without
changing how partitions consume data. See
[Communication in the Fractal Partition Pattern](communication-in-the-fractal-partition-pattern.md)
and
[Inter-partition Communication](inter-partition-communication.md)
for details on bus communication.

## Role 4: Arbitrator

The compositor is the single owner for shared state machines at its layer (FPA-006).
When a partition wants to change shared state — request a lifecycle transition, modify a
coordination variable, trigger a phase change — it emits a typed request on the bus. The
compositor receives the request, evaluates it against the state machine's transition
rules, and applies or rejects it.

Single-owner arbitration prevents conflicting mutations. The compositor evaluates each
request against the state machine's transition rules and applies or rejects it. When
multiple requests arrive in the same processing cycle, they are processed sequentially
using a deterministic, transport-independent rule defined by the domain-specific
specification. All requests and resolutions are logged, providing an audit trail.

The arbitration pattern is the same at every layer. At layer 0, the orchestrator
arbitrates system-level state machines. At layer 1, a partition's compositor arbitrates
that partition's internal state machines. The mechanism is identical; only the scope and
the specific state machines change. See
[Inter-partition Communication](inter-partition-communication.md)
for the request-not-mutation pattern.

## Role 5: Relay gateway

When a request on the inner bus needs to reach an outer layer, the compositor acts as a
relay gateway (FPA-010). It has full authority over what crosses its layer boundary:

- **Relay as-is.** Forward the request unchanged. Common for requests where the
  compositor has no reason to intervene.
- **Transform.** Modify the request before relaying — add context, change the request
  type, convert an internal warning into an external stop request.
- **Suppress.** Handle the request internally without forwarding. The compositor might
  respond to a sub-partition failure by switching to a fallback rather than propagating
  the failure outward.
- **Aggregate.** Combine multiple requests from a single processing cycle into one
  consolidated message on the outer bus.

Relay authority preserves encapsulation. The outer layer sees only what the compositor's
contract promises, regardless of internal events. A compositor that relays everything
transparently is making a deliberate choice, not following a default. See
[Inter-layer Communication](inter-layer-communication.md)
for the full relay chain treatment.

## Role 6: Fault handler

When a sub-partition faults during any lifecycle invocation — `init()`, `step()`,
`shutdown()`, `contribute_state()`, or `load_state()` — by returning an error, panicking,
or timing out, the compositor catches the fault, adds diagnostic context (which partition,
which layer, which operation), and responds (FPA-011).

The response is:

1. **Fallback.** If a fallback implementation is configured for the faulting partition,
   the compositor activates it, logs the fault and fallback activation, and continues
   processing. The fallback must have the same partition identity as the primary. The
   outer layer does not see an error, but the fault is recorded.
2. **Propagate.** If no fallback is configured, the compositor returns an error from its
   own lifecycle method call, cascading through the compositor chain to the orchestrator.
   The compositor transitions to Error state before returning.

The compositor enforces per-invocation elapsed-time deadlines for all lifecycle calls.
Default values are 50 ms for step/contribute_state and 500 ms for
init/load_state/shutdown; domains configure values appropriate to their constraints.
Deadline enforcement cannot be disabled.

The fault detection mechanism varies by execution strategy. Under direct invocation, the
compositor catches errors and panics from the call itself and enforces deadlines. Under
supervisory coordination, the compositor detects faults through heartbeat monitoring,
connection state, or error messages on the bus. The fault handling policy is the same
regardless of detection mechanism — only the detection latency differs.

The compositor never silently absorbs a fault. Every fault is logged with full diagnostic
context, and the compositor either activates a configured fallback or propagates the error.
There is no third option.

## Role 7: Partition on the outer layer

From the outer layer's perspective, the compositor is just a partition. It implements the
contract traits defined at the outer layer, receives lifecycle invocations from the outer
compositor, and publishes its output on the outer bus. The outer layer does not know
whether the partition is a leaf implementation or a compositor with sub-partitions inside.

This is the encapsulation guarantee. A partition that decomposes into three
sub-partitions looks identical on the outer bus to a partition that is monolithic. The
bus boundary matches the contract boundary. Replacing a compositor-based partition with
a monolithic one (or vice versa) is transparent to the outer layer, provided both
conform to the same contract.

The compositor's output on the outer bus is an aggregation of its sub-partitions'
outputs, transformed and shaped to match the outer contract. How the aggregation works —
whether it is a simple passthrough, a weighted combination, or a complex synthesis — is
the compositor's implementation detail.

## Role 8: Strategy adapter

When the compositor's internal execution strategy differs from the outer layer's, the
compositor adapts between them. A supervisory compositor embedded in a lock-step system
translates a synchronous `step()` call into coordination with async sub-partitions. A
lock-step compositor embedded in a supervisory system runs its own processing loop and
publishes results at its own rate.

This adaptation is what makes execution strategies composable across layers. The outer
layer's strategy does not constrain the inner layer's strategy, and vice versa. The
compositor at the boundary translates.

When the inner strategy produces output asynchronously, the compositor's output may
reflect previously computed state rather than state computed for the current invocation.
The compositor communicates this through the **StateContribution** envelope — a wrapper
defined in the contract crate that wraps all `contribute_state()` output with freshness
metadata: the `state` itself, a `fresh` flag indicating whether it was computed for the
current invocation, and an `age_ms` field indicating how stale the data is.

Consumers read the freshness metadata and decide how to handle stale data — proceed
normally, interpolate, or flag a degraded condition. Under uniform lock-step execution,
freshness is trivially "fresh" every cycle. It becomes meaningful at layer boundaries
where strategies diverge.

See
[Execution Strategies in the Fractal Partition Pattern](execution-strategies-in-the-fractal-partition-pattern.md)
for the full treatment of strategy composability and freshness.

## How the roles connect

The roles are not independent — they interact in specific ways:

**Assembly feeds lifecycle.** The compositor assembles partition implementations, then
coordinates their execution. The composition fragments determine *what* runs; the
lifecycle authority determines *when* and *how*.

**Bus ownership enables arbitration.** The compositor can arbitrate requests because it
owns the bus and therefore receives all messages. A partition cannot bypass the
compositor's arbitration because there is no other communication path at that layer.

**Arbitration feeds relay.** The compositor arbitrates a request internally and then
decides whether to relay it outward. The arbitration result (applied, rejected, or
pending) may influence the relay decision.

**Fault handling interacts with lifecycle and relay.** A fault detected during a lifecycle
invocation may trigger a fallback (lifecycle), an error propagation (relay to the outer
layer), or both. The compositor's fault handling policy determines which path is taken.

**The outer-partition role constrains all other roles.** The compositor must satisfy its
outer contract. Its lifecycle coordination, bus management, arbitration, relay, and fault
handling all serve the goal of producing output that conforms to the outer layer's
expectations. Internal complexity is irrelevant to the outer layer — only the contract
output matters.

## The compositor at every scale

The fractal partition pattern is named for the self-similarity of structure at every
scale. The compositor is where this self-similarity is most visible. A layer 0
orchestrator assembling four partitions performs the same roles — assembly, lifecycle
authority, bus ownership, arbitration, relay, fault handling — as a layer 2 compositor
assembling three sub-sub-partitions. The scale changes; the roles do not.

This means a contributor who understands how the orchestrator works at layer 0
immediately understands what a compositor does at layer 1 or layer 2. The number of
concepts stays constant as the system grows deeper.

It also means that the compositor is the most complex component to implement correctly.
It must handle assembly, lifecycle coordination, bus management, arbitration, relay, fault
handling, outer-contract conformance, and strategy adaptation — all in a single component
at every layer. This complexity is concentrated by design: the compositor is complex so
that partitions can be simple. A partition implements a contract trait and publishes on
the bus. All coordination complexity lives in the compositor.

## Related documents

- [Execution Strategies in the Fractal Partition Pattern](execution-strategies-in-the-fractal-partition-pattern.md) —
  direct invocation, multi-rate, supervisory coordination, strategy composability, and
  data freshness
- [Communication in the Fractal Partition Pattern](communication-in-the-fractal-partition-pattern.md) —
  horizontal and vertical communication, the compositor's dual communication role
- [Inter-partition Communication](inter-partition-communication.md) —
  the contract crate, typed messages, transport independence, the request-not-mutation
  pattern, shared state machines
- [Inter-layer Communication](inter-layer-communication.md) —
  relay authority, direct signals, state snapshots, fault propagation
- [Tick Lifecycle and Synchronization](tick-lifecycle-and-synchronization.md) —
  the three-phase tick lifecycle convention (one specific execution strategy)
- [The Fractal Partition Pattern](fractal-partition-pattern.md) —
  the overall pattern and its emergent properties
