# Benchmark Results

> ferroq v1.0.0-rc — High-performance QQ Bot unified gateway
>
> **Platform**: Windows 11, AMD/Intel x86_64, Rust 1.85 stable
> **Profile**: `release` (LTO enabled, single codegen unit, stripped)
> **Tool**: [Criterion.rs](https://github.com/bheisler/criterion.rs) v0.5

---

## Summary

| Metric | Target (v1.0.0-rc) | Measured | Status |
|--------|-----------------|----------|--------|
| Event bus publish (1 sub) | — | **309 ns** | ✅ |
| Full pipeline latency (parse→dedup→bus→serialize) | < 5 ms p99 | **17.5 µs** | ✅ ~285× under budget |
| Full pipeline latency (1 KB payload) | < 5 ms p99 | **37.8 µs** | ✅ ~132× under budget |
| Throughput (small events) | 1k msg/s | **2.16M msg/s** | ✅ 2160× over target |
| Throughput (1 KB events) | 1k msg/s | **384K msg/s** | ✅ 384× over target |
| End-to-end throughput (1 KB, parse→serialize) | 1k msg/s | **62K msg/s** | ✅ 62× over target |
| Memory — idle (core components) | < 10 MB | **4.9 MB** | ✅ |
| Memory — 100k events processed | < 30 MB | **10.0 MB** | ✅ |
| Memory — 1k×1KB events buffered | — | **12.7 MB** | ✅ |

---

## 0. Latest Optimization Pass (2026-03-03)

The table below is an A/B rerun on the same host after parser + dedup hot-path
optimizations (criterion benches, `ferroq-gateway`).

| Benchmark | Before | After | Delta |
|-----------|--------|-------|-------|
| `parse/onebot_v11_small` | 2.2107 µs | 1.9512 µs | **-11.7%** |
| `parse/onebot_v11_1kb` | 8.5115 µs | 6.5880 µs | **-22.6%** |
| `roundtrip/parse_then_serialize` | 3.4925 µs | 3.1596 µs | **-9.5%** |
| `parse/throughput/1000_events` | 400.03 K msg/s | 452.89 K msg/s | **+13.2%** |
| `dedup/duplicate_event` | 574.91 ns | 571.22 ns | **-0.6%** |
| `dedup/throughput/1000_unique` | 1.4532 M msg/s | 1.5040 M msg/s | **+3.5%** |

These gains come from:

- removing avoidable `serde_json::Value` clones in the OneBot v11 parser
- reducing lock contention in dedup eviction logic (atomic eviction timestamp)
- switching WS outbound paths to bounded queues to prevent unbounded memory growth
- adding WS overload observability counters (`ws_events_dropped`, `ws_api_rejected`)

---

## 1. Event Bus

The event bus uses `tokio::sync::broadcast` to fan-out events from backend adapters to all protocol server subscribers.

| Benchmark | Time | Throughput |
|-----------|------|------------|
| `bus/publish_no_sub` | 162 ns | — |
| `bus/publish_to_N_sub/1` | 309 ns | — |
| `bus/publish_to_N_sub/4` | 319 ns | — |
| `bus/publish_to_N_sub/16` | 306 ns | — |
| `bus/publish_receive_latency` | 40.8 µs | — |
| `bus/throughput_small` (1000 events) | 464 µs | **2.16M msg/s** |
| `bus/throughput_1kb` (1000 × 1KB) | 2.54 ms | **393K msg/s** |
| `bus/throughput_async_drain` (1000 events) | 817 µs | **1.22M msg/s** |

**Key takeaway**: Publishing scales O(1) with subscriber count — the broadcast channel clones the `Arc` internally, not the event payload. Throughput exceeds 2M msg/s for small events and ~400K msg/s for 1KB payloads.

---

## 2. Event Parsing (OneBot v11)

Measures the cost of converting raw JSON from a backend into internal `Event` types.

| Benchmark | Time | Throughput |
|-----------|------|------------|
| `parse/onebot_v11_small` | 2.64 µs | — |
| `parse/onebot_v11_1kb` | 9.53 µs | — |
| `serialize/event_to_json` | 822 ns | — |
| `serialize/event_1kb_to_json` | 1.94 µs | — |
| `roundtrip/parse_then_serialize` | 3.73 µs | — |
| `parse/throughput` (1000 events) | 2.62 ms | **382K msg/s** |

**Key takeaway**: A full parse + serialize round-trip is under 4 µs for typical messages, meaning the serialization layer alone can sustain ~250K msg/s.

---

## 3. Deduplication Filter

The dedup filter uses a time-windowed fingerprint map with lazy eviction.

| Benchmark | Time | Throughput |
|-----------|------|------------|
| `dedup/unique_event` (cache miss) | 674 ns | — |
| `dedup/duplicate_event` (cache hit) | 417 ns | — |
| `dedup/throughput` (1000 unique) | 535 µs | **1.87M msg/s** |
| `dedup/warm_cache/100` entries | 693 ns | — |
| `dedup/warm_cache/1000` entries | 667 ns | — |
| `dedup/warm_cache/10000` entries | 662 ns | — |

**Key takeaway**: dedup latency is ~670 ns per event and remains constant regardless of cache size (100 → 10K entries show no degradation). This is because the fingerprint is a 128-bit hash used as a `HashMap` key — O(1) lookup.

---

## 4. End-to-End Pipeline

Full message forwarding path: **raw JSON → parse → dedup → event bus → subscriber receive → serialize to JSON**.

| Benchmark | Time | Throughput |
|-----------|------|------------|
| `pipeline/full_latency` (small) | **17.5 µs** | — |
| `pipeline/full_latency_1kb` (1KB) | **37.8 µs** | — |
| `pipeline/throughput` (1000 small) | 3.44 ms | **291K msg/s** |
| `pipeline/throughput_1kb` (1000 × 1KB) | 15.7 ms | **63.8K msg/s** |

**Key takeaway**: The full forwarding latency of **17.5 µs** (small) / **37.8 µs** (1KB) is ~285× / ~132× under the v0.1.0 target of 5 ms p99. Even at 1KB per message, end-to-end throughput is 63K msg/s — 63× above the 1K msg/s target.

---

## 5. Memory Profile

Measures working-set (RSS) growth as the gateway processes events.

| Stage | RSS | Delta |
|-------|-----|-------|
| Baseline (process start) | 4.24 MB | — |
| Core init (EventBus + DedupFilter + Stats) | 4.89 MB | +668 KB |
| 4 protocol server subscribers | 4.89 MB | +4 KB |
| After 1K events processed | 5.04 MB | +152 KB |
| After 10K events processed | 5.84 MB | +812 KB |
| After 100K events processed | 9.96 MB | +4.13 MB |
| 1K × 1KB events buffered in bus | 12.69 MB | +2.72 MB |

**Key takeaway**: Idle memory is **4.9 MB** (well under the 10 MB target). Even after processing 100K events and buffering 1K×1KB messages, RSS is only 12.7 MB — comfortably under the 30 MB target for 100-group active usage.

---

## 6. Official Adapter (HTTP) — 2026-03-03

Benchmark target: API round-trip cost of the new `official` HTTP adapter with a local mock backend.

| Benchmark | Time |
|-----------|------|
| `official_http/call_api_get_login_info` | **79.8–84.5 µs** |
| `official_http/call_api_get_status` | **84.3–92.7 µs** |

**Key takeaway**: the dedicated `official` adapter is now implemented and benchmarked; single-call overhead is sub-100µs in local loopback tests.

---

## 7. vs. Node.js `onebots`

A direct apples-to-apples comparison is not yet available (requires running both systems on the same hardware with identical workloads). However, based on architectural analysis:

| Dimension | ferroq (Rust) | onebots (Node.js) |
|-----------|---------------|-------------------|
| Event parsing | 2.6 µs (zero-copy serde) | ~50–200 µs (JSON.parse + object spread) |
| Throughput | 291K msg/s (end-to-end) | ~5–20K msg/s (V8 single-threaded) |
| Memory (idle) | 4.9 MB | ~40–80 MB (V8 heap) |
| Memory (active) | ~13 MB | ~100–200 MB |
| Binary size | ~8 MB (stripped) | ~60 MB (node_modules) |
| Startup time | < 100 ms | ~1–3 s |

> *Node.js estimates based on typical V8 overhead and community benchmarks. Actual onebots numbers will be published in a future comparative benchmark.*

---

## Running Benchmarks

```sh
# All benchmarks
cargo bench -p ferroq-gateway

# Individual benchmark suites
cargo bench --bench event_bus -p ferroq-gateway
cargo bench --bench event_parse -p ferroq-gateway
cargo bench --bench dedup_filter -p ferroq-gateway
cargo bench --bench pipeline -p ferroq-gateway
cargo bench --bench official_adapter -p ferroq-gateway

# Memory profile (standalone, not criterion)
cargo bench --bench memory_profile -p ferroq-gateway
```

HTML reports are generated in `target/criterion/` — open `target/criterion/report/index.html` in a browser.

---

## Reproducing

1. Ensure Rust 1.85+ stable is installed
2. Clone the repository
3. Run `cargo bench -p ferroq-gateway`
4. Results vary by hardware — the numbers above were measured on a development machine

---

*Last updated: 2026-03-03*
