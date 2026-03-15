// DeferredBus unit tests — direct verification of deferred/flush behavior
// without compositor involvement.
//
// Traces to: FPA-014 (intra-tick isolation), FPA-007 (delivery semantics).

use std::sync::Arc;

use fpa_bus::{Bus, BusExt, BusReader, DeferredBus, InProcessBus, Transport};
use fpa_contract::test_support::{SensorReading, TestCommand};

/// Publishing while deferred queues messages — subscriber sees nothing until flush.
#[test]
fn deferred_mode_queues_messages() {
    let inner = Arc::new(InProcessBus::new("test"));
    let deferred = DeferredBus::new(inner);
    let bus: &dyn Bus = &deferred;
    let mut reader = bus.subscribe::<SensorReading>();

    deferred.set_deferred(true);
    bus.publish(SensorReading {
        value: 42.0,
        source: "s".into(),
    });

    assert!(reader.read().is_none(), "message should be queued, not delivered");
}

/// Publishing while not deferred passes through immediately.
#[test]
fn non_deferred_mode_passes_through() {
    let inner = Arc::new(InProcessBus::new("test"));
    let deferred = DeferredBus::new(inner);
    let bus: &dyn Bus = &deferred;
    let mut reader = bus.subscribe::<SensorReading>();

    bus.publish(SensorReading {
        value: 1.0,
        source: "s".into(),
    });

    let msg = reader.read().expect("message should pass through immediately");
    assert!((msg.value - 1.0).abs() < 1e-12);
}

/// Flush delivers queued messages in publish order.
#[test]
fn flush_delivers_in_publish_order() {
    let inner = Arc::new(InProcessBus::new("test"));
    let deferred = DeferredBus::new(inner);
    let bus: &dyn Bus = &deferred;
    let mut reader = bus.subscribe::<TestCommand>();

    deferred.set_deferred(true);
    for i in 1..=3 {
        bus.publish(TestCommand {
            command: format!("cmd_{i}"),
            sequence: i,
        });
    }

    assert!(reader.read().is_none(), "nothing before flush");

    deferred.flush();

    let commands = reader.read_all();
    assert_eq!(commands.len(), 3);
    for (i, cmd) in commands.iter().enumerate() {
        assert_eq!(cmd.sequence, (i + 1) as u64, "publish order preserved");
        assert_eq!(cmd.command, format!("cmd_{}", i + 1));
    }
}

/// Double flush doesn't duplicate messages.
#[test]
fn flush_is_idempotent() {
    let inner = Arc::new(InProcessBus::new("test"));
    let deferred = DeferredBus::new(inner);
    let bus: &dyn Bus = &deferred;
    let mut reader = bus.subscribe::<TestCommand>();

    deferred.set_deferred(true);
    bus.publish(TestCommand {
        command: "once".into(),
        sequence: 1,
    });

    deferred.flush();
    deferred.flush(); // second flush should be a no-op

    let commands = reader.read_all();
    assert_eq!(commands.len(), 1, "message delivered exactly once despite double flush");
}

/// LatestValue subscriber sees only the last value after flush.
#[test]
fn latest_value_in_deferred_mode() {
    let inner = Arc::new(InProcessBus::new("test"));
    let deferred = DeferredBus::new(inner);
    let bus: &dyn Bus = &deferred;
    let mut reader = bus.subscribe::<SensorReading>();

    deferred.set_deferred(true);
    for i in 1..=3 {
        bus.publish(SensorReading {
            value: i as f64,
            source: "s".into(),
        });
    }

    deferred.flush();

    // LatestValue: only the last published value is visible
    let msg = reader.read().expect("should have a value after flush");
    assert!((msg.value - 3.0).abs() < 1e-12, "should be the last published value");
    assert!(reader.read().is_none(), "no more values after latest consumed");
}

/// DeferredBus delegates transport() and id() to the inner bus.
#[test]
fn transport_and_id_delegate_to_inner() {
    let inner = Arc::new(InProcessBus::new("my-bus-id"));
    let deferred = DeferredBus::new(inner);

    assert_eq!(deferred.transport(), Transport::InProcess);
    assert_eq!(deferred.id(), "my-bus-id");
}

/// Subscribers created on DeferredBus receive messages after flush.
#[test]
fn subscribe_goes_to_inner() {
    let inner = Arc::new(InProcessBus::new("test"));
    let deferred = DeferredBus::new(inner);
    let bus: &dyn Bus = &deferred;

    // Subscribe via deferred wrapper
    let mut reader = bus.subscribe::<SensorReading>();

    // Publish in deferred mode, flush, verify subscriber receives
    deferred.set_deferred(true);
    bus.publish(SensorReading {
        value: 99.0,
        source: "src".into(),
    });
    deferred.set_deferred(false);
    deferred.flush();

    let msg = reader.read().expect("subscriber on deferred bus should receive flushed messages");
    assert!((msg.value - 99.0).abs() < 1e-12);
}
