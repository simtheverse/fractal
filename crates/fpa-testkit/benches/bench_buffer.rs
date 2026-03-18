use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use fpa_compositor::double_buffer::DoubleBuffer;

fn bench_buffer_swap(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_swap");

    for &n in &[10, 100, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let keys: Vec<String> = (0..n).map(|i| format!("p{}", i)).collect();
            let mut buf = DoubleBuffer::with_capacity(n);
            // Pre-fill the write buffer
            for (i, key) in keys.iter().enumerate() {
                buf.write(key, toml::Value::Integer(i as i64));
            }
            b.iter(|| {
                buf.swap();
                // Re-fill write buffer for next swap
                for (i, key) in keys.iter().enumerate() {
                    buf.write(key, toml::Value::Integer(i as i64));
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_buffer_swap);
criterion_main!(benches);
