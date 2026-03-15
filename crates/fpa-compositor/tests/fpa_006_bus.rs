//! Tests for FPA-006: Bus-mediated transition requests.
//!
//! Verifies that a partition can publish a TransitionRequest on the bus
//! and the compositor processes it during Phase 3 of run_tick.

use std::sync::Arc;

use fpa_bus::{BusExt, InProcessBus};
use fpa_compositor::compositor::Compositor;
use fpa_compositor::state_machine::ExecutionState;
use fpa_contract::test_support::Counter;
use fpa_contract::{Partition, TransitionRequest};

/// Publishing TransitionRequest(Paused) on bus → compositor transitions to Paused.
#[test]
fn bus_mediated_transition_to_paused() {
    let bus: Arc<dyn fpa_bus::Bus> = Arc::new(InProcessBus::new("test-bus"));
    let partitions: Vec<Box<dyn Partition>> = vec![Box::new(Counter::new("a"))];
    let mut compositor = Compositor::new(partitions, Arc::clone(&bus));

    compositor.init().unwrap();
    assert_eq!(compositor.state(), ExecutionState::Running);

    // Simulate a partition publishing a transition request on the bus
    bus.publish(TransitionRequest {
        requested_by: "partition-a".to_string(),
        target_state: ExecutionState::Paused,
    });

    // Run tick — compositor should process the bus request in Phase 3
    compositor.run_tick(1.0).unwrap();

    assert_eq!(
        compositor.state(),
        ExecutionState::Paused,
        "compositor should have transitioned to Paused via bus-mediated request"
    );
}

/// Multiple bus-mediated requests are processed in order.
#[test]
fn bus_mediated_multiple_requests_processed_in_order() {
    let bus: Arc<dyn fpa_bus::Bus> = Arc::new(InProcessBus::new("test-bus"));
    let partitions: Vec<Box<dyn Partition>> = vec![Box::new(Counter::new("a"))];
    let mut compositor = Compositor::new(partitions, Arc::clone(&bus));

    compositor.init().unwrap();

    // Publish Paused then Running — final state should be Running
    bus.publish(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Paused,
    });
    bus.publish(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Running,
    });

    compositor.run_tick(1.0).unwrap();

    assert_eq!(
        compositor.state(),
        ExecutionState::Running,
        "after Paused then Running, compositor should end in Running"
    );
}

/// Partition publishes TransitionRequest during step() → DeferredBus queues it →
/// Phase 2 flush → Phase 3 processes it. This validates the production code path.
#[test]
fn partition_publishes_transition_request_during_step() {
    use fpa_bus::DeferredBus;
    use fpa_contract::PartitionError;

    /// A partition that publishes a TransitionRequest(Paused) on its first step.
    struct PauseRequester {
        id: String,
        bus: Arc<DeferredBus>,
        requested: bool,
    }

    impl Partition for PauseRequester {
        fn id(&self) -> &str {
            &self.id
        }
        fn init(&mut self) -> Result<(), PartitionError> {
            Ok(())
        }
        fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
            if !self.requested {
                self.requested = true;
                self.bus.publish(TransitionRequest {
                    requested_by: self.id.clone(),
                    target_state: ExecutionState::Paused,
                });
            }
            Ok(())
        }
        fn shutdown(&mut self) -> Result<(), PartitionError> {
            Ok(())
        }
        fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
            Ok(toml::Value::Table(toml::map::Map::new()))
        }
        fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> {
            Ok(())
        }
    }

    let inner_bus = Arc::new(InProcessBus::new("test-bus"));
    let deferred_bus = Arc::new(DeferredBus::new(inner_bus));

    let requester = PauseRequester {
        id: "requester".to_string(),
        bus: Arc::clone(&deferred_bus),
        requested: false,
    };

    let partitions: Vec<Box<dyn Partition>> = vec![Box::new(requester)];
    let mut compositor = Compositor::from_deferred_bus(partitions, deferred_bus);

    compositor.init().unwrap();
    assert_eq!(compositor.state(), ExecutionState::Running);

    // Run one tick: partition publishes during Phase 2 step, DeferredBus queues it,
    // end_deferred flushes, Phase 3 processes the request.
    compositor.run_tick(1.0).unwrap();

    assert_eq!(
        compositor.state(),
        ExecutionState::Paused,
        "compositor should have transitioned to Paused via DeferredBus pipeline"
    );
}

/// Invalid bus-mediated transition request returns error from run_tick.
#[test]
fn bus_mediated_invalid_transition_returns_error() {
    let bus: Arc<dyn fpa_bus::Bus> = Arc::new(InProcessBus::new("test-bus"));
    let partitions: Vec<Box<dyn Partition>> = vec![Box::new(Counter::new("a"))];
    let mut compositor = Compositor::new(partitions, Arc::clone(&bus));

    compositor.init().unwrap();

    // Running → Initializing is invalid
    bus.publish(TransitionRequest {
        requested_by: "test".to_string(),
        target_state: ExecutionState::Initializing,
    });

    let result = compositor.run_tick(1.0);
    assert!(
        result.is_err(),
        "invalid bus-mediated transition should cause run_tick to return error"
    );
}
