# Events as a Fractal Primitive

## What problem does it solve?

The fractal partition pattern says that structural primitives are uniform across layers.
Events are one of those primitives. But what does "uniform" mean for events? The event
*mechanism* — triggers, arming, TOML schema — is obviously the same everywhere. The
harder question is whether the event *vocabulary* — what actions events can invoke —
should also be uniform, or whether it should vary by layer and partition.

A physics partition needs to inject sensor faults, switch aerodynamic models, and trigger
structural failure modes. A GN&C partition needs to engage terminal guidance, arm abort
sequences, and transition estimator modes. A visualization partition needs to cut cameras,
trigger rendering effects, and switch display modes. These are all discrete occurrences
that should be schedulable and configurable — exactly what events are for — but they have
nothing to do with execution state.

Without partition-specific actions, partitions face an uncomfortable choice: encode
domain-specific event logic procedurally in their `step()` implementation (defeating the
purpose of a declarative event system), or register all actions in a global vocabulary
visible to all partitions (creating coupling that violates partition encapsulation). The
solution is to scope the action vocabulary to contract crates, while keeping the event
mechanism uniform everywhere.

## Contract-crate-scoped actions

All event actions are declared in contract crates. The set of actions available to events
at a given scope is determined by the contract-crate dependency graph — the same
mechanism that determines which typed messages and traits are available.

`sim-core` declares execution state actions: `"sim_stop"`, `"sim_pause"`,
`"sim_resume"`. Because every partition in the system transitively depends on `sim-core`,
these actions are available at every layer. This is not a special property of execution
state actions — it is the same visibility that makes `sim-core`'s typed messages
(like `ExecutionStateRequest`) available everywhere.

Each partition's contract crate declares actions relevant to its domain:

**Physics contract crate (layer 1):**
- `"inject_sensor_fault"` — corrupt a named sensor signal with specified bias or noise
- `"switch_aero_model"` — transition from one aerodynamic model to another mid-flight
- `"enable_structural_loads"` — activate structural load monitoring after a flight phase
- `"trigger_engine_failure"` — fail a named engine with specified failure mode

**GN&C contract crate (layer 1):**
- `"engage_terminal_guidance"` — switch guidance law for terminal phase
- `"arm_abort_sequence"` — arm the abort decision logic
- `"reset_estimator"` — reinitialize the navigation estimator state
- `"switch_control_mode"` — transition between control law configurations

**Visualization contract crate (layer 1):**
- `"transition_camera"` — switch to a named camera preset
- `"toggle_trail_rendering"` — enable or disable vehicle trail display
- `"activate_hud_overlay"` — show a heads-up display element

**Atmosphere contract crate (layer 2, within physics):**
- `"activate_dust_storm"` — inject a dust storm disturbance model
- `"transition_to_mars_model"` — switch atmospheric model for planetary transition

Actions declared in a partition's contract crate are available to events scoped within
that partition's hierarchy. A `[[physics.events]]` entry can use `"inject_sensor_fault"`
because the physics partition depends on the physics contract crate. It can also use
`"sim_stop"` because the physics partition transitively depends on `sim-core`. It cannot
use `"engage_terminal_guidance"` because the physics partition does not depend on the
GN&C contract crate.

## How actions are declared

A contract crate declares its action vocabulary as an explicit part of its contract
interface, alongside trait definitions and typed message declarations. This declaration
serves three purposes:

1. **Scenario authoring.** Authors know what actions are available when writing event
   entries for a partition. The vocabulary is discoverable and documented alongside the
   contract.

2. **Validation at load time.** When the compositor loads a scenario TOML, it validates
   that every action identifier in a partition's event entries is declared in a contract
   crate visible at that scope. An unrecognized action is a configuration error caught
   before execution begins.

3. **Encapsulation.** Actions are scoped to the declaring contract crate's dependency
   subtree. A physics action cannot be used in a `[[gnc.events]]` entry. This prevents
   cross-partition coupling through the event vocabulary — the same principle that
   prevents partitions from importing each other's types.

The exact declaration form (an enum, a set of string constants, a trait method returning
supported actions) is an implementation choice. The requirement is that the vocabulary is
explicit, inspectable, and enforceable at load time.

## Event handling at the compositor boundary

Actions scoped to a partition's contract crate are handled within that partition. The
outer layer does not receive an event notification — it sees only whatever the partition
publishes on the outer bus as a consequence of its internal state change.

When a partition-scoped action fires:

```
Scenario TOML: [[physics.events]]
  trigger = { condition = "altitude_msl < 500.0" }
  action = "deploy_landing_gear"
  parameters = { gear_id = "main" }

-> Physics partition's event dispatcher receives the trigger
-> Dispatcher invokes the "deploy_landing_gear" handler
-> Handler modifies internal physics state
-> Physics partition's next PlantState output reflects the change
-> Outer layer sees the updated PlantState, not the event
```

The outer layer does not know that an event fired. It observes the partition's outputs
through the bus, as always. If the event's effect is significant enough to warrant
outer-layer awareness — say, a structural failure that should stop the simulation — the
handler can emit `ExecutionStateRequest::Stop` on the local bus, or the event definition
can use `"sim_stop"` directly. The event system does not provide a separate propagation
path for action effects.

Actions at layer 2 (sub-partitions) are handled by the layer 1 compositor's event
dispatcher, not by the layer 0 orchestrator. The layer 1 compositor is the event
authority for its sub-partitions, just as it is the bus authority and the fault-handling
authority. If a sub-partition's event has consequences for the outer layer, the
compositor surfaces them through its own contract — either by emitting a request on the
outer bus or by reflecting the change in its own outputs.

Actions from `sim-core` (like `"sim_stop"`) follow this same pattern, but their handlers
emit typed bus messages (`ExecutionStateRequest`) that enter the arbitration pipeline
and propagate through the compositor relay chain. The dispatch mechanism is identical —
the handler's behavior is what differs.

## Mixing actions from different contract crates

A single partition can use actions from multiple contract crates in its event definitions,
limited to crates in its dependency graph. A physics partition might define:

```toml
# Action from physics contract crate: inject a fault at scenario time T+60s
[[physics.events]]
trigger = { time = 60.0 }
action = "inject_sensor_fault"
parameters = { sensor = "altimeter", bias = 50.0 }

# Action from sim-core: stop when structural load exceeds limit
[[physics.events]]
trigger = { condition = "structural_load > 9.0" }
action = "sim_stop"
```

Both entries use the same TOML schema. The event dispatcher resolves the action
identifier against the contract crates in scope, finds the declared handler, and invokes
it. The scenario author does not need to know which contract crate declares an action —
the schema is the same, and validation catches invalid identifiers at load time.

When multiple events fire in the same tick, ordering follows the event system's
evaluation semantics. Actions whose handlers emit bus messages interact with the
arbitration mechanisms already defined (SIM-SYS-043 for execution state conflicts).
Actions whose handlers modify internal state take effect in the order the event
dispatcher evaluates them.

## Time semantics across layers

SIM-SYS-036 establishes that time-triggered events use different time references at
different layers: wall-clock at layer 0, scenario time at layer 1. This applies to all
actions regardless of which contract crate declares them. An action from `sim-core` used
in a layer 1 event triggers on scenario time. The same action used in a layer 0 event
triggers on wall-clock time. The time reference is a property of the layer, not of the
action or its declaring contract crate.

This is another instance of the general principle: the mechanism (time-triggered
evaluation) is uniform, the semantics (which clock, what the time means in the domain)
vary by layer.

## Condition predicates and partition-scoped actions

Condition-triggered events (SIM-SYS-037) evaluate predicates against observable signals.
The observable signals include anything on the bus or in partition state. For events
scoped within a partition, the observable signals additionally include partition-internal
signals that are not published on the bus — exactly the capability described in
SIM-SYS-038 (partition-scoped event arming).

This means events can be triggered by conditions that the outer layer cannot observe. A
GN&C partition might arm an event on an internal estimator convergence metric that
triggers `"switch_control_mode"` — neither the trigger signal nor the action is visible
outside the partition. The event is fully encapsulated: defined in the partition's TOML
section, evaluated against internal signals, handled by an internal dispatcher, with
effects visible only through the partition's contract outputs.

## Design choices and tradeoffs

### Why contract-crate scoping, not a flat action namespace

A flat namespace — all actions registered in one place — would simplify the event
dispatcher but create cross-partition coupling. A change to the physics partition's
action vocabulary would affect the global namespace, potentially conflicting with
identifiers from other partitions. The namespace would need conventions
(`"physics.inject_sensor_fault"`) that duplicate the partition scoping already provided
by the TOML section structure (`[[physics.events]]`).

Contract-crate scoping avoids these problems. Each contract crate owns its namespace.
The dependency graph determines visibility. No global coordination is needed. This is
the same scoping strategy used for typed messages and traits — event actions are one
more contract-crate artifact.

### Why contract-crate declaration, not runtime registration

Actions could be registered at runtime — each partition announcing its supported actions
during initialization. This would allow dynamic action vocabularies but sacrifice static
validation. A typo in a scenario TOML (`"injecct_sensor_fault"`) would not be caught
until the event tries to fire at runtime, potentially deep into a long simulation run.

Contract-crate declaration makes the vocabulary inspectable and validatable at
configuration load time. It also makes the vocabulary part of the contract's documented
interface, visible to scenario authors and verifiable in contract tests. The tradeoff is
that adding a new action requires a contract crate change — but this is appropriate,
since a new event action is a new capability that should be version-tracked alongside
other contract changes.

### Why actions do not propagate through the relay chain

Actions could be propagated upward through the compositor relay chain, giving the outer
layer visibility into inner-layer events. But this visibility is exactly what the fractal
partition pattern's encapsulation is designed to prevent. If a physics partition's
`"inject_sensor_fault"` event were visible on the layer 0 bus, the orchestrator would
implicitly depend on the physics partition's internal action vocabulary. Replacing the
physics partition with one that uses different actions would change what the orchestrator
sees.

Actions stay internal because their effects are internal. The outer layer sees results
(changed outputs on the bus), not causes (which event fired). If the outer layer needs to
know about an event — for logging, diagnostics, or operator display — the partition can
publish a diagnostic message on its bus as a side effect of handling the action. This is
an explicit choice by the partition, not an automatic propagation by the event
infrastructure.

### Why the dependency graph is the right scoping mechanism

The contract-crate dependency graph already determines what types, traits, and messages
are visible at each scope. Using the same graph for event action visibility means there
is one scoping mechanism for all contract-crate artifacts. A contributor who understands
how typed messages become available at a given layer immediately understands how event
actions become available — the answer is the same.

This also explains why `sim-core`'s actions are available everywhere without special
treatment. Every partition depends on `sim-core`, so `sim-core`'s action vocabulary is
in scope everywhere — just as `sim-core`'s `ExecutionStateRequest` type is in scope
everywhere. The universality of execution state actions is a consequence of the
dependency graph, not a special category.

### Why the event mechanism stays uniform despite semantic variation

The key insight is that "uniform mechanism, varying vocabulary" is how every fractal
partition primitive works. The bus is the same mechanism everywhere, but different typed
messages flow at each layer. Contracts have the same structure everywhere, but different
traits and obligations per partition. Composition fragments use the same TOML syntax and
override semantics everywhere, but different fields per scope. Testing follows the same
contract-test/compositor-test structure everywhere, but different test content per
partition.

Events follow the same pattern. The trigger types, arming lifecycle, TOML schema, and
evaluation semantics are the mechanism — identical everywhere. The action identifiers
and their handlers are the vocabulary — scoped to contract crates and made visible
through the dependency graph. This is not a special case requiring special justification;
it is the fractal partition pattern applied consistently to one more primitive.
