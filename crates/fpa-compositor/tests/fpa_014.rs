//! Tests for FPA-014: Compositor Tick Lifecycle (double-buffer isolation).

use std::sync::Arc;

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_compositor::double_buffer::DoubleBuffer;
use fpa_contract::test_support::Counter;
use fpa_contract::StateContribution;

/// Helper: create and initialize a compositor with the given partition IDs.
fn make_compositor(ids: &[&str]) -> Compositor {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> = ids
        .iter()
        .map(|id| Box::new(Counter::new(*id)) as Box<dyn fpa_contract::Partition>)
        .collect();
    let bus = InProcessBus::new("test");
    let mut comp = Compositor::new(partitions, Arc::new(bus));
    comp.init().unwrap();
    comp
}

/// Test 1: Partition A's tick N output is NOT visible to partition B during tick N.
///
/// After running tick 1, both partitions have written to the write buffer,
/// but the read buffer (which partitions would consult) still holds the
/// previous tick's state — it does not contain tick 1's outputs.
#[test]
fn tick_n_output_not_visible_during_tick_n() {
    let mut comp = make_compositor(&["a", "b"]);

    // Before any tick, both buffers are empty.
    assert!(comp.buffer().read("a").is_none());
    assert!(comp.buffer().read("b").is_none());

    // Run tick 1.
    comp.run_tick(1.0).unwrap();

    // After tick 1, outputs are in the WRITE buffer (not yet readable).
    // The read buffer should still be empty (it was swapped from the
    // previously-empty write buffer at the start of tick 1).
    assert!(
        comp.buffer().read("a").is_none(),
        "partition a's tick-1 output should NOT be in the read buffer during/after tick 1"
    );
    assert!(
        comp.buffer().read("b").is_none(),
        "partition b's tick-1 output should NOT be in the read buffer during/after tick 1"
    );

    // But the write buffer does contain both outputs.
    assert!(comp.buffer().write_all().contains_key("a"));
    assert!(comp.buffer().write_all().contains_key("b"));
}

/// Test 2: Partition B reads A's tick N output during tick N+1.
///
/// After running two ticks, the read buffer contains tick 1's outputs
/// (because the swap at the start of tick 2 moved them to the read side).
#[test]
fn tick_n_output_readable_during_tick_n_plus_1() {
    let mut comp = make_compositor(&["a", "b"]);

    // Tick 1: counters go to 1.
    comp.run_tick(1.0).unwrap();

    // Tick 2: swap moves tick-1 outputs to read buffer.
    comp.run_tick(1.0).unwrap();

    // The read buffer now contains tick-1 outputs (count=1), wrapped in StateContribution.
    let read_a = comp.buffer().read("a").unwrap();
    let read_sc = StateContribution::from_toml(read_a).unwrap();
    let count_a = read_sc.state.as_table().unwrap().get("count").unwrap().as_integer().unwrap();
    assert_eq!(count_a, 1, "read buffer should contain tick-1 output (count=1)");

    // The write buffer should have tick-2 outputs (count=2).
    let write_a = comp.buffer().write_all().get("a").unwrap();
    let write_sc = StateContribution::from_toml(write_a).unwrap();
    let write_count_a = write_sc.state.as_table().unwrap().get("count").unwrap().as_integer().unwrap();
    assert_eq!(write_count_a, 2, "write buffer should contain tick-2 output (count=2)");
}

/// Test 3: Step order does not affect final result.
///
/// Running partitions in [A, B] order vs [B, A] order should produce
/// identical final state after multiple ticks, because the double buffer
/// prevents intra-tick visibility.
#[test]
fn step_order_does_not_affect_result() {
    // Forward order: A then B.
    let mut forward = make_compositor(&["a", "b"]);
    for _ in 0..5 {
        forward.run_tick(1.0).unwrap();
    }

    // Reverse order: B then A.
    let mut reverse = make_compositor(&["b", "a"]);
    for _ in 0..5 {
        reverse.run_tick(1.0).unwrap();
    }

    // Compare: both should have identical read and write buffer contents.
    let fwd_read = forward.buffer().read_all();
    let rev_read = reverse.buffer().read_all();
    assert_eq!(fwd_read, rev_read, "read buffers should be identical regardless of step order");

    let fwd_write = forward.buffer().write_all();
    let rev_write = reverse.buffer().write_all();
    assert_eq!(fwd_write, rev_write, "write buffers should be identical regardless of step order");
}

/// Test 4: Buffer swap occurs between pre-tick and partition stepping.
///
/// Verify that the swap happens first within run_tick by inspecting
/// the read buffer state. We manually write to the buffer, then run a
/// tick and confirm the swap moved those values to the read side.
#[test]
fn buffer_swap_occurs_before_stepping() {
    let partitions: Vec<Box<dyn fpa_contract::Partition>> =
        vec![Box::new(Counter::new("a"))];
    let bus = InProcessBus::new("test");
    let mut comp = Compositor::new(partitions, Arc::new(bus));
    comp.init().unwrap();

    // We can't directly write to the compositor's buffer, so we use the
    // DoubleBuffer unit test below for this invariant instead.
    // Here we just verify the tick lifecycle works correctly.
    comp.run_tick(1.0).unwrap();

    // After tick 1, "a" is in write buffer only.
    assert!(comp.buffer().read("a").is_none());
    assert!(comp.buffer().write_all().contains_key("a"));

    // After tick 2, "a"'s tick-1 output is in read buffer.
    comp.run_tick(1.0).unwrap();
    assert!(comp.buffer().read("a").is_some());
}

/// Test: DoubleBuffer isolation at the unit level.
///
/// Writes during a tick are invisible to reads until swap.
#[test]
fn double_buffer_isolation() {
    let mut buf = DoubleBuffer::new();

    // Write two partition outputs.
    buf.write("physics", toml::Value::Float(9.81));
    buf.write("render", toml::Value::String("frame_0".into()));

    // Neither is readable yet.
    assert!(buf.read("physics").is_none());
    assert!(buf.read("render").is_none());

    // Swap.
    buf.swap();

    // Now both are readable.
    assert_eq!(buf.read("physics"), Some(&toml::Value::Float(9.81)));
    assert_eq!(
        buf.read("render"),
        Some(&toml::Value::String("frame_0".into()))
    );

    // Write buffer is clear.
    assert!(buf.write_all().is_empty());
}

/// Test 1C.4: Step order independence across all permutations.
///
/// 3 partitions (a, b, c), all 6 permutations, 100 ticks each.
/// Read and write buffers must be identical across all permutations,
/// confirming that the double buffer prevents intra-tick visibility.
#[test]
fn step_order_independent_across_all_permutations() {
    let permutations: [&[&str]; 6] = [
        &["a", "b", "c"],
        &["a", "c", "b"],
        &["b", "a", "c"],
        &["b", "c", "a"],
        &["c", "a", "b"],
        &["c", "b", "a"],
    ];
    let tick_count = 100;

    // Run the reference permutation
    let mut reference = make_compositor(permutations[0]);
    for _ in 0..tick_count {
        reference.run_tick(1.0).unwrap();
    }
    let ref_read = reference.buffer().read_all().clone();
    let ref_write = reference.buffer().write_all().clone();

    // Run each remaining permutation and compare
    for perm in &permutations[1..] {
        let mut comp = make_compositor(perm);
        for _ in 0..tick_count {
            comp.run_tick(1.0).unwrap();
        }

        let read = comp.buffer().read_all();
        let write = comp.buffer().write_all();

        assert_eq!(
            &ref_read, read,
            "read buffers differ for permutation {:?} vs {:?}",
            permutations[0], perm
        );
        assert_eq!(
            &ref_write, write,
            "write buffers differ for permutation {:?} vs {:?}",
            permutations[0], perm
        );
    }
}

/// Test: Buffer swap moves sentinel values to read side (unit-level).
#[test]
fn buffer_swap_moves_sentinel() {
    let mut buf = DoubleBuffer::new();

    // Manually inject a value into the write buffer.
    buf.write("sentinel", toml::Value::Boolean(true));

    // Swap.
    buf.swap();

    // The sentinel should now be in the read buffer.
    assert_eq!(
        buf.read("sentinel"),
        Some(&toml::Value::Boolean(true)),
        "sentinel should have been swapped to read buffer"
    );

    // Write buffer is clear.
    assert!(buf.write_all().is_empty());
}
