//! FPA-001: Fractal Partition Pattern
//!
//! Verifies that the uniform contract/implementation/compositor structure exists.
//! - Partition trait defines init/step/shutdown/contribute_state/load_state
//! - Multiple implementations conform to the same trait
//! - The trait is usable as a trait object (Box<dyn Partition>)

use fpa_contract::{Partition, PartitionError};
use fpa_contract::test_support::{Counter, Accumulator};

/// The Partition trait exists and defines the expected lifecycle methods.
#[test]
fn partition_trait_has_lifecycle_methods() {
    let mut counter = Counter::new("test");

    // init -> step -> contribute_state -> shutdown is the lifecycle
    counter.init().unwrap();
    counter.step(1.0 / 60.0).unwrap();
    let _state = counter.contribute_state().unwrap();
    counter.shutdown().unwrap();
}

/// Multiple implementations conform to the same Partition trait.
#[test]
fn multiple_implementations_share_trait() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Counter::new("counter")),
        Box::new(Accumulator::new("accumulator")),
    ];

    // Both can be used through the trait interface
    for mut p in partitions {
        p.init().unwrap();
        p.step(1.0).unwrap();
        let _state = p.contribute_state().unwrap();
        p.shutdown().unwrap();
    }
}

/// Partition trait is object-safe and supports dynamic dispatch.
#[test]
fn partition_trait_is_object_safe() {
    fn run_partition(p: &mut dyn Partition) -> Result<(), PartitionError> {
        p.init()?;
        p.step(0.1)?;
        p.shutdown()?;
        Ok(())
    }

    let mut counter = Counter::new("c");
    let mut accum = Accumulator::new("a");

    run_partition(&mut counter).unwrap();
    run_partition(&mut accum).unwrap();
}

/// Each partition has an identity.
#[test]
fn partitions_have_identity() {
    let counter = Counter::new("my_counter");
    let accum = Accumulator::new("my_accum");

    assert_eq!(counter.id(), "my_counter");
    assert_eq!(accum.id(), "my_accum");
}
