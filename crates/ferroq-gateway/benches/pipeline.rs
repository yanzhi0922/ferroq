//! End-to-end pipeline benchmarks.
//!
//! Measures the full message forwarding path:
//! raw JSON → parse → dedup → event bus publish → subscriber receive → serialize.

mod helpers;

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use ferroq_gateway::bus::EventBus;
use ferroq_gateway::dedup::DedupFilter;
use ferroq_gateway::onebot_v11;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Benchmark: full pipeline — parse → dedup → bus → receive → serialize.
fn bench_full_pipeline_latency(c: &mut Criterion) {
    let rt = Runtime::new().expect("failed to create tokio runtime");

    c.bench_function("pipeline/full_latency", |b| {
        let mut msg_id = 0i64;
        b.iter(|| {
            msg_id += 1;
            rt.block_on(async {
                let bus = Arc::new(EventBus::new());
                let dedup = DedupFilter::new(60);
                let mut rx = bus.subscribe();

                let raw = helpers::make_raw_onebot_v11_json(123456, msg_id);
                let event = onebot_v11::parse_event(raw).expect("parse");

                if !dedup.is_duplicate(&event) {
                    bus.publish(event);
                }

                let received = rx.recv().await.expect("recv");
                let _ = black_box(serde_json::to_string(&received));
            });
        });
    });
}

/// Benchmark: full pipeline — parse → dedup → bus → receive → serialize (1KB payload).
fn bench_full_pipeline_1kb(c: &mut Criterion) {
    let rt = Runtime::new().expect("failed to create tokio runtime");

    c.bench_function("pipeline/full_latency_1kb", |b| {
        let mut msg_id = 0i64;
        b.iter(|| {
            msg_id += 1;
            rt.block_on(async {
                let bus = Arc::new(EventBus::new());
                let dedup = DedupFilter::new(60);
                let mut rx = bus.subscribe();

                let raw = helpers::make_raw_onebot_v11_1kb_json(123456, msg_id);
                let event = onebot_v11::parse_event(raw).expect("parse");

                if !dedup.is_duplicate(&event) {
                    bus.publish(event);
                }

                let received = rx.recv().await.expect("recv");
                let _ = black_box(serde_json::to_string(&received));
            });
        });
    });
}

/// Benchmark: pipeline throughput — 1000 messages end-to-end (reuse bus).
fn bench_pipeline_throughput(c: &mut Criterion) {
    let rt = Runtime::new().expect("failed to create tokio runtime");
    let mut group = c.benchmark_group("pipeline/throughput");
    let count = 1000u64;
    group.throughput(Throughput::Elements(count));

    group.bench_function("1000_events", |b| {
        let mut base_id = 0i64;
        b.iter(|| {
            rt.block_on(async {
                let bus = Arc::new(EventBus::new());
                let dedup = DedupFilter::new(600);
                let mut rx = bus.subscribe();

                for _ in 0..count {
                    base_id += 1;
                    let raw = helpers::make_raw_onebot_v11_json(123456, base_id);
                    let event = onebot_v11::parse_event(raw).expect("parse");
                    if !dedup.is_duplicate(&event) {
                        bus.publish(event);
                    }
                }

                for _ in 0..count {
                    let received = rx.recv().await.expect("recv");
                    let _ = black_box(serde_json::to_string(&received));
                }
            });
        });
    });
    group.finish();
}

/// Benchmark: pipeline throughput — 1000 events at 1KB each.
fn bench_pipeline_throughput_1kb(c: &mut Criterion) {
    let rt = Runtime::new().expect("failed to create tokio runtime");
    let mut group = c.benchmark_group("pipeline/throughput_1kb");
    let count = 1000u64;
    group.throughput(Throughput::Elements(count));

    group.bench_function("1000_events", |b| {
        let mut base_id = 0i64;
        b.iter(|| {
            rt.block_on(async {
                let bus = Arc::new(EventBus::new());
                let dedup = DedupFilter::new(600);
                let mut rx = bus.subscribe();

                for _ in 0..count {
                    base_id += 1;
                    let raw = helpers::make_raw_onebot_v11_1kb_json(123456, base_id);
                    let event = onebot_v11::parse_event(raw).expect("parse");
                    if !dedup.is_duplicate(&event) {
                        bus.publish(event);
                    }
                }

                for _ in 0..count {
                    let received = rx.recv().await.expect("recv");
                    let _ = black_box(serde_json::to_string(&received));
                }
            });
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_full_pipeline_latency,
    bench_full_pipeline_1kb,
    bench_pipeline_throughput,
    bench_pipeline_throughput_1kb,
);
criterion_main!(benches);
