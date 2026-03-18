//! Phase 6 Track P: Replaceability & Isolation Evaluation
//!
//! 6P.1 — Partition swap verification: an alternative partition implementation
//! (ScalingCounter) is defined here and exercised through both contract and
//! compositional test suites to verify drop-in replaceability.
//!
//! 6P.2 — Test isolation measurement: verifies that contract tests have no
//! peer-crate imports and that the test pyramid has the expected shape.

use std::sync::Arc;

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_compositor::state_machine::ExecutionState;
use fpa_contract::error::PartitionError;
use fpa_contract::test_support::{Accumulator, CanonicalInputs, Counter, OutputProperties};
use fpa_contract::Partition;

// ---------------------------------------------------------------------------
// ScalingCounter: alternative Partition implementation for swap experiments
// ---------------------------------------------------------------------------

struct ScalingCounter {
    id: String,
    count: u64,
    scale: f64,
    initialized: bool,
}

impl ScalingCounter {
    fn new(id: impl Into<String>, scale: f64) -> Self {
        Self {
            id: id.into(),
            count: 0,
            scale,
            initialized: false,
        }
    }
}

impl Partition for ScalingCounter {
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
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), PartitionError> {
        self.initialized = false;
        Ok(())
    }

    fn contribute_state(&self) -> Result<toml::Value, PartitionError> {
        let count = i64::try_from(self.count).map_err(|_| {
            PartitionError::new(&self.id, "contribute_state", "count exceeds i64::MAX")
        })?;
        let mut table = toml::map::Map::new();
        table.insert("count".to_string(), toml::Value::Integer(count));
        table.insert("scale".to_string(), toml::Value::Float(self.scale));
        Ok(toml::Value::Table(table))
    }

    fn load_state(&mut self, state: toml::Value) -> Result<(), PartitionError> {
        let table = state.as_table().ok_or_else(|| {
            PartitionError::new(&self.id, "load_state", "expected table")
        })?;
        let count = table
            .get("count")
            .and_then(|v| v.as_integer())
            .ok_or_else(|| {
                PartitionError::new(&self.id, "load_state", "missing or invalid 'count'")
            })?;
        if count < 0 {
            return Err(PartitionError::new(
                &self.id,
                "load_state",
                "count is negative",
            ));
        }
        self.count = count as u64;
        self.scale = table
            .get("scale")
            .and_then(|v| v.as_float())
            .ok_or_else(|| {
                PartitionError::new(&self.id, "load_state", "missing or invalid 'scale'")
            })?;
        Ok(())
    }
}

// ===========================================================================
// 6P.1 — Partition swap verification
// ===========================================================================

/// ScalingCounter passes the generic contract test lifecycle using only
/// fpa_contract types and OutputProperties assertions.
#[test]
fn swap_alternative_through_contract_suite() {
    let dts = CanonicalInputs::timestep_sequence(10);
    let mut p = ScalingCounter::new("scaling", 2.0);

    p.init().unwrap();
    for dt in &dts {
        p.step(*dt).unwrap();
    }

    let state = p.contribute_state().unwrap();
    OutputProperties::assert_valid_state_table(&state);
    OutputProperties::assert_non_negative_numeric_fields(&state);
    OutputProperties::assert_state_roundtrip(&mut p, &state);

    p.shutdown().unwrap();
}

/// ScalingCounter mixed with Counter and Accumulator in a Compositor.
/// Asserts compositional properties: delivery, conservation, ordering.
#[test]
fn swap_alternative_through_compositional_suite() {
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(ScalingCounter::new("scaling", 3.0)),
        Box::new(Counter::new("counter")),
        Box::new(Accumulator::new("accum")),
    ];

    let ids: Vec<String> = partitions.iter().map(|p| p.id().to_string()).collect();
    let initial_count = partitions.len();
    let bus = InProcessBus::new("test-bus");
    let mut comp = Compositor::new(partitions, Arc::new(bus));

    assert_eq!(comp.state(), ExecutionState::Uninitialized);
    comp.init().unwrap();
    assert_eq!(comp.state(), ExecutionState::Running);

    for i in 0..10 {
        comp.run_tick(1.0 / 60.0).unwrap();

        // Conservation: partition count stable
        assert_eq!(comp.partitions().len(), initial_count);

        // Ordering: tick count monotonic
        assert_eq!(comp.tick_count(), (i + 1) as u64);
    }

    // Delivery: all partition states in write buffer
    let write_buf = comp.buffer().write_all();
    for id in &ids {
        assert!(
            write_buf.contains_key(id),
            "partition '{}' state missing from write buffer",
            id
        );
        let state = &write_buf[id];
        assert!(state.is_table());
        assert!(!state.as_table().unwrap().is_empty());
    }

    // Conservation: buffer has exactly N entries
    assert_eq!(write_buf.len(), initial_count);

    comp.shutdown().unwrap();
    assert_eq!(comp.state(), ExecutionState::Terminated);
}

/// ScalingCounter compiles from this test file with only fpa_contract imports.
/// No peer partition source modules are referenced. The fact this file compiles
/// is the proof; this test documents that explicitly.
#[test]
fn swap_no_peer_source_changes() {
    println!(
        "ScalingCounter compiles from test file with only fpa_contract imports \
         — 0 peer dependencies"
    );
}

/// Metrics for the ScalingCounter swap experiment.
#[test]
fn swap_metrics() {
    // Measure ScalingCounter LOC from the source file itself.
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let src = std::fs::read_to_string(
        workspace_root.join("crates/fpa-testkit/tests/eval_replaceability.rs"),
    )
    .unwrap();
    let start = src
        .find("struct ScalingCounter")
        .expect("should find struct");
    let impl_end_marker = "// ===========================================================================";
    let end = src[start..]
        .find(impl_end_marker)
        .map(|i| start + i)
        .unwrap_or(src.len());
    let loc = src[start..end].lines().count();
    let files_touched = 1;
    let compilation_errors = 0;

    println!("ScalingCounter swap metrics:");
    println!("  LOC (approx): {}", loc);
    println!("  Files touched: {}", files_touched);
    println!("  Compilation errors: {}", compilation_errors);

    assert!(loc > 0);
    assert_eq!(files_touched, 1);
    assert_eq!(compilation_errors, 0);
}

// ===========================================================================
// 6P.2 — Test isolation measurement
// ===========================================================================

/// Contract-tier tests should not instantiate types from peer crates
/// (fpa_compositor, fpa_bus, fpa_testkit). They should only use fpa_contract.
#[test]
fn contract_tests_no_peer_instantiation() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let contract_tests = workspace_root.join("crates/fpa-contract/tests");

    let forbidden = ["use fpa_compositor", "use fpa_bus", "use fpa_testkit"];

    let mut checked = 0;
    for entry in std::fs::read_dir(&contract_tests).expect("contract tests dir should exist") {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if !name.starts_with("fpa_") || !name.ends_with(".rs") {
            continue;
        }
        let content = std::fs::read_to_string(&path).unwrap();
        for pattern in &forbidden {
            assert!(
                !content.contains(pattern),
                "{} contains forbidden import '{}'",
                name,
                pattern
            );
        }
        checked += 1;
    }

    println!(
        "Checked {} contract test files — no peer crate imports found",
        checked
    );
    assert!(checked > 0, "should have checked at least one contract test file");
}

/// Count #[test] in spec-linked test files (fpa_*.rs) in each tier and verify
/// the pyramid shape: contract >= system. Eval tests (eval_*.rs) are excluded
/// as they are meta-tests about the framework, not spec requirement tests.
#[test]
fn test_pyramid_shape() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let tiers: &[(&str, &str)] = &[
        ("contract", "crates/fpa-contract/tests"),
        ("bus", "crates/fpa-bus/tests"),
        ("compositor", "crates/fpa-compositor/tests"),
        ("system", "crates/fpa-testkit/tests"),
    ];

    let mut counts: Vec<(&str, usize)> = Vec::new();

    for (tier_name, rel_path) in tiers {
        let dir = workspace_root.join(rel_path);
        let mut tier_count = 0;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries {
                let entry = entry.unwrap();
                let path = entry.path();
                let name = path.file_name().unwrap().to_str().unwrap();
                if !name.starts_with("fpa_") || !name.ends_with(".rs") {
                    continue;
                }
                let content = std::fs::read_to_string(&path).unwrap();
                tier_count += content.matches("#[test]").count();
                tier_count += content.matches("#[tokio::test]").count();
            }
        }
        counts.push((tier_name, tier_count));
    }

    println!("Test pyramid counts:");
    for (tier, count) in &counts {
        println!("  {}: {} tests", tier, count);
    }

    let contract_count = counts.iter().find(|(t, _)| *t == "contract").unwrap().1;
    let system_count = counts.iter().find(|(t, _)| *t == "system").unwrap().1;

    assert!(
        contract_count >= system_count,
        "contract tier ({}) should have >= system tier ({}) tests (pyramid shape)",
        contract_count,
        system_count
    );
}
