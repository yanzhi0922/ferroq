# 竞品对照压测

本页说明如何用可复现流程对比 **ferroq** 与其他网关。

## 1. 目标

在同一台机器、同一请求模型、同一并发下对比：

- 吞吐（req/s）
- 错误率
- 延迟（p50 / p95 / p99）

## 2. 前置条件

- PowerShell 7+（`pwsh`）
- ferroq 和竞品都可通过 HTTP 访问
- 已配置鉴权 token（如果开启了鉴权）

## 3. 运行对照

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

脚本会生成 `COMPARE_BENCHMARK.md`。

## 4. 公平性检查清单

在对外发布性能结论前，请确认：

1. 同一台机器和相同 CPU 调频策略。
2. 同一接口和同一请求体。
3. 同一请求数 / 并发 / 超时时间。
4. 无无关高 CPU 后台任务干扰。
5. 至少跑 3 轮，取中位结果。

## 5. Stars 差距追踪

要追踪与竞品的 GitHub stars 和活跃度：

```bash
pwsh -File scripts/competitive_snapshot.ps1
```

该脚本会更新 `COMPETITOR_SNAPSHOT.md`。
