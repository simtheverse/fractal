# FPA-013: Direct Signal Boundary Scoping

## Finding

FPA-013 requires direct signals to "not propagate beyond the declaring contract
crate's boundary." However, "contract crate" has no runtime identity in Rust —
there is no built-in mechanism for a compositor to know which crate declared a
signal.

Prior to this change, `collect_inner_signals()` unconditionally propagated ALL
signals from inner compositors to the outer layer, regardless of whether the
outer compositor recognized them. This violated the spec's boundary scoping
requirement.

## Resolution

The outer compositor's `DirectSignalRegistry` serves as the boundary filter.
`collect_inner_signals()` now filters inner signals against the outer registry:
only signals whose `signal_id` is registered at the outer layer propagate.

This is consistent with FPA principles:
- The compositor has full authority over its layer boundary (same as relay
  authority for messages under FPA-010)
- Registry-based filtering is explicit and declarative — the outer compositor
  must opt in to each signal it wants to receive
- Unregistered signals are silently dropped at the boundary, matching the
  relay Suppress semantics

## Spec Implication

The spec should clarify that "contract crate boundary" maps to compositor
boundary in the runtime. The `DirectSignalRegistry` at each layer defines
which signals cross that boundary. This is a natural consequence of the
compositor's role as the layer authority.
