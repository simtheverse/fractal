# FPA Prototype

## What this is

This is a research prototype of the **Fractal Partition Architecture (FPA)** — a
domain-agnostic architecture where systems decompose into layers of partitions with
uniform structural primitives. The prototype exists to stress-test the FPA specification
(`docs/design/SPECIFICATION.md`) and conventions (`docs/design/CONVENTIONS.md`), surface
findings against the spec, and improve it through implementation experience.

This prototype is also intended to bootstrap real FPA applications. Code quality, API design,
and architectural decisions should be production-grade, not throwaway.

## Relationship to the spec

The spec (FPA-SRS-000) is the authority. The prototype validates spec claims and feeds
findings back into the spec. When the implementation reveals a tension or gap:

1. Surface the finding clearly — don't hide it with workarounds
2. Update the spec if the finding is conclusive
3. Defer to a future phase if more evidence is needed

Every decision in the prototype should improve the specification, not just make the
prototype work. Prefer honest implementations that expose architectural tensions over
hacks that make tests pass.

## Crate structure

- **fpa-contract** — Core `Partition` trait, `Message` trait, `SharedContext`,
  `StateContribution`, error types, and test support (Counter, Accumulator, Doubler).
  No dependencies on other FPA crates.
- **fpa-bus** — `Bus` trait (object-safe), `BusExt` typed extension, `InProcessBus`,
  `AsyncBus`, `NetworkBus`. Transport selection is a compositor concern.
- **fpa-compositor** — `Compositor` (lock-step), `SupervisoryCompositor` (async tasks),
  double buffer, state machine, fault handling, direct signals, multi-rate scheduling,
  event engine integration.
- **fpa-config** — TOML composition fragment parsing, deep merge, extends chains,
  named fragment registry.
- **fpa-events** — Event engine: time-triggered and condition-triggered events,
  arm/disarm lifecycle, signal-based evaluation.
- **fpa-testkit** — Documentation structure validation, requirement traceability tests.

## Key architectural concepts

- **Partition trait** is strategy-neutral: `init/step/shutdown/contribute_state/load_state`.
  Both compositors implement it, enabling fractal nesting.
- **Box<dyn Bus>** for runtime transport selection. Partitions never know which transport
  is in use.
- **StateContribution** envelope wraps all `contribute_state()` output with freshness
  metadata (`state`, `fresh`, `age_ms`). Defined in fpa-contract.
- **Synchronous shutdown is a signal, not a confirmation** under supervisory coordination
  (FPA-009). `async_shutdown()` confirms. The spec documents this distinction.
- **ContractVersion** is a closed enum. Adding a version produces compiler errors at
  every site that needs version-specific data.

## Research plan

`PLAN.md` tracks all phases. Phases 0-4 are complete. Current state:
- 282 tests across 6 crates
- Phases 5-7 remain (network serialization, system tests, determinism/replaceability
  evaluation, synthesis)

## Development philosophy
- When considering solutions, think critically and challenge assumptions, including the specification (the prototype aims to inform the spec after all.) 
  - Anything is up for grabs, but the best, most effective, and most sustainable solution should be selected.
- Simple is better than complex.
- Treat the root cause, not the symptom.
- For FPA to be successful, the prototype must be of the highest quality and a scalable bootstrap for FPA applications.
- Uphold the FPA principles in all solutions- fractality, symmetry, drop-in replacability, runtime configurability, etc.
- Dare to address core architectural tensions and design instead of avoiding or working around them.

## Testing discipline

- Tests live in `crates/<crate>/tests/fpa_NNN.rs`, named after the requirement they verify
- Contract tests assert output properties, not exact values (FPA-036)
- Compositor tests assert compositional properties (delivery, conservation, ordering)
  that hold regardless of partition implementation (FPA-037)
- Canonical inputs and tolerances live in fpa-contract test support, scoped by version
- Tests must comply with spec behavior. Do not change tests without considering how the spec can be improved first

## Open feedback files

- `docs/feedback/FPA-004-network.md` — NetworkBus serialization gap (deferred to Phase 5 Track J2)
- `docs/feedback/FPA-030.md` — Diataxis documentation structure (deferred to Phase 7)

## Pull requests

- Do not include "Generated with Claude Code" or similar attribution lines in PR bodies
- To edit a PR after creation, use the REST API (not `gh pr edit`, which fails on
  this repo due to a Projects Classic deprecation error):
  `gh api repos/simtheverse/fractal/pulls/N --method PATCH -f body="..."`
- Create PRs with `gh pr create --title "..." --body "..."`

## PR review threads

To resolve review threads via `gh`, use the GraphQL API:

1. Get thread IDs: `gh api graphql -f query='{ repository(owner: "simtheverse", name: "fractal") { pullRequest(number: N) { reviewThreads(first: 20) { nodes { id isResolved comments(first: 1) { nodes { body } } } } } } }'`
2. Reply with `gh api repos/simtheverse/fractal/pulls/N/comments --method POST -f body="..." -F in_reply_to=COMMENT_ID`
3. Resolve with `gh api graphql -f query='mutation { resolveReviewThread(input: {threadId: "PRRT_..."}) { thread { isResolved } } }'`

## Commit style

- No co-authored-by lines
- Commit messages: imperative, focused on why not what
