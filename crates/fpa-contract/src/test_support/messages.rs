//! Typed message types for test partitions, declared in the contract crate (FPA-005).

use serde::{Deserialize, Serialize};

use crate::message::{DeliverySemantic, Message};

/// Output message from the Counter partition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterOutput {
    pub count: u64,
}

impl Message for CounterOutput {
    const NAME: &'static str = "CounterOutput";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}

/// Output message from the Accumulator partition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccumulatorOutput {
    pub total: f64,
}

impl Message for AccumulatorOutput {
    const NAME: &'static str = "AccumulatorOutput";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}

/// Output message from the Doubler partition (contract version 2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoublerOutput {
    pub value: f64,
}

impl Message for DoublerOutput {
    const NAME: &'static str = "DoublerOutput";
    const VERSION: u32 = 2;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}

/// Queued command — exercises ordered delivery (Document Editor pattern).
///
/// Partitions publish TestCommand to request actions. Queued delivery ensures
/// all commands are received in order with no silent drops (FPA-007).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestCommand {
    pub command: String,
    pub sequence: u64,
}

impl Message for TestCommand {
    const NAME: &'static str = "TestCommand";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::Queued;
}

/// Sensor reading — exercises LatestValue subscription (Controller pattern).
///
/// Partitions publish SensorReading for continuous state observation.
/// LatestValue delivery means subscribers see only the most recent reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SensorReading {
    pub value: f64,
    pub source: String,
}

impl Message for SensorReading {
    const NAME: &'static str = "SensorReading";
    const VERSION: u32 = 1;
    const DELIVERY: DeliverySemantic = DeliverySemantic::LatestValue;
}
