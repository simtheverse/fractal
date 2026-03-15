//! FPA-005: Typed Message Contracts
//!
//! Verifies that all inter-partition data uses named, versioned message types
//! declared in the contract crate.

use fpa_contract::{Message, DeliverySemantic};
use fpa_contract::test_support::{CounterOutput, AccumulatorOutput};

/// Messages have names.
#[test]
fn messages_are_named() {
    assert_eq!(CounterOutput::NAME, "CounterOutput");
    assert_eq!(AccumulatorOutput::NAME, "AccumulatorOutput");
}

/// Messages have version numbers.
#[test]
fn messages_are_versioned() {
    assert_eq!(CounterOutput::VERSION, 1);
    assert_eq!(AccumulatorOutput::VERSION, 1);
}

/// Messages declare their delivery semantic.
#[test]
fn messages_declare_delivery_semantic() {
    assert_eq!(CounterOutput::DELIVERY, DeliverySemantic::LatestValue);
    assert_eq!(AccumulatorOutput::DELIVERY, DeliverySemantic::LatestValue);
}

/// Messages are concrete typed structs, not untyped buffers.
#[test]
fn messages_are_statically_typed() {
    let output = CounterOutput { count: 42 };
    // The field is statically typed — compiler enforces this.
    let _count: u64 = output.count;

    let output = AccumulatorOutput { total: 3.15 };
    let _total: f64 = output.total;
}

/// Messages implement Clone (required for bus distribution).
#[test]
fn messages_are_cloneable() {
    let output = CounterOutput { count: 1 };
    let cloned = output.clone();
    assert_eq!(output, cloned);
}
