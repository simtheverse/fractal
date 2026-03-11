# Testing in the Fractal Partition Pattern

## What problem does it solve?

A partitioned system needs a testing strategy that matches its decomposition structure.
The naive approach is a binary split: unit tests for small functions, integration tests
for the assembled system. But this misses the middle — the partition boundaries, the
contract conformance, the compositor assembly — and it doesn't scale with depth. When
layer 1 partitions decompose into layer 2 sub-partitions, a flat unit-vs-integration
distinction doesn't tell you which layer a test is verifying or which contract it is
exercising.

The fractal partition pattern resolves this by making the testing structure mirror the
partition structure. Just as the same structural primitives (contracts, compositors,
events, configuration) appear at every layer, the same testing primitives appear at
every layer. A test at layer N verifies a contract defined at layer N. The test can
run in isolation because the contract defines a boundary. The testing strategy does not
need to be redesigned as the system deepens — it propagates with the pattern.

## Three questions, three test types

The testing strategy is organized around three distinct questions. Each question
produces a test type with a defined scope and purpose.

- **Contract tests** answer: *does this implementation satisfy its contract?*
- **Compositor tests** answer: *does assembly at this layer produce correct interactions?*
- **System tests** answer: *does the system meet its requirements end-to-end?*

These are not points on a spectrum. They test different things — implementations,
wiring, and requirements — and a failure in one localizes to a different cause than
a failure in another.

## Testing follows the layers

### Contract tests

A **contract test** verifies that an implementation satisfies the contract defined at
its layer. At layer 0, a contract test instantiates a partition implementation, invokes
it through the traits defined in the contract crate, and asserts that the outputs conform
to the contract's behavioral requirements. At layer 1, a contract test does the same for
a sub-partition implementation against the traits defined in the partition's contract
module.

Contract tests are what most frameworks call "unit tests," but the term is misleading
here. A contract test for a partition might exercise a complete processing step
involving multiple sub-partitions composed together. It is a "unit" only in the sense
that it tests one unit of replaceability — one partition — in isolation from its peers
at the same layer. The unit is not a function or a module. The unit is whatever the
contract makes independently replaceable.

The key property is **isolation at the contract boundary**. A contract test for
partition A does not require running partitions B, C, or D. It supplies inputs that
conform to the contract's input types and asserts outputs that conform to the contract's
output types. The contract boundary is the isolation boundary.

At layer 1, the same structure applies. A contract test for sub-partition Alpha within
partition A does not require sub-partitions Beta or Gamma. It tests the Alpha
implementation against the Alpha contract trait in isolation.

### Compositor tests

A **compositor test** verifies that the compositor at a given layer correctly assembles
its partitions and that the assembled partitions interact correctly through their
contracts. At layer 0, this means the system compositor composes the
partitions and they exchange messages correctly through the bus. At layer 1, this means
a partition's internal compositor assembles its sub-partitions and they interact
correctly.

Compositor tests are what most frameworks call "integration tests." The distinction
matters because a compositor test has a defined scope: it tests one layer's assembly,
not the entire system. A layer 0 compositor test verifies that the partitions
communicate correctly through the contract crate's defined contracts and the bus. It
does not verify the internal correctness of each partition's sub-partitions — that is
the job of layer 1 contract tests.

This scope discipline prevents the common failure mode of integration tests: testing
everything at once, so that failures are difficult to localize and tests are slow and
brittle. A compositor test at layer N assumes that layer N+1 contract tests pass. It
tests composition, not implementation.

### System tests

A **system test** exercises the full stack within a system's scope boundary, from
configuration to final output, using the same entry point an operator or caller would
use. System tests verify end-to-end properties that emerge from the interaction of all
layers within that system: a run with specific initial conditions reaches a specific
final state; a scenario with scripted events produces the expected sequence of execution
state transitions; output from a complete run matches a reference.

System tests are distinguished from compositor tests by two properties. First, they
use the system's public entry points — configuration fragments, compositor API, CLI —
not internal assembly interfaces. A compositor test calls the compositor directly and
inspects the assembled result. A system test submits a configuration and observes the
output, the same way an operator would. Second, system tests trace to the system's own
specification requirements. Each system test verifies one or more requirements from
that system's SPECIFICATION.md (see FPA-SRS-000). Their purpose is not to catch bugs
in individual partitions — contract tests do that — but to verify that the
requirements are met when all partitions are composed together.

System tests are expensive and slow relative to contract tests. Their role in the test
pyramid is to be few in number, high in confidence, and directly tied to requirements
traceability.

### System tests are relative to system scope

The specification (FPA-SRS-000) states that system tests exist only at layer 0, and
this is true — but layer 0 is relative. Every system that applies the fractal partition
pattern has its own layer 0, and therefore its own system tests.

Consider a system called "inner-system" whose system tests exercise the full processing
stack: configuration in, results out. They trace to inner-system's FPA requirements.
When inner-system runs standalone, these are the outermost tests.

Now consider a larger pipeline that uses inner-system as a partition — one stage in a
pre-process/compute/post-process workflow. From the pipeline's perspective, inner-system
is a layer 1 partition. The pipeline has its own layer 0 compositor tests verifying that
pre-processing, computation, and post-processing interact correctly. And the pipeline
has its own system tests, exercising the full pipeline from input configuration to
published results, tracing to the pipeline's own requirements.

Inner-system's system tests do not disappear in this scenario. They remain
inner-system's system tests — they verify that inner-system meets its own requirements
at its own scope boundary. The pipeline's system tests verify something different: that
the pipeline meets the pipeline's requirements. These are different systems with
different layer 0 boundaries, and each has system tests appropriate to its scope.

This is the fractal property applied to testing. Just as a partition's internal structure
mirrors the system's structure, a partition's test suite mirrors the system's test suite.
A partition complex enough to have its own compositor and sub-partitions is complex enough
to have its own system tests verifying its own requirements through its own public API.
The three test types — contract, compositor, system — repeat at every scope boundary
where the pattern is applied.

### The relationship between test types

The three test types form a pyramid that mirrors the layer structure:

```
            ┌─────────────┐
            │ System tests│   Full stack within scope boundary
            │             │   Verify system requirements
            ├─────────────┤
            │ Compositor  │   One layer's assembly
            │   tests     │   Verify composition correctness
            ├─────────────┤
            │  Contract   │   One partition in isolation
            │   tests     │   Verify contract conformance
            └─────────────┘
```

There are two equivalent ways to describe where a test belongs. You can describe it
from above — "layer 0 contract tests verify the layer 1 partitions" — or from within —
"these contract tests live inside partition A's test suite." These are the
same tests seen from different vantage points. The "from above" view emphasizes what
the tests verify (the next layer down). The "from within" view emphasizes where the
tests live (inside the partition that owns the contract). Both are useful. The summary
below uses the "from above" view because it makes the layer relationships explicit.

At each layer N within a given system:
- **Contract tests** verify that each partition at layer N+1 satisfies its contract.
- **Compositor tests** verify that the compositor at layer N correctly assembles layer
  N+1 partitions.
- **System tests** exist at that system's layer 0, verifying end-to-end properties
  through the system's public interface.

Because the fractal pattern allows arbitrary depth, the contract/compositor pair
repeats at every layer. Layer 1 contract tests verify sub-partition implementations.
Layer 1 compositor tests verify that the partition's internal compositor assembles
sub-partitions correctly. Layer 2 contract tests verify sub-sub-partition
implementations, and so on.

When a partition is itself a system — complex enough to have its own specification,
its own compositor, and its own public API — it carries its own test pyramid. A system
has contract tests (verifying each of its partitions), compositor tests (verifying the
compositor assembles them correctly), and system tests (verifying the system's FPA
requirements end-to-end). A partition, if complex enough, could have contract tests
(verifying each sub-partition), compositor tests (verifying sub-partition assembly),
and its own system tests (verifying partition-level requirements through the partition's
public interface). The structure recurses as deep as the domain warrants.

## What emerges from the pattern

### Test isolation matches partition isolation

A contract test's isolation boundary is the same as the partition's replaceability
boundary. If a partition can be replaced without modifying its peers, a contract test
can run without instantiating its peers. The structural guarantee that makes the system
modular is the same guarantee that makes the tests fast and independent.

This is not a coincidence — it is a direct consequence of the contract-centric design.
The contract defines both what a replacement must provide (for modularity) and what a
test must assert (for correctness). A single artifact serves both purposes.

### Alternative implementations are testable by construction

When a contributor provides an alternative implementation of a partition — a third-party
plugin, a team's custom sub-partition — the contract tests for that partition's contract
apply unchanged. The test suite for a contract verifies any conforming implementation,
not just the default one. If the alternative passes the contract tests, it is a valid
replacement. If it doesn't, the failures identify exactly which contract obligations
are unmet.

This eliminates the common problem of alternative implementations that "mostly work"
but fail in edge cases that aren't covered by ad-hoc tests. The contract tests are
comprehensive with respect to the contract, and the contract is the complete definition
of what an implementation must do.

### Transport independence is testable

The requirement that all transport modes produce identical results (FPA-004) becomes a
compositor test parameterized over transport mode. The same scenario runs with each
supported transport — in-process, async, networked — and the final state is compared.
This test exists at layer 0 because transport is a layer 0 concern. Layer 1 tests are
unaware of transport because layer 1 partitions are unaware of transport.

### Fidelity tiers are testable without special machinery

A fidelity tier is a named composition fragment that selects a set of sub-partition
implementations. Testing a fidelity tier is a compositor test at layer 1: compose the
sub-partitions selected by the tier, run a reference scenario, and compare outputs
against the expected reference for that fidelity level. No fidelity-specific test
infrastructure is needed — it is the same compositor test with a different composition
fragment input.

### Snapshot round-trip is testable

State snapshot dump and load (FPA-023) is testable as a compositor test: run a scenario
to time T, dump state, load the dumped state into a fresh instance, continue to time
T+N, and compare the result with a continuous run from 0 to T+N. Because state snapshots
are composition fragments, the dump output is human-readable and diffable, making test
failures diagnosable.

### Test reference data follows contract boundaries

The testing structure creates a reference data management concern: tests at every layer
need inputs to supply and outputs to expect, and a contract change at one layer can
propagate upward through compositor and system test references. The architecture that
addresses this is specified in FPA-CON-000 (FPA-036 through FPA-039) and explained in
full in
[Test Reference Data in the Fractal Partition Pattern](test-reference-data-in-the-fractal-partition-pattern.md).

The core principle is that reference data ownership follows the same boundaries as
everything else in the pattern — the contract boundary. Contract tests assert output
properties defined by the contract itself, not exact values from a golden file.
Compositor tests assert compositional properties (message delivery, conservation,
ordering) that are stable across implementation changes. Where exact references are
unavoidable — system tests tied to specific requirements — they are generated
mechanically with recorded provenance and regenerated bottom-up following the layer
structure. Contract versioning bounds the propagation of reference data changes so that
alternative implementations are not invalidated by contract evolution they have not yet
adopted.

## Design choices and tradeoffs

### Why contract tests, not mock-based unit tests

The conventional approach to testing modular systems is to mock dependencies and test
each module in isolation against its mocks. The fractal partition pattern avoids mocks
in favor of contract tests for a specific reason: the contract is already a precisely
defined interface. A mock is an ad-hoc reimplementation of that interface for testing
purposes. If the mock diverges from the real contract behavior, the tests pass but the
integration fails.

Contract tests avoid this by testing against the contract itself, not against a mock of
it. The test supplies real inputs and asserts real outputs as defined by the contract
types. Where a test needs to supply data that would normally come from another partition,
it constructs instances of the contract's input types directly — this is not a mock, it
is using the contract's type system as intended.

There is a cost: contract tests for complex partitions may require nontrivial input
construction. A contract test for partition A must supply a plausible input message
matching partition B's output type. This is more work than mocking a provider trait.
The tradeoff is that the test exercises the actual contract boundary and cannot silently
diverge from the real interface.

### Why compositor tests have a defined scope

It would be simpler to have two levels — unit tests and end-to-end tests — without the
intermediate compositor test. The problem is that end-to-end tests are slow, brittle,
and hard to debug. When an end-to-end test fails, the failure could be in any partition,
any sub-partition, or in the composition itself. Localizing the failure requires
investigation that the test structure doesn't help with.

Compositor tests localize composition failures to a specific layer. If a layer 0
compositor test fails but all layer 1 contract tests pass, the problem is in the
inter-partition communication or the compositor's assembly logic, not in any
partition's internal implementation. This diagnostic property is worth the additional
test category.

### Why system tests trace to requirements

System tests could be organized by scenario, by workload type, or by feature. Organizing
them by requirement — each system test traces to one or more requirement identifiers from
the system's own specification — has a specific benefit: coverage analysis is trivial. If
every requirement has at least one system test, and all system tests pass, the system
satisfies its specification. If a requirement has no system test, the gap is visible in
the traceability matrix.

This traceability applies at every scope where system tests exist. A system's tests
trace to that system's FPA requirements. If a partition maintains its own system tests,
they trace to that partition's requirements. A larger pipeline that embeds the system as
a partition would have system tests tracing to the pipeline's own requirement
identifiers. The discipline is the same; the scope boundary determines which
requirements are relevant.
