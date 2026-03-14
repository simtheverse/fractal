# fpa-compositor — Specification Traceability

Traces to: FPA-SRS-000, FPA-CON-000

## Requirements

| ID | Description | Status |
|----|-------------|--------|
| FPA-006 | Shared state machine with single owner | Pending |
| FPA-009 | Compositor runtime: lifecycle, bus, context, arbitration | Pending |
| FPA-010 | Relay authority for inter-layer communication | Implemented |
| FPA-011 | Fault handling with context and fallback | Pending |
| FPA-012 | Recursive state contribution | Implemented |
| FPA-013 | Direct signals bypass relay chain | Implemented |
| FPA-014 | Double-buffered tick lifecycle | Pending |
| FPA-022 | State snapshot as composition fragment | Pending |
| FPA-023 | Dump/load round-trip identity | Pending |
| FPA-024 | Event system integrated into compositor | Pending |

## FPA-010: Relay Authority

The compositor acts as a relay gateway for transition requests from inner partitions.
A `RelayPolicy` enum controls how inner requests are forwarded to the outer layer:

- **Forward**: pass the request unchanged.
- **Transform**: apply a transformation function before forwarding.
- **Suppress**: silently drop the request.
- **Aggregate**: collapse multiple requests into a single summary request.

Inner partitions submit requests via `submit_inner_request()`. The outer layer
retrieves forwarded requests via `drain_relayed_requests()`, which applies the
active relay policy.

## FPA-012: Recursive State Contribution

The `Compositor` implements the `Partition` trait, enabling vertical composition.
When `contribute_state()` is called on a compositor-as-partition, it delegates to
`dump()`, producing a nested TOML fragment containing:

- `partitions`: a table of sub-partition states keyed by partition ID.
- `system`: compositor metadata (tick count, etc.).

From the outer compositor's perspective, each inner compositor appears as a single
partition contribution. The outer `dump()` produces one entry per partition,
regardless of whether a partition is a leaf or a nested compositor.

`load_state()` delegates to `load()`, decomposing the TOML fragment and dispatching
sub-partition states to the appropriate inner partitions.

## FPA-013: Direct Signals

Direct signals bypass the relay chain, reaching the declaring crate's orchestrator
without passing through intermediate relay policies.

- **Registration**: signals are registered via `register_direct_signal()` on the
  compositor. Only registered signal IDs can be emitted.
- **Emission**: `emit_direct_signal()` creates a `DirectSignal` with signal ID,
  reason, emitter identity, and layer depth. The signal is stored in the
  compositor's `emitted_signals` vec for inspection by the orchestrator.
- **Scope enforcement**: signal registration is scoped to the declaring compositor.
  Unregistered signal IDs are rejected.
- **Logging**: every emission is recorded with identity and depth.
