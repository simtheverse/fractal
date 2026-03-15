# FPA-011: Fault Timeouts Should Be Configurable

## Finding

FPA-011 specifies per-invocation deadlines of 50ms for step/contribute_state
and 500ms for init/shutdown/load_state. The prototype previously hardcoded
these as constants. Different domains have fundamentally different timing
budgets:

- Industrial controller at 100 Hz: 10ms tick budget, needs <10ms step timeout
- Flight sim at 30 Hz: 33ms tick budget, 50ms step timeout is reasonable
- Kiosk at UI frame rate: 16ms frame budget, but event-driven partitions
  may legitimately take longer on specific frames

## Resolution

Timeouts are now configurable per-compositor via `TimeoutConfig`. The spec's
values (50ms/500ms) are the defaults. Domain applications set their own
values at compositor construction time.

## Recommendation

Update FPA-011 to state that the timeout values are domain-configurable
defaults, not fixed requirements. The spec should define the mechanism
(per-invocation deadline detection) and the default values, while allowing
domains to override based on their timing constraints.
