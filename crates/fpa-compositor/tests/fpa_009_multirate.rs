//! Tests for FPA-009: Multi-rate execution.
//!
//! Verifies that partitions can run at different rates within the same compositor.
//! A partition with rate 4 steps 4 times per outer tick (with dt/4 each sub-step).

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_compositor::multi_rate::RateConfig;
use fpa_contract::test_support::{Accumulator, Counter};
use fpa_contract::{Partition, PartitionError, StateContribution};

/// Fast partition (rate 4) steps 4x per tick, slow partition (rate 1) steps 1x.
#[test]
fn fast_partition_steps_4x_per_slow_partition_1x() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("fast")),
        Box::new(Counter::new("slow")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    let mut rate_config = RateConfig::new();
    rate_config.set_rate("fast", 4);
    // "slow" defaults to rate 1
    compositor.set_rate_config(rate_config);

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    // After 1 outer tick: fast counter has count=4, slow counter has count=1
    let write_buf = compositor.buffer().write_all();

    let fast_sc = StateContribution::from_toml(&write_buf["fast"]).unwrap();
    let fast_count = fast_sc.state.as_table().unwrap()
        .get("count").unwrap().as_integer().unwrap();
    assert_eq!(fast_count, 4, "fast partition should have stepped 4 times");

    let slow_sc = StateContribution::from_toml(&write_buf["slow"]).unwrap();
    let slow_count = slow_sc.state.as_table().unwrap()
        .get("count").unwrap().as_integer().unwrap();
    assert_eq!(slow_count, 1, "slow partition should have stepped 1 time");
}

/// After 5 outer ticks: fast=20, slow=5.
#[test]
fn multi_rate_accumulates_over_multiple_ticks() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("fast")),
        Box::new(Counter::new("slow")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    let mut rate_config = RateConfig::new();
    rate_config.set_rate("fast", 4);
    compositor.set_rate_config(rate_config);

    compositor.init().unwrap();
    for _ in 0..5 {
        compositor.run_tick(1.0).unwrap();
    }

    let write_buf = compositor.buffer().write_all();

    let fast_sc = StateContribution::from_toml(&write_buf["fast"]).unwrap();
    let fast_count = fast_sc.state.as_table().unwrap()
        .get("count").unwrap().as_integer().unwrap();
    assert_eq!(fast_count, 20, "fast partition should have stepped 20 times after 5 ticks");

    let slow_sc = StateContribution::from_toml(&write_buf["slow"]).unwrap();
    let slow_count = slow_sc.state.as_table().unwrap()
        .get("count").unwrap().as_integer().unwrap();
    assert_eq!(slow_count, 5, "slow partition should have stepped 5 times after 5 ticks");
}

/// Shared context on bus reflects the final sub-step state.
#[test]
fn shared_context_reflects_final_state() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = vec![
        Box::new(Counter::new("fast")),
        Box::new(Counter::new("slow")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    let mut rate_config = RateConfig::new();
    rate_config.set_rate("fast", 4);
    compositor.set_rate_config(rate_config);

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    // The shared context published on the bus should reflect the final state
    // We verify via the write buffer which is what gets published
    let write_buf = compositor.buffer().write_all();

    // fast partition stepped 4 times, so its state should show count=4
    let fast_sc = StateContribution::from_toml(&write_buf["fast"]).unwrap();
    let fast_count = fast_sc.state.as_table().unwrap()
        .get("count").unwrap().as_integer().unwrap();
    assert_eq!(fast_count, 4, "shared context should reflect final sub-step state");
}

/// Default rate config (no rates set) means all partitions step once per tick.
#[test]
fn default_rate_is_one() {
    let config = RateConfig::new();
    assert_eq!(config.get_rate("anything"), 1);
    assert_eq!(config.get_rate("nonexistent"), 1);
}

/// Rate multiplier of 0 should panic.
#[test]
#[should_panic(expected = "rate multiplier must be at least 1")]
fn rate_zero_panics() {
    let mut config = RateConfig::new();
    config.set_rate("bad", 0);
}

// ---------------------------------------------------------------------------
// dt correctness tests (Fix 2)
// ---------------------------------------------------------------------------

/// Accumulator with rate=4 and outer dt=1.0 should accumulate 1.0 per tick
/// (4 sub-steps of 0.25 each).
#[test]
fn multi_rate_dt_is_divided_correctly() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Accumulator::new("acc")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    let mut rate_config = RateConfig::new();
    rate_config.set_rate("acc", 4);
    compositor.set_rate_config(rate_config);

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    // 4 sub-steps * 0.25 = 1.0
    let write_buf = compositor.buffer().write_all();
    let acc_sc = StateContribution::from_toml(&write_buf["acc"]).unwrap();
    let total = acc_sc.state.as_table().unwrap()
        .get("total").unwrap().as_float().unwrap();
    assert!(
        (total - 1.0).abs() < 1e-12,
        "after 1 tick with rate=4 and dt=1.0, total should be 1.0 but was {}",
        total,
    );

    // After a second tick the total should be 2.0
    compositor.run_tick(1.0).unwrap();
    let write_buf = compositor.buffer().write_all();
    let acc_sc = StateContribution::from_toml(&write_buf["acc"]).unwrap();
    let total = acc_sc.state.as_table().unwrap()
        .get("total").unwrap().as_float().unwrap();
    assert!(
        (total - 2.0).abs() < 1e-12,
        "after 2 ticks with rate=4 and dt=1.0, total should be 2.0 but was {}",
        total,
    );
}

// ---------------------------------------------------------------------------
// Fallback during multi-rate test (Fix 3)
// ---------------------------------------------------------------------------

/// A partition that fails after N steps. Used to test fallback activation
/// during a multi-rate sub-step cycle.
struct FailAfterN {
    id: String,
    count: u64,
    fail_at: u64,
    initialized: bool,
}

impl FailAfterN {
    fn new(id: impl Into<String>, fail_at: u64) -> Self {
        Self {
            id: id.into(),
            count: 0,
            fail_at,
            initialized: false,
        }
    }
}

impl Partition for FailAfterN {
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
        self.count += 1;
        if self.count >= self.fail_at {
            return Err(PartitionError::new(&self.id, "step", "intentional failure"));
        }
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        self.initialized = false;
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        let mut table = toml::map::Map::new();
        table.insert("count".to_string(), toml::Value::Integer(self.count as i64));
        Ok(toml::Value::Table(table))
    }

    fn load_state(&mut self, _state: toml::Value) -> Result<(), PartitionError> {
        Ok(())
    }
}

/// When a partition fails on sub-step 3 (0-indexed: sub=2) with rate=4,
/// the fallback should be activated and complete the remaining sub-steps.
/// Fallback (Counter) should be stepped for: sub 2 (the failed one) + sub 3 = 2 steps.
#[test]
fn fallback_completes_remaining_sub_steps() {
    // FailAfterN with fail_at=3 means it succeeds on steps 1,2 and fails on step 3 (sub=2).
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(FailAfterN::new("fragile", 3)),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    let mut rate_config = RateConfig::new();
    rate_config.set_rate("fragile", 4);
    compositor.set_rate_config(rate_config);

    // Register a Counter as fallback
    compositor.register_fallback("fragile", Box::new(Counter::new("fragile")));

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    // The fallback Counter should have been stepped for:
    //   sub 2 (the failed sub-step, fallback takes over) + sub 3 (remaining) = 2 steps
    let write_buf = compositor.buffer().write_all();
    let fragile_sc = StateContribution::from_toml(&write_buf["fragile"]).unwrap();
    let count = fragile_sc.state.as_table().unwrap()
        .get("count").unwrap().as_integer().unwrap();
    assert_eq!(
        count, 2,
        "fallback should complete remaining sub-steps: 1 for the failed sub-step + 1 remaining = 2"
    );
}

// ---------------------------------------------------------------------------
// Edge case tests (Fix 4)
// ---------------------------------------------------------------------------

/// Non-power-of-2 rate (rate=3): dt division and total should be correct.
#[test]
fn non_power_of_two_rate_divides_dt_correctly() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Accumulator::new("acc3")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    let mut rate_config = RateConfig::new();
    rate_config.set_rate("acc3", 3);
    compositor.set_rate_config(rate_config);

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    // 3 sub-steps * (1.0/3.0) = 1.0
    let write_buf = compositor.buffer().write_all();
    let acc3_sc = StateContribution::from_toml(&write_buf["acc3"]).unwrap();
    let total = acc3_sc.state.as_table().unwrap()
        .get("total").unwrap().as_float().unwrap();
    assert!(
        (total - 1.0).abs() < 1e-12,
        "rate=3 with dt=1.0 should sum to 1.0 but was {}",
        total,
    );
}

/// Multiple partitions with different rates (rate=2 and rate=3) in the same compositor.
#[test]
fn mixed_rates_in_same_compositor() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Counter::new("r2")),
        Box::new(Counter::new("r3")),
        Box::new(Accumulator::new("a2")),
        Box::new(Accumulator::new("a3")),
    ];
    let bus = InProcessBus::new("test-bus");
    let mut compositor = Compositor::new(partitions, Box::new(bus));

    let mut rate_config = RateConfig::new();
    rate_config.set_rate("r2", 2);
    rate_config.set_rate("r3", 3);
    rate_config.set_rate("a2", 2);
    rate_config.set_rate("a3", 3);
    compositor.set_rate_config(rate_config);

    compositor.init().unwrap();
    compositor.run_tick(1.0).unwrap();

    let write_buf = compositor.buffer().write_all();

    // Counter r2: 2 steps
    let r2_sc = StateContribution::from_toml(&write_buf["r2"]).unwrap();
    let r2_count = r2_sc.state.as_table().unwrap()
        .get("count").unwrap().as_integer().unwrap();
    assert_eq!(r2_count, 2, "rate=2 counter should have 2 steps");

    // Counter r3: 3 steps
    let r3_sc = StateContribution::from_toml(&write_buf["r3"]).unwrap();
    let r3_count = r3_sc.state.as_table().unwrap()
        .get("count").unwrap().as_integer().unwrap();
    assert_eq!(r3_count, 3, "rate=3 counter should have 3 steps");

    // Accumulator a2: 2 * 0.5 = 1.0
    let a2_sc = StateContribution::from_toml(&write_buf["a2"]).unwrap();
    let a2_total = a2_sc.state.as_table().unwrap()
        .get("total").unwrap().as_float().unwrap();
    assert!(
        (a2_total - 1.0).abs() < 1e-12,
        "rate=2 accumulator should total 1.0 but was {}",
        a2_total,
    );

    // Accumulator a3: 3 * (1/3) = 1.0
    let a3_sc = StateContribution::from_toml(&write_buf["a3"]).unwrap();
    let a3_total = a3_sc.state.as_table().unwrap()
        .get("total").unwrap().as_float().unwrap();
    assert!(
        (a3_total - 1.0).abs() < 1e-12,
        "rate=3 accumulator should total 1.0 but was {}",
        a3_total,
    );
}
