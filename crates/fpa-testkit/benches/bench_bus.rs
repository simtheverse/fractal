use criterion::{criterion_group, criterion_main, Criterion};
use std::sync::Arc;

use fpa_bus::{AsyncBus, Bus, BusExt, BusReader, InProcessBus, NetworkBus};
use fpa_contract::test_support::SensorReading;

fn bench_bus_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("bus_throughput");
    let msg = SensorReading {
        value: 42.0,
        source: "bench".into(),
    };

    group.bench_function("inprocess", |b| {
        let bus = Arc::new(InProcessBus::new("bench"));
        let mut reader = bus.subscribe::<SensorReading>();
        b.iter(|| {
            bus.publish(msg.clone());
            std::hint::black_box(reader.read());
        });
    });

    group.bench_function("async", |b| {
        let bus = Arc::new(AsyncBus::new("bench"));
        let mut reader = bus.subscribe::<SensorReading>();
        b.iter(|| {
            bus.publish(msg.clone());
            std::hint::black_box(reader.read());
        });
    });

    group.bench_function("network", |b| {
        let bus = Arc::new(NetworkBus::new("bench"));
        let mut reader = bus.subscribe::<SensorReading>();
        b.iter(|| {
            bus.publish(msg.clone());
            std::hint::black_box(reader.read());
        });
    });

    group.finish();
}

fn bench_type_erasure(c: &mut Criterion) {
    let msg = SensorReading {
        value: 42.0,
        source: "bench".into(),
    };

    c.bench_function("type_erasure_dyn_dispatch", |b| {
        let bus: Arc<dyn Bus> = Arc::new(InProcessBus::new("bench"));
        let mut reader = bus.subscribe::<SensorReading>();
        b.iter(|| {
            bus.publish(msg.clone());
            std::hint::black_box(reader.read());
        });
    });

    c.bench_function("type_erasure_concrete", |b| {
        let bus = Arc::new(InProcessBus::new("bench"));
        let mut reader = bus.subscribe::<SensorReading>();
        b.iter(|| {
            bus.publish(msg.clone());
            std::hint::black_box(reader.read());
        });
    });
}

criterion_group!(benches, bench_bus_throughput, bench_type_erasure);
criterion_main!(benches);
