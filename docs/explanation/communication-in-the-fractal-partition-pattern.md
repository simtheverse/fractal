# Communication in the Fractal Partition Pattern

## What problem does it solve?

A fractal partition system has two communication problems, not one. Partitions at the
same layer must exchange data without coupling to each other — the **inter-partition**
problem (horizontal). And compositors must communicate with their child partitions across
layer boundaries without violating encapsulation — the **inter-layer** problem
(vertical). These problems look similar but have different structures, and conflating
them leads to architectures that solve one while undermining the other.

The fractal partition pattern requires that communication mechanisms, like all structural
primitives, be uniform across layers. But uniformity does not mean identity. The same
principles — contracts in one place, typed messages, transparent transport — apply in
both directions, but the mechanisms differ because the relationships differ. Peer
partitions are symmetric; compositors and their children are not.

This document establishes the shared principles, draws the distinction between horizontal
and vertical communication, and points to the companion documents that develop each in
full detail.

## Shared principles

Three principles govern all communication in the fractal partition pattern, regardless
of direction:

### Contracts in one place

All data structures and behavioral contracts that cross a boundary — whether between
peers or between layers — are defined in a single contract crate (or contract module)
at the relevant scope. No partition imports types from another partition. No
sub-partition imports types from its compositor's internals. The contract crate is the
single source of truth for what may be said across any boundary at that layer.

### Typed messages

All data crossing boundaries travels as instances of named, versioned message types
declared in the contract crate. Compile-time type checking ensures that changes to the
message vocabulary are visible to every producer and consumer. The contract crate is
simultaneously the interface specification and the documentation of what flows between
components.

### Transport independence

Messages travel through a transport abstraction (the `Bus` trait) that supports
multiple modes — in-process, async, network. The active mode is selected at runtime.
No partition contains transport-specific code. The requirement that all transport modes
produce identical results is a correctness constraint, not a convenience.

## Two directions of communication

### Inter-partition (horizontal)

Peer partitions at the same layer exchange data through the bus. Partition A publishes
its output message type; Partition B consumes it. Neither knows the other exists. The bus
mediates. Communication is symmetric — any partition can publish, any can subscribe —
and the compositor at that layer is not involved in routing.

The inter-partition communication model is detailed in
[Inter-partition Communication](inter-partition-communication.md).

### Inter-layer (vertical)

The compositor communicates with its child partitions across the layer boundary. This
relationship is asymmetric: the compositor drives execution, the partitions respond. The
direction of communication determines the mechanism:

- **Downward (compositor → partitions):** Lifecycle control flows through trait method
  calls — `step()`, `init()`, `shutdown()`. Shared context flows through bus broadcasts
  on the layer's bus — the compositor publishes shared state and execution state, and
  partitions consume them alongside peer messages.

- **Upward (partitions → compositor):** Requests flow through the bus — a partition
  emits state-change requests or other typed requests, and the compositor receives
  them as the bus owner and arbitrates.

- **Relay (inner layer → outer layer):** When a request must reach a higher layer,
  the compositor acts as a relay gateway, receiving the request on its inner bus and
  deciding whether to forward it to the outer bus. The compositor has authority to
  filter, transform, or suppress — it is not a transparent pipe. Requests propagate
  through the relay chain until they reach the orchestrator.

The inter-layer communication model — including downward and upward communication,
relay chains, recursive state contributions, fault handling, and direct signals — is
detailed in [Inter-layer Communication](inter-layer-communication.md).

## How the two relate

The bus is **layer-scoped**. Each compositor owns a bus instance for its layer's
partitions. The layer 0 orchestrator owns the layer 0 bus. A compositor at layer 1
owns a separate layer 1 bus for its sub-partitions. These are distinct bus
instances, potentially with different transport modes.

```
Layer 0 bus:  orchestrator ↔ partition-A, partition-B, partition-C, partition-D
Layer 1 bus (A):  partition-A compositor ↔ sub-A1, sub-A2, sub-A3
Layer 1 bus (B):  partition-B compositor ↔ sub-B1, sub-B2, sub-B3
```

A sub-partition at layer 1 never publishes directly to the layer 0 bus. Normal requests
travel through the compositor relay chain — the layer 1 compositor receives a request on
the layer 1 bus, decides to relay, and emits on the layer 0 bus. Each compositor in the
chain can intercept, transform, or suppress. Safety-critical signals use the direct
signal mechanism (FPA-013), which bypasses bus relay policy within the declaring contract
crate's hierarchy — these exist for scenarios where compositor suppression would be
unsafe. Each compositor still filters direct signals against its own signal registry;
unregistered signal types are dropped at the layer boundary.

This scoping preserves encapsulation: the outer layer sees its partitions as opaque
units. It does not know — and cannot depend on — the internal structure of any partition.
A partition that decomposes into three sub-partitions looks identical on the layer 0 bus
to a partition that is monolithic. The bus boundary matches the contract boundary.

### What is the same

| Aspect | Inter-partition | Inter-layer |
|---|---|---|
| Contract location | Contract crate at that layer | Same contract crate |
| Typed messages | All boundary-crossing data | Same |
| Transport independence | Bus trait, 3 modes | Same, per layer |
| Requests not mutations | Partitions emit, owner arbitrates | Same — compositor is owner |
| Shared state machines | Defined in contract, single owner | Same |

### What differs

| Aspect | Inter-partition | Inter-layer |
|---|---|---|
| Relationship | Symmetric peers | Asymmetric parent-child |
| Bus instance | Shared among peers | Compositor owns, partitions participate |
| Lifecycle control | Not involved | Trait method calls (downward) |
| Routing across boundaries | Direct on shared bus | Compositor relay with authority |
| Fault handling | Not applicable | Compositor catches and translates |

## The compositor's dual role

The compositor's communication roles are part of its broader set of responsibilities —
assembly, lifecycle authority, bus ownership, arbitration, relay, fault handling, and
strategy adaptation. For the complete picture, see
[The Compositor in the Fractal Partition Pattern](the-compositor-in-the-fractal-partition-pattern.md).
The two communication roles that are most relevant here operate simultaneously:

1. **Bus owner for its layer.** It creates the bus, connects partitions, publishes
   shared context (aggregated partition state, execution state, tick metadata), and
   receives requests. In this role it is the layer's orchestrator — the single authority for arbitration and state ownership.

2. **Partition on the outer layer.** It implements the contract traits of the layer
   above, participates on the outer bus as a peer alongside other partitions, and
   contributes state when asked. In this role it is an opaque unit — the outer layer does
   not know it has children.

These roles connect through the relay mechanism: when the compositor (in role 1)
receives a request on its inner bus that needs to reach the outer layer, it (in role 2)
emits the appropriate message on the outer bus. The compositor is the bridge between
layers, and its authority over what crosses that bridge is what makes layer-scoped buses
compatible with system-wide coordination.

## Design choices and tradeoffs

### Why layer-scoped buses, not a global bus

A single global bus is simpler — any partition at any depth can publish directly to it,
and the orchestrator sees everything. But this breaks encapsulation: the orchestrator
would see messages from sub-partitions it shouldn't know about, transport mode selection
becomes system-wide rather than per-layer, and replacing a partition's internal structure
could change the messages visible on the global bus.

Layer-scoped buses preserve the contract boundary as the communication boundary. The
cost is the relay mechanism — compositors must explicitly forward inter-layer requests.
This cost is modest because inter-layer requests are infrequent relative to intra-layer
data flow, and the relay gives compositors a natural point to apply policy.

### Why the same principles but different mechanisms

A fully uniform system would use the bus for everything — lifecycle control, data flow,
requests — in both directions. But the compositor-partition relationship is inherently
asymmetric. The compositor must coordinate execution and manage the partition lifecycle.
Under the tick lifecycle convention (FPA-014), this includes enforcing tick boundaries
and controlling step order. These are imperative operations that require call-and-return
semantics, not publish-subscribe.

The principled split is: **trait calls for imperative control, bus for data flow and
requests.** This matches the nature of each communication type. Lifecycle control is targeted and coordinated by the compositor (the specific
dispatch mechanism — synchronous calls, async dispatch, remote invocation — depends on
the execution strategy and transport mode). Data flow and
requests are broadcast and decoupled (any partition can publish, any can subscribe).

### Why compositor authority over transparent relay

Transparent relay is simpler but violates the encapsulation guarantee. If a sub-partition
can emit any message that automatically appears on the outer bus, the outer layer
implicitly depends on the inner structure. The compositor's authority to filter and
transform ensures that the outer layer sees only what the compositor's contract promises,
regardless of what happens internally.

A compositor that chooses to relay transparently can do so — authority does not mandate
filtering. But the default is deliberate: the compositor decides what crosses the
boundary.

## Companion documents

- [Inter-partition Communication](inter-partition-communication.md) — the horizontal
  model: contract crate structure, typed message channels, transport modes, shared state
  machines, and the request/arbitration pattern.

- [Inter-layer Communication](inter-layer-communication.md) — the vertical model:
  layer-scoped buses, compositor runtime role, downward data broadcast, upward request
  relay, compositor relay authority, recursive state contributions, fault handling, and
  direct signals.
