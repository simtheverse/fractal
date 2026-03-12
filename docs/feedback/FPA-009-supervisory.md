# FPA-009: Supervisory Compositor Lifecycle Asymmetry

**Requirement:** Supervisory compositor where partitions run own processing loops.

**Issue 1 — Shutdown sync vs async:** The Partition trait requires synchronous
`shutdown()`, but supervisory shutdown requires awaiting async task completion.
Current resolution: sync shutdown sends signals but doesn't await completion.
Graceful shutdown available via separate `async_shutdown()` method.

**Issue 2 — dt parameter:** Supervisory partitions manage their own timing via
step_interval. The `dt` parameter from `Partition::step()` is unused. This is a
semantic mismatch: lock-step compositors use dt to advance simulation time, while
supervisory compositors run in wall-clock time.

**Issue 3 — State restoration:** `load_state()` can restore the output store but
cannot restart partition tasks from saved state. Live hot-reload of supervisory
partitions requires additional infrastructure (task respawning).

**Proposed Resolution:** Accept these as inherent asymmetries between execution
strategies. Document that supervisory compositors have different lifecycle semantics
than lock-step compositors. Phase 4 Track M (cross-strategy composition) must
account for these differences at strategy boundaries.
