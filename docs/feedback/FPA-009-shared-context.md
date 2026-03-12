# FPA-009: SharedContext Message Contract

**Requirement:** FPA-009 — Compositor Runtime Role

**Issue:** The spec says the compositor "publishes shared context on the bus each tick" but does not define SharedContext as a typed message contract. The implementation creates a `SharedContext` struct implementing the `Message` trait with:
- `state: toml::Value` (aggregated partition states)
- `tick: u64` (tick number)
- Delivery semantic: LatestValue

This message type is compositor-internal and not declared in any contract crate, which creates a tension with FPA-003 (inter-partition interface ownership) and FPA-005 (typed message contracts).

**Proposed Clarification:** Consider whether SharedContext should be:
1. A message type declared in the system-level contract crate (consistent with FPA-003/FPA-005)
2. A compositor-internal implementation detail (current approach, but then partitions reading SharedContext depend on the compositor crate rather than the contract crate)

The current approach works for the prototype but would need resolution in a production system where partitions should depend only on contract crates.
