//! Event bus benchmarks — publish/subscribe throughput and latency.

mod helpers;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ferroq_gateway::bus::EventBus;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Benchmark: publish a single event with no subscribers.
fn bench_publish_no_subscribers(c: &mut Criterion) {
    let bus = EventBus::new();
    let event = helpers::make_message_event(123456, 1);

    c.bench_function("bus/publish_no_sub", |b| {
        b.iter(|| {
            bus.publish(black_box(event.clone()));
        });
    });
}

/// Benchmark: publish a single event to N subscribers.
fn bench_publish_with_subscribers(c: &mut Criterion) {
    let mut group = c.benchmark_group("bus/publish_to_N_sub");

    for n_subs in [1, 4, 16] {
        let bus = EventBus::new();
        let _receivers: Vec<_> = (0..n_subs).map(|_| bus.subscribe()).collect();
        let event = helpers::make_message_event(123456, 1);

        group.bench_with_input(BenchmarkId::from_parameter(n_subs), &n_subs, |b, _| {
            b.iter(|| {
                bus.publish(black_box(event.clone()));
            });
        });
    }
    group.finish();
}

/// Benchmark: end-to-end publish → receive latency (single subscriber).
fn bench_publish_receive_latency(c: &mut Criterion) {
    let rt = Runtime::new().expect("failed to create tokio runtime");

    c.bench_function("bus/publish_receive_latency", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = Arc::new(EventBus::new());
                let mut rx = bus.subscribe();
                let event = helpers::make_message_event(123456, 1);
                bus.publish(event);
                let _ = black_box(rx.recv().await);
            });
        });
    });
}

/// Benchmark: throughput — publish 1000 events (small payload).
fn bench_throughput_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("bus/throughput_small");
    let count = 1000u64;
    group.throughput(Throughput::Elements(count));

    let bus = EventBus::new();
    let _rx = bus.subscribe(); // need at least one subscriber

    group.bench_function("1000_events", |b| {
        b.iter(|| {
            for i in 0..count {
                let event = helpers::make_message_event(123456, i as i64);
                bus.publish(black_box(event));
            }
        });
    });
    group.finish();
}

/// Benchmark: throughput — publish 1000 events (1KB payload).
fn bench_throughput_1kb(c: &mut Criterion) {
    let mut group = c.benchmark_group("bus/throughput_1kb");
    let count = 1000u64;
    group.throughput(Throughput::Elements(count));

    let bus = EventBus::new();
    let _rx = bus.subscribe();

    group.bench_function("1000_events", |b| {
        b.iter(|| {
            for i in 0..count {
                let event = helpers::make_1kb_message_event(123456, i as i64);
                bus.publish(black_box(event));
            }
        });
    });
    group.finish();
}

/// Benchmark: throughput with async consumers draining (1 subscriber).
fn bench_throughput_async_drain(c: &mut Criterion) {
    let rt = Runtime::new().expect("failed to create tokio runtime");
    let mut group = c.benchmark_group("bus/throughput_async_drain");
    let count = 1000u64;
    group.throughput(Throughput::Elements(count));

    group.bench_function("1000_events", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = Arc::new(EventBus::new());
                let mut rx = bus.subscribe();

                // produce
                for i in 0..count {
                    let event = helpers::make_message_event(123456, i as i64);
                    bus.publish(event);
                }

                // drain
                for _ in 0..count {
                    let _ = black_box(rx.recv().await);
                }
            });
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_publish_no_subscribers,
    bench_publish_with_subscribers,
    bench_publish_receive_latency,
    bench_throughput_small,
    bench_throughput_1kb,
    bench_throughput_async_drain,
);
criterion_main!(benches);
