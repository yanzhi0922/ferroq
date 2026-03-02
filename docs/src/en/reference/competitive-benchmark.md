# Competitive Benchmarking

This page describes how to compare **ferroq** with other gateways using a
reproducible process.

## 1. Objective

Use the same host, same request model, and same concurrency to compare:

- throughput (req/s)
- error rate
- latency (p50 / p95 / p99)

## 2. Prerequisites

- PowerShell 7+ (`pwsh`)
- ferroq and competitor are both reachable over HTTP
- valid tokens (if auth is enabled)

## 3. Run comparison

```bash
pwsh -File scripts/compare_gateways.ps1 \
  -CompetitorName "NapCat" \
  -CompetitorUrl "http://127.0.0.1:3000/onebot/v11/api/get_login_info" \
  -FerroqUrl "http://127.0.0.1:8080/onebot/v11/api/get_login_info" \
  -FerroqToken "your-token" \
  -CompetitorToken "your-token" \
  -Requests 5000 \
  -Concurrency 200
```

The script writes `COMPARE_BENCHMARK.md`.

## 4. Fairness checklist

Before publishing benchmark numbers, verify:

1. Same machine and CPU governor.
2. Same endpoint and request payload.
3. Same request count / concurrency / timeout.
4. No unrelated high-CPU tasks running in background.
5. Retry at least 3 runs and report median.

## 5. Star-gap tracking

To track GitHub stars and activity against competitors:

```bash
pwsh -File scripts/competitive_snapshot.ps1
```

This updates `COMPETITOR_SNAPSHOT.md`.
