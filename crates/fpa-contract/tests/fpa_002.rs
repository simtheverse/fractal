//! FPA-002: Partition Independence
//!
//! Verifies that substituting one partition implementation with another requires
//! no changes to peer partition source code.
//! - Both Counter and Accumulator implement Partition
//! - They can be swapped in a generic test harness without modification
//! - No partition imports from another partition's module

use fpa_contract::Partition;
use fpa_contract::test_support::{Counter, Accumulator, CanonicalInputs};

/// Generic test harness that works with any Partition implementation.
/// This validates FPA-002: substitution requires no peer changes.
fn run_partition_lifecycle(p: &mut dyn Partition, steps: usize) -> toml::Value {
    p.init().unwrap();
    for _ in 0..steps {
        p.step(CanonicalInputs::standard_dt()).unwrap();
    }
    let state = p.contribute_state().unwrap();
    p.shutdown().unwrap();
    state
}

/// Counter can be run through the generic harness.
#[test]
fn counter_runs_through_generic_harness() {
    let mut counter = Counter::new("counter");
    let state = run_partition_lifecycle(&mut counter, 10);

    // Assert output property (not exact value): count is positive after stepping
    let count = state.as_table().unwrap().get("count").unwrap().as_integer().unwrap();
    assert!(count > 0, "counter should have a positive count after stepping");
}

/// Accumulator can be run through the same generic harness — no changes needed.
#[test]
fn accumulator_substitutes_without_changes() {
    let mut accum = Accumulator::new("accum");
    let state = run_partition_lifecycle(&mut accum, 10);

    // Assert output property: total is positive after stepping with positive dt
    let total = state.as_table().unwrap().get("total").unwrap().as_float().unwrap();
    assert!(total > 0.0, "accumulator should have a positive total after stepping");
}

/// The generic harness accepts any impl Partition — compile-time verification.
#[test]
fn generic_function_accepts_any_partition() {
    fn exercise<P: Partition>(mut p: P) {
        p.init().unwrap();
        p.step(1.0).unwrap();
        let _ = p.contribute_state().unwrap();
        p.shutdown().unwrap();
    }

    exercise(Counter::new("c"));
    exercise(Accumulator::new("a"));
}
