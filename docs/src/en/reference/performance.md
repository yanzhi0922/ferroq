# Performance

ferroq is designed for extreme performance. This page summarizes benchmark results and design choices that enable sub-millisecond latency.

> Full benchmark data: [BENCHMARK.md](https://github.com/yanzhi0922/ferroq/blob/main/BENCHMARK.md)

## Key Numbers

| Metric | Measured | Target |
|--------|----------|--------|
| Full pipeline latency (small event) | **17.5 µs** | < 5 ms |
| Full pipeline latency (1 KB event) | **37.8 µs** | < 5 ms |
| Event bus throughput (small) | **2.16M msg/s** | 1K msg/s |
| End-to-end throughput (1 KB) | **63K msg/s** | 1K msg/s |
| Event parsing (OneBot v11) | **2.6 µs** | — |
| Dedup check | **670 ns** | — |
| Memory (idle) | **4.9 MB** | < 10 MB |
| Memory (100K events) | **10 MB** | < 30 MB |

## Why Is It Fast?

### Zero-Copy Parsing

ferroq uses `serde_json` with Rust's zero-copy deserialization where possible. OneBot v11 events are parsed directly from the raw JSON into strongly-typed `Event` structs without intermediate string copies.

### Lock-Free Event Bus

The event bus uses `tokio::sync::broadcast`, which is a multi-producer, multi-consumer channel. Publishing is O(1) regardless of subscriber count — adding 16 subscribers adds zero measurable overhead.

### Efficient Dedup

The deduplication filter uses a 128-bit fingerprint (not full event hash) with a `HashMap` for O(1) lookup. Lazy eviction avoids sweeping the map on every event.

### Single Binary

The release binary uses LTO (Link-Time Optimization), single codegen unit, and symbol stripping. This produces highly optimized native code with minimal binary size.

## Running Benchmarks

```bash
# Criterion benchmarks (event_bus, event_parse, dedup_filter, pipeline)
cargo bench -p ferroq-gateway

# Memory profile
cargo bench --bench memory_profile -p ferroq-gateway
```

HTML reports are generated in `target/criterion/report/index.html`.
