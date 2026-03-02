//! Deduplication filter benchmarks.

mod helpers;

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use ferroq_gateway::dedup::DedupFilter;

/// Benchmark: check a unique event (cache miss).
fn bench_dedup_unique(c: &mut Criterion) {
    let filter = DedupFilter::new(60);

    c.bench_function("dedup/unique_event", |b| {
        let mut counter = 0i64;
        b.iter(|| {
            counter += 1;
            let event = helpers::make_message_event(123456, counter);
            let _ = black_box(filter.is_duplicate(&event));
        });
    });
}

/// Benchmark: check a duplicate event (cache hit).
fn bench_dedup_duplicate(c: &mut Criterion) {
    let filter = DedupFilter::new(60);
    // Seed the filter with one event.
    let event = helpers::make_message_event(123456, 42);
    let _ = filter.is_duplicate(&event);

    c.bench_function("dedup/duplicate_event", |b| {
        b.iter(|| {
            // Same self_id + message_id → duplicate.
            let event = helpers::make_message_event(123456, 42);
            let _ = black_box(filter.is_duplicate(&event));
        });
    });
}

/// Benchmark: throughput of unique events through dedup filter.
fn bench_dedup_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("dedup/throughput");
    let count = 1000u64;
    group.throughput(Throughput::Elements(count));

    group.bench_function("1000_unique", |b| {
        let filter = DedupFilter::new(60);
        let mut counter = 0i64;
        b.iter(|| {
            for _ in 0..count {
                counter += 1;
                let event = helpers::make_message_event(123456, counter);
                let _ = black_box(filter.is_duplicate(&event));
            }
        });
    });
    group.finish();
}

/// Benchmark: dedup under different cache sizes (warm cache with N entries).
fn bench_dedup_warm_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("dedup/warm_cache");

    for cache_size in [100, 1000, 10_000] {
        let filter = DedupFilter::new(600); // long window so nothing expires
        // Pre-fill the filter.
        for i in 0..cache_size {
            let event = helpers::make_message_event(123456, i);
            let _ = filter.is_duplicate(&event);
        }

        group.bench_with_input(
            BenchmarkId::from_parameter(cache_size),
            &cache_size,
            |b, &size| {
                let mut counter = size;
                b.iter(|| {
                    counter += 1;
                    let event = helpers::make_message_event(123456, counter);
                    let _ = black_box(filter.is_duplicate(&event));
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_dedup_unique,
    bench_dedup_duplicate,
    bench_dedup_throughput,
    bench_dedup_warm_cache,
);
criterion_main!(benches);
