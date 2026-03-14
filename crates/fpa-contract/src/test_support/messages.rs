//! Typed message types for test partitions, declared in the contract crate (FPA-005).

use crate::message::{DeliverySemantic, Message};

/// Output message from the Counter partition.
#[derive(Debug, Clone, PartialEq)]
pub struct CounterOutput {
    pub count: u64,
}

impl Message for CounterOutput {
    const NAME: &'static str = "CounterOutput";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}

/// Output message from the Accumulator partition.
#[derive(Debug, Clone, PartialEq)]
pub struct AccumulatorOutput {
    pub total: f64,
}

impl Message for AccumulatorOutput {
    const NAME: &'static str = "AccumulatorOutput";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}

/// Output message from the Doubler partition (contract version 2).
#[derive(Debug, Clone, PartialEq)]
pub struct DoublerOutput {
    pub value: f64,
}

impl Message for DoublerOutput {
    const NAME: &'static str = "DoublerOutput";
    const VERSION: u32 = 2;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}
