# Test Reference Data in the Fractal Partition Pattern

## What problem does it solve?

The fractal partition pattern produces tests at every layer — contract tests, compositor
tests, and system tests, recursing as deep as the decomposition warrants. Each of these
tests needs reference data: inputs to supply, outputs to expect, tolerances to check
against. In a flat system with one layer of contracts, managing this data is
straightforward — a contract changes, its tests update, done. In a fractal system, a
contract change at any layer can propagate upward through every layer above it, silently
invalidating reference data in compositor tests and system tests that transitively depend
on the changed contract's outputs.

This document explains the reference data architecture that prevents that propagation
from becoming a maintenance burden. The architecture follows the same principle as the
rest of the fractal partition pattern: structural boundaries do the work.

## The cascade problem

Consider a concrete example. A sub-partition at layer 2 (inside a layer 1 partition)
improves its calculation algorithm. The change is correct — it passes the sub-partition's
contract tests. But the improvement changes the sub-partition's outputs slightly, which
changes the parent partition's composed output at layer 1, which changes the system's
end-to-end output at layer 0.

If tests at each layer assert against exact golden files — captured output from a
previous run — then this single sub-partition improvement invalidates:

- The sub-partition's contract test golden file (layer 2)
- The parent partition's compositor test golden file (layer 1)
- The layer 0 compositor test golden file
- System test reference outputs

The contributor who improved the sub-partition must now understand and update golden
files across four layers, possibly owned by different teams, in different parts of the
repository. If they miss one, the stale golden file either causes a false test failure
(annoying but safe) or, worse, the test framework's tolerance is loose enough that it
still passes against the old reference (silent incorrectness).

This is not a hypothetical problem — it is a direct consequence of the fractal
structure. The same depth that makes the system modular makes reference data management
a cross-cutting concern that touches every layer.

## The architecture: four principles

The reference data architecture is specified in FPA-036 through FPA-039. The
requirements are precise; this section explains the reasoning and the relationships
between them.

### Principle 1: Reference data lives at the contract boundary

*Specified in FPA-036.*

Each contract owns two things for testing purposes:

- **Canonical inputs** — representative instances of the contract's input types,
  constructed using the contract's own type system.
- **Expected output properties** — invariants, tolerances, and constraints that any
  conforming implementation must satisfy for those inputs.

The critical distinction is between **output properties** and **output values**. A
contract test for a sub-partition does not assert "given this input, the output value
is exactly 223.15." It asserts "given this input, the output is within the contract's
stated tolerance band" or "the output satisfies a monotonicity invariant" or "the output
satisfies conservation within 1e-6 relative error."

These are properties of the contract, not properties of a specific implementation. They
change when the contract changes and are stable across implementation changes. This
eliminates cascade invalidation at the contract test level entirely: improving a
sub-partition's accuracy does not change the contract's invariants, so the contract
tests do not need updating.

The canonical inputs and expected output properties live alongside the contract
definition — in the same module, maintained by the same author. When a contract's
behavioral requirements change, the reference data changes as part of the same commit.
There is no separate golden file that can drift.

### Principle 2: Compositor tests assert compositional properties

*Specified in FPA-037.*

The cascade problem is worst at the compositor level, because a compositor test's output
depends on the combined behavior of every partition it composes. If a compositor test
uses exact golden output, it is coupled to every partition's implementation details
simultaneously.

The solution mirrors Principle 1: compositor tests assert **compositional properties**
rather than exact values. Compositional properties are invariants of correct assembly,
independent of any specific partition's numerical output:

- **Message delivery**: partition A sends a message on channel X, partition B receives
  it in the same tick (or next tick, per the contract).
- **Conservation**: structural invariants are preserved — e.g., the write buffer has
  exactly one entry per active partition, the set of active partition IDs is stable
  across ticks when no lifecycle ops occur. Domain-level conservation (energy, mass,
  momentum) across partition
  boundaries is a future concern that reference domains would exercise.
- **Ordering**: execution order respects the declared dependency graph — partition A runs
  before partition B, which runs before partition C.
- **Visibility**: state that one partition publishes is visible to its declared consumers
  at the time specified by the contract.

These properties are stable across partition implementation changes because they derive
from the composition structure, not from numerical outputs. Replacing the default
implementation with an alternative doesn't change whether messages are delivered or
conserved quantities are preserved — it changes the specific output values, which the
compositor test doesn't assert.

Where compositor tests genuinely need regression baselines with exact values — for
example, to detect unintended behavioral changes — those baselines are generated
mechanically from the current implementations. Regeneration is a single command, not a
manual editing process. The generation command and the implementation versions used are
recorded alongside the baseline, so provenance is always traceable.

### Principle 3: System test references are generated bottom-up

*Specified in FPA-038.*

System tests are the one place where exact end-to-end reference outputs are sometimes
unavoidable. A requirement like "a system started with these initial conditions reaches
this final state" demands a specific expected value, not just an invariant.

For these cases, the reference data is generated by running the full stack with
known-good implementations and capturing the output. The key discipline is **bottom-up
regeneration**: when a partition changes, the regeneration process follows the layer
structure:

1. Verify that the changed partition passes its contract tests (layer N+1).
2. Regenerate compositor-level references at layer N using the updated partition.
3. Regenerate system-level references at layer 0 using the updated compositor output.

This ordering ensures that no reference is regenerated against a partition that hasn't
itself been verified. It preserves the diagnostic layering of the test pyramid: if step 1
fails, the problem is in the partition, not in the composition. If step 1 passes but
step 2 fails, the problem is in the composition.

Each generated reference records:

- The generation command (so it can be reproduced).
- The implementation version of each partition used.
- The contract version in effect at each layer.

This provenance makes reference files auditable. When a system test fails against its
reference, the recorded versions identify exactly which implementations produced the
reference, which helps determine whether the reference needs regeneration or the
implementation has a bug.

### Principle 4: Contract versioning bounds propagation

*Specified in FPA-039.*

The fractal partition pattern supports alternative implementations at every layer.
Third-party components, experimental implementations, and the defaults all coexist. When a contract changes, all of these alternatives must update — unless
the change is versioned.

Contract versioning scopes reference data propagation: each contract version carries its
own canonical inputs and expected output properties. When a contract evolves from version
N to version N+1:

- Implementations targeting version N continue to use version N's reference data.
  Their contract tests continue to pass. They are unaffected.
- Implementations targeting version N+1 use the new reference data.
  They must satisfy the new contract's properties.

This allows alternative implementations to migrate on their own schedule. A third-party
component can continue targeting contract version N while the default implementation
moves to version N+1. Both are testable, both are valid within their contract version,
and neither's reference data is invalidated by the other's changes.

The version boundary is the propagation boundary. A contract change does not propagate
to implementations that haven't adopted the new version.

## How the principles compose

The four principles form a layered defense against reference data staleness:

| Layer          | What tests assert              | Reference data source          |
|----------------|--------------------------------|--------------------------------|
| Contract tests | Output properties (invariants) | Contract definition itself     |
| Compositor tests | Compositional properties     | Composition structure          |
| Compositor regression | Exact baselines (when needed) | Generated from implementations |
| System tests   | End-to-end expected outputs    | Generated bottom-up            |

At the contract level, there is nothing to go stale — the reference data *is* the
contract. At the compositor level, compositional properties are structural, not
numerical, so they don't drift. Where exact values are needed (compositor regression,
system tests), they are generated mechanically with recorded provenance and regenerated
bottom-up when implementations change.

Contract versioning adds a horizontal boundary: changes to a contract's reference data
propagate only to implementations targeting the new version, not to all alternatives
simultaneously.

The result is a reference data architecture where:

- A contract change requires updating reference data in exactly one place (the contract).
- A partition implementation change requires no manual reference data updates at any
  layer (contract tests assert properties, compositor tests assert structural invariants,
  exact baselines are regenerated mechanically).
- Alternative implementations are isolated from each other's reference data by contract
  versioning.

## Design choices and tradeoffs

### Why properties over golden files

Golden files are appealing because they are easy to create — run the code, capture the
output, commit the file. The cost comes later: every behavioral change, even a correct
one, requires updating every golden file in the dependency chain. In a fractal system,
the dependency chain spans every layer. Property-based assertions require more thought
upfront — the test author must identify what the contract actually guarantees, not just
what the current implementation happens to produce — but they are stable across the
implementation changes that the fractal partition pattern is designed to encourage.

The tradeoff is real: properties are harder to write and may miss regressions that exact
comparisons would catch. This is why the architecture allows generated baselines as a
complement, not a replacement. Properties catch contract violations. Generated baselines
catch unintended behavioral changes. Both serve a purpose; the architecture keeps them
separate so that one failing doesn't mask the other.

### Why bottom-up regeneration, not top-down

An alternative is top-down: regenerate system references first, then derive expected
compositor and contract outputs by decomposing the system reference. This fails because
it inverts the diagnostic layering. If the system reference is wrong, you don't know
whether the problem is in a partition, in the composition, or in the reference generation
itself. Bottom-up regeneration preserves the same diagnostic ordering as the test
pyramid: verify the leaves first, then verify that the leaves compose correctly, then
verify that the composed result meets system requirements.

### Why version-scoped reference data

An alternative is to require all implementations to target the latest contract version.
This is simpler but hostile to the ecosystem the fractal partition pattern creates.
Alternative implementations — third-party components, research prototypes, specialized
variants — often lag behind the latest contract. Requiring simultaneous migration forces
either breakage (alternatives fail until updated) or stagnation (contracts don't evolve
because too many alternatives would break). Version-scoped reference data allows the
contract to evolve while giving alternatives a stable target to test against.

The cost is maintaining reference data for multiple contract versions. This is bounded
by a deprecation policy (old versions are eventually retired) and mitigated by the fact
that canonical inputs and output properties are small artifacts — they are type instances
and invariant assertions, not large data files.
