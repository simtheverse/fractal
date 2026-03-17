use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::sync::Arc;

use fpa_bus::InProcessBus;
use fpa_compositor::compositor::Compositor;
use fpa_contract::test_support::Counter;
use fpa_contract::Partition;

fn make_compositor(n: usize) -> Compositor {
    let partitions: Vec<Box<dyn Partition>> = (0..n)
        .map(|i| Box::new(Counter::new(format!("c{}", i))) as Box<dyn Partition>)
        .collect();
    let bus = InProcessBus::new("bench");
    Compositor::new(partitions, Arc::new(bus))
}

fn bench_tick_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick_overhead");
    for &n in &[10, 100, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let mut comp = make_compositor(n);
            comp.init().unwrap();
            b.iter(|| comp.run_tick(1.0 / 60.0).unwrap());
            comp.shutdown().unwrap();
        });
    }
    group.finish();
}

fn make_nested(depth: usize) -> Compositor {
    if depth == 0 {
        let partitions: Vec<Box<dyn Partition>> =
            vec![Box::new(Counter::new("leaf"))];
        return Compositor::new(partitions, Arc::new(InProcessBus::new("leaf-bus")));
    }
    let inner = make_nested(depth - 1);
    let partitions: Vec<Box<dyn Partition>> = vec![
        Box::new(Counter::new(format!("c-d{}", depth))),
        Box::new(
            inner
                .with_id(format!("nested-d{}", depth - 1))
                .with_layer_depth(depth as u32),
        ),
    ];
    Compositor::new(
        partitions,
        Arc::new(InProcessBus::new(format!("bus-d{}", depth))),
    )
}

fn bench_relay_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("relay_depth");
    for depth in 1..=3 {
        group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, &depth| {
            let mut comp = make_nested(depth);
            comp.init().unwrap();
            b.iter(|| comp.run_tick(1.0 / 60.0).unwrap());
            comp.shutdown().unwrap();
        });
    }
    group.finish();
}

criterion_group!(benches, bench_tick_overhead, bench_relay_depth);
criterion_main!(benches);
