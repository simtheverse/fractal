//! Bus-aware test partitions for inter-partition communication tests.
//!
//! These partitions exercise the bus communication patterns from the
//! reference domain applications (FPA REFERENCE_DOMAINS.md):
//! - Sensor: config-driven publisher (industrial controller, flight sim)
//! - Follower: subscriber that publishes commands on threshold (kiosk, controller)
//! - Recorder: pure consumer that logs everything (data logger, renderer)
//!
//! They live in fpa-testkit (not fpa-contract) because they depend on
//! fpa-bus for publish/subscribe — fpa-contract cannot have that dependency.

use std::sync::Arc;

use fpa_bus::{Bus, BusExt, BusReader, TypedReader};
use fpa_contract::error::PartitionError;
use fpa_contract::partition::Partition;
use fpa_contract::test_support::{SensorReading, TestCommand};
use fpa_contract::SharedContext;

/// Config-driven, bus-publishing partition.
///
/// Reads `scale` and `offset` from TOML config. Each step, publishes a
/// `SensorReading` on the bus with value = step_count * scale + offset.
/// Maintains a history buffer for complex/nested state testing.
///
/// Mirrors: SensorInput (industrial controller), Environment (flight sim).
pub struct Sensor {
    id: String,
    bus: Arc<dyn Bus>,
    scale: f64,
    offset: f64,
    step_count: i64,
    history: Vec<(i64, f64)>,
    initialized: bool,
}

impl Sensor {
    pub fn new(id: impl Into<String>, bus: Arc<dyn Bus>, scale: f64, offset: f64) -> Self {
        Self {
            id: id.into(),
            bus,
            scale,
            offset,
            step_count: 0,
            history: Vec::new(),
            initialized: false,
        }
    }

    /// Create from TOML config table. Reads `scale` and `offset` keys.
    pub fn from_config(
        id: impl Into<String>,
        config: &toml::Value,
        bus: Arc<dyn Bus>,
    ) -> Result<Self, PartitionError> {
        let id = id.into();
        let scale = config
            .get("scale")
            .and_then(|v| v.as_float())
            .unwrap_or(1.0);
        let offset = config
            .get("offset")
            .and_then(|v| v.as_float())
            .unwrap_or(0.0);
        Ok(Self::new(id, bus, scale, offset))
    }
}

impl Partition for Sensor {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        self.initialized = true;
        Ok(())
    }

    fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
        if !self.initialized {
            return Err(PartitionError::new(&self.id, "step", "not initialized"));
        }
        self.step_count += 1;
        let value = self.step_count as f64 * self.scale + self.offset;
        self.history.push((self.step_count, value));

        self.bus.publish(SensorReading {
            value,
            source: self.id.clone(),
        });

        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        self.initialized = false;
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        let mut table = toml::map::Map::new();
        table.insert(
            "scale".to_string(),
            toml::Value::Float(self.scale),
        );
        table.insert(
            "offset".to_string(),
            toml::Value::Float(self.offset),
        );
        table.insert(
            "step_count".to_string(),
            toml::Value::Integer(self.step_count),
        );

        // History as array of tables — exercises complex/nested state.
        let history_arr: Vec<toml::Value> = self
            .history
            .iter()
            .map(|(tick, value)| {
                let mut entry = toml::map::Map::new();
                entry.insert("tick".to_string(), toml::Value::Integer(*tick));
                entry.insert("value".to_string(), toml::Value::Float(*value));
                toml::Value::Table(entry)
            })
            .collect();
        table.insert(
            "history".to_string(),
            toml::Value::Array(history_arr),
        );

        Ok(toml::Value::Table(table))
    }

    fn load_state(&mut self, state: toml::Value) -> Result<(), PartitionError> {
        let table = state.as_table().ok_or_else(|| {
            PartitionError::new(&self.id, "load_state", "expected table")
        })?;

        let err = |field: &str| {
            PartitionError::new(&self.id, "load_state", format!("missing or invalid field '{}'", field))
        };

        self.scale = table.get("scale").and_then(|v| v.as_float()).ok_or_else(|| err("scale"))?;
        self.offset = table.get("offset").and_then(|v| v.as_float()).ok_or_else(|| err("offset"))?;
        let step_count = table.get("step_count").and_then(|v| v.as_integer()).ok_or_else(|| err("step_count"))?;
        if step_count < 0 {
            return Err(PartitionError::new(&self.id, "load_state", "step_count is negative"));
        }
        self.step_count = step_count;

        let history_arr = table.get("history").and_then(|v| v.as_array()).ok_or_else(|| err("history"))?;
        self.history.clear();
        for entry in history_arr {
            let tick = entry.get("tick").and_then(|v| v.as_integer()).ok_or_else(|| err("history[].tick"))?;
            if tick < 0 {
                return Err(PartitionError::new(&self.id, "load_state", "history[].tick is negative"));
            }
            let value = entry.get("value").and_then(|v| v.as_float()).ok_or_else(|| err("history[].value"))?;
            self.history.push((tick, value));
        }

        Ok(())
    }
}

/// Subscribes to SensorReading (LatestValue), publishes TestCommand (Queued)
/// each tick the sensor value is at or above a configurable threshold.
///
/// The core "read input, produce output" pattern from the reference domains.
///
/// Mirrors: SafetyInterlock (controller), OrderBuilder (kiosk), ControlLaw (flight sim).
pub struct Follower {
    id: String,
    bus: Arc<dyn Bus>,
    sensor_reader: TypedReader<SensorReading>,
    threshold: f64,
    last_reading: f64,
    commands_sent: i64,
    initialized: bool,
}

impl Follower {
    pub fn new(id: impl Into<String>, bus: Arc<dyn Bus>, threshold: f64) -> Self {
        let sensor_reader = bus.subscribe::<SensorReading>();
        Self {
            id: id.into(),
            bus,
            sensor_reader,
            threshold,
            last_reading: 0.0,
            commands_sent: 0,
            initialized: false,
        }
    }

    /// Create from TOML config table. Reads `threshold` key.
    pub fn from_config(
        id: impl Into<String>,
        config: &toml::Value,
        bus: Arc<dyn Bus>,
    ) -> Result<Self, PartitionError> {
        let id = id.into();
        let threshold = config
            .get("threshold")
            .and_then(|v| v.as_float())
            .unwrap_or(5.0);
        Ok(Self::new(id, bus, threshold))
    }
}

impl Partition for Follower {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        self.initialized = true;
        Ok(())
    }

    fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
        if !self.initialized {
            return Err(PartitionError::new(&self.id, "step", "not initialized"));
        }

        // Read latest sensor value (LatestValue semantic — only most recent).
        if let Some(reading) = self.sensor_reader.read() {
            self.last_reading = reading.value;

            // Publish a command each tick the value is at or above threshold.
            if reading.value >= self.threshold {
                self.commands_sent += 1;
                self.bus.publish(TestCommand {
                    command: format!("threshold_crossed:{}", reading.value),
                    sequence: self.commands_sent as u64,
                });
            }
        }

        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        self.initialized = false;
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        let mut table = toml::map::Map::new();
        table.insert(
            "last_reading".to_string(),
            toml::Value::Float(self.last_reading),
        );
        table.insert(
            "commands_sent".to_string(),
            toml::Value::Integer(self.commands_sent),
        );
        table.insert(
            "threshold".to_string(),
            toml::Value::Float(self.threshold),
        );
        Ok(toml::Value::Table(table))
    }

    fn load_state(&mut self, state: toml::Value) -> Result<(), PartitionError> {
        let table = state.as_table().ok_or_else(|| {
            PartitionError::new(&self.id, "load_state", "expected table")
        })?;

        let err = |field: &str| {
            PartitionError::new(&self.id, "load_state", format!("missing or invalid field '{}'", field))
        };

        self.last_reading = table.get("last_reading").and_then(|v| v.as_float()).ok_or_else(|| err("last_reading"))?;
        let commands_sent = table.get("commands_sent").and_then(|v| v.as_integer()).ok_or_else(|| err("commands_sent"))?;
        if commands_sent < 0 {
            return Err(PartitionError::new(&self.id, "load_state", "commands_sent is negative"));
        }
        self.commands_sent = commands_sent;
        self.threshold = table.get("threshold").and_then(|v| v.as_float()).ok_or_else(|| err("threshold"))?;

        Ok(())
    }
}

/// Pure consumer — subscribes to SharedContext and TestCommand, logs
/// everything, publishes nothing.
///
/// The DataLogger / Renderer pattern: observes all inter-partition
/// communication without producing side effects.
///
/// Mirrors: DataLogger (controller), Renderer (document editor), DataRecorder (flight sim).
pub struct Recorder {
    id: String,
    context_reader: TypedReader<SharedContext>,
    command_reader: TypedReader<TestCommand>,
    entries_logged: i64,
    commands_received: i64,
    last_tick_seen: i64,
    initialized: bool,
}

impl Recorder {
    pub fn new(id: impl Into<String>, bus: Arc<dyn Bus>) -> Self {
        let context_reader = bus.subscribe::<SharedContext>();
        let command_reader = bus.subscribe::<TestCommand>();
        Self {
            id: id.into(),
            context_reader,
            command_reader,
            entries_logged: 0,
            commands_received: 0,
            last_tick_seen: 0,
            initialized: false,
        }
    }

    /// Create from TOML config table (no config parameters needed).
    pub fn from_config(
        id: impl Into<String>,
        _config: &toml::Value,
        bus: Arc<dyn Bus>,
    ) -> Result<Self, PartitionError> {
        Ok(Self::new(id, bus))
    }
}

impl Partition for Recorder {
    fn id(&self) -> &str {
        &self.id
    }

    fn init(&mut self) -> Result<(), PartitionError> {
        self.initialized = true;
        Ok(())
    }

    fn step(&mut self, _dt: f64) -> Result<(), PartitionError> {
        if !self.initialized {
            return Err(PartitionError::new(&self.id, "step", "not initialized"));
        }

        // Consume SharedContext (LatestValue).
        if let Some(ctx) = self.context_reader.read() {
            self.last_tick_seen = i64::try_from(ctx.tick).map_err(|_| {
                PartitionError::new(&self.id, "step", "tick exceeds i64::MAX")
            })?;
            self.entries_logged += 1;
        }

        // Consume all queued commands (Queued semantic — drain all).
        let commands = self.command_reader.read_all();
        self.commands_received += commands.len() as i64;

        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        self.initialized = false;
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        let mut table = toml::map::Map::new();
        table.insert(
            "entries_logged".to_string(),
            toml::Value::Integer(self.entries_logged),
        );
        table.insert(
            "commands_received".to_string(),
            toml::Value::Integer(self.commands_received),
        );
        table.insert(
            "last_tick_seen".to_string(),
            toml::Value::Integer(self.last_tick_seen),
        );
        Ok(toml::Value::Table(table))
    }

    fn load_state(&mut self, state: toml::Value) -> Result<(), PartitionError> {
        let table = state.as_table().ok_or_else(|| {
            PartitionError::new(&self.id, "load_state", "expected table")
        })?;

        let err = |field: &str| {
            PartitionError::new(&self.id, "load_state", format!("missing or invalid field '{}'", field))
        };

        let entries_logged = table.get("entries_logged").and_then(|v| v.as_integer()).ok_or_else(|| err("entries_logged"))?;
        if entries_logged < 0 {
            return Err(PartitionError::new(&self.id, "load_state", "entries_logged is negative"));
        }
        self.entries_logged = entries_logged;
        let commands_received = table.get("commands_received").and_then(|v| v.as_integer()).ok_or_else(|| err("commands_received"))?;
        if commands_received < 0 {
            return Err(PartitionError::new(&self.id, "load_state", "commands_received is negative"));
        }
        self.commands_received = commands_received;
        let last_tick_seen = table.get("last_tick_seen").and_then(|v| v.as_integer()).ok_or_else(|| err("last_tick_seen"))?;
        if last_tick_seen < 0 {
            return Err(PartitionError::new(&self.id, "load_state", "last_tick_seen is negative"));
        }
        self.last_tick_seen = last_tick_seen;

        Ok(())
    }
}
