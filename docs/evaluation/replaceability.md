# Phase 6 Track P: Replaceability & Isolation

Evaluation of FPA's drop-in replaceability guarantees and test isolation properties.

## 6P.1 — Swap Experiment Results

A `ScalingCounter` partition was implemented in the eval test file using only `fpa_contract`
types. It counts steps and applies a configurable scale factor, storing raw count and scale
in state for clean roundtrip.

| Impl            | LOC (measured) | Files touched | Compile errors | Contract pass | Compositor pass |
|-----------------|----------------|---------------|----------------|---------------|-----------------|
| Counter         | 70             | 1             | 0              | yes           | yes             |
| ScalingCounter  | 79             | 1             | 0              | yes           | yes             |

Key observations:

- ScalingCounter passes both the contract test lifecycle (`OutputProperties` assertions)
  and the compositional suite (delivery, conservation, ordering) with zero modifications
  to any existing source file.
- The `Partition` trait surface is sufficient: `init/step/shutdown/contribute_state/load_state`
  covers the full integration contract.
- No peer partition modules are imported. The implementation depends only on `fpa_contract`
  types (`Partition`, `PartitionError`, `toml::Value`).

## 6P.2 — Test Isolation Findings

### Peer-free verification

8 `fpa_contract/tests/fpa_*.rs` files were scanned at runtime. None contain imports
from `fpa_compositor`, `fpa_bus`, or `fpa_testkit`. Contract tests are self-contained
within their tier.

### Test pyramid counts

Counts include both `#[test]` and `#[tokio::test]` functions.

| Tier        | Test count |
|-------------|------------|
| Contract    | 59         |
| Bus         | 48         |
| Compositor  | 162        |
| System      | 45         |

The contract tier (59) exceeds the system tier (45), confirming the expected pyramid
shape. The compositor tier is the largest (162), reflecting its role as the primary
integration surface with many compositional property and cross-strategy tests.

## Analysis

**Replaceability is well-supported.** The `Partition` trait provides a clean contract
boundary. New implementations require:

1. A struct with domain state (~70-80 LOC for a minimal partition)
2. A `Partition` impl targeting the trait's six methods
3. No knowledge of peer partitions or compositor internals

**Test isolation is maintained.** Contract tests use only contract-crate types. The
tier boundaries are enforced by Cargo's dependency graph and validated by runtime
source scanning.

## Spec Implications

- The current trait surface is sufficient for drop-in replacement. No additional
  methods or marker traits were needed.
- `contribute_state`/`load_state` roundtrip is the critical contract property for
  replaceability — if an implementation satisfies this, it integrates correctly.
- The `OutputProperties` helpers in `test_support` make it straightforward to verify
  new implementations without writing custom assertions.
