//! Registry helpers for test partitions.
//!
//! Re-exports `PartitionRegistry` from fpa-compositor and provides
//! `with_all_test_partitions()` which registers all 6 test partitions
//! (Counter, Accumulator, Doubler + Sensor, Follower, Recorder).

pub use fpa_compositor::compose::PartitionRegistry;

use crate::test_partitions::{Follower, Recorder, Sensor};

/// Create a registry with all test partitions: the 3 contract-level
/// partitions (Counter, Accumulator, Doubler) plus the 3 bus-aware
/// partitions (Sensor, Follower, Recorder).
///
/// Use this for system tests and reference generation that need
/// inter-partition communication.
pub fn with_all_test_partitions() -> PartitionRegistry {
    let mut reg = PartitionRegistry::with_test_partitions();

    reg.register(
        "Sensor",
        Box::new(|id, config, bus| {
            Ok(Box::new(Sensor::from_config(id, config, bus.clone())?))
        }),
    );
    reg.register(
        "Follower",
        Box::new(|id, config, bus| {
            Ok(Box::new(Follower::from_config(id, config, bus.clone())?))
        }),
    );
    reg.register(
        "Recorder",
        Box::new(|id, _config, bus| {
            Ok(Box::new(Recorder::new(id, bus.clone())))
        }),
    );

    reg
}
