# FPA-009: Multi-rate Shared Context Update Frequency

**Requirement:** "Shared context updated at each sub-step"

**Ambiguity:** Does "updated" mean:
(a) The double buffer write slot is overwritten per sub-step (current impl), or
(b) SharedContext is published on the bus after each sub-step?

**Current Implementation:** Option (a) — the double buffer receives the latest state
after each partition completes all its sub-steps. SharedContext is published on the
bus once per outer tick after all partitions step.

**Rationale:** Publishing on the bus per sub-step would create intermediate states
visible to subscribers that don't correspond to a consistent system snapshot (other
partitions haven't stepped yet). This violates snapshot semantics.

**Proposed Resolution:** Clarify that "shared context updated at each sub-step" means
the partition's write buffer slot is overwritten, not that a bus publication occurs.
