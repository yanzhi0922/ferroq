//! OneBot v11 event parsing benchmarks — serialization/deserialization latency.

mod helpers;

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use ferroq_gateway::onebot_v11;

/// Benchmark: parse a small OneBot v11 message event JSON → internal Event.
fn bench_parse_small_message(c: &mut Criterion) {
    let raw = helpers::make_raw_onebot_v11_json(123456, 1);

    c.bench_function("parse/onebot_v11_small", |b| {
        b.iter(|| {
            let _ = black_box(onebot_v11::parse_event(raw.clone()));
        });
    });
}

/// Benchmark: parse a 1KB OneBot v11 message event JSON → internal Event.
fn bench_parse_1kb_message(c: &mut Criterion) {
    let raw = helpers::make_raw_onebot_v11_1kb_json(123456, 1);

    c.bench_function("parse/onebot_v11_1kb", |b| {
        b.iter(|| {
            let _ = black_box(onebot_v11::parse_event(raw.clone()));
        });
    });
}

/// Benchmark: serialize internal Event → JSON string.
fn bench_serialize_event(c: &mut Criterion) {
    let event = helpers::make_message_event(123456, 1);

    c.bench_function("serialize/event_to_json", |b| {
        b.iter(|| {
            let _ = black_box(serde_json::to_string(&event));
        });
    });
}

/// Benchmark: serialize 1KB internal Event → JSON string.
fn bench_serialize_1kb_event(c: &mut Criterion) {
    let event = helpers::make_1kb_message_event(123456, 1);

    c.bench_function("serialize/event_1kb_to_json", |b| {
        b.iter(|| {
            let _ = black_box(serde_json::to_string(&event));
        });
    });
}

/// Benchmark: full round-trip (raw JSON → Event → JSON string).
fn bench_roundtrip(c: &mut Criterion) {
    let raw = helpers::make_raw_onebot_v11_json(123456, 1);

    c.bench_function("roundtrip/parse_then_serialize", |b| {
        b.iter(|| {
            let event = onebot_v11::parse_event(raw.clone()).expect("parse");
            let _ = black_box(serde_json::to_string(&event));
        });
    });
}

/// Benchmark: batch parse throughput (1000 events).
fn bench_parse_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse/throughput");
    let count = 1000u64;
    group.throughput(Throughput::Elements(count));

    let msgs: Vec<serde_json::Value> = (0..count)
        .map(|i| helpers::make_raw_onebot_v11_json(123456, i as i64))
        .collect();

    group.bench_function("1000_events", |b| {
        b.iter(|| {
            for msg in &msgs {
                let _ = black_box(onebot_v11::parse_event(msg.clone()));
            }
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_parse_small_message,
    bench_parse_1kb_message,
    bench_serialize_event,
    bench_serialize_1kb_event,
    bench_roundtrip,
    bench_parse_throughput,
);
criterion_main!(benches);
