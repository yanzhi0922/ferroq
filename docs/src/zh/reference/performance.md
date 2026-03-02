# 性能基准

ferroq 以极致性能为设计目标。本页总结基准测试结果。

> 完整数据：[BENCHMARK.md](https://github.com/yanzhi0922/ferroq/blob/main/BENCHMARK.md)

## 核心指标

| 指标 | 实测值 | 目标 |
|------|--------|------|
| 全链路延迟（小事件） | **17.5 µs** | < 5 ms |
| 全链路延迟（1KB 事件） | **37.8 µs** | < 5 ms |
| 事件总线吞吐量 | **2.16M msg/s** | 1K msg/s |
| 端到端吞吐量（1KB） | **63K msg/s** | 1K msg/s |
| 事件解析（OneBot v11） | **2.6 µs** | — |
| 去重检查 | **670 ns** | — |
| 内存（空载） | **4.9 MB** | < 10 MB |
| 内存（100K 事件后） | **10 MB** | < 30 MB |

## 为什么这么快？

### 零拷贝解析

ferroq 使用 `serde_json` 配合 Rust 的零拷贝反序列化，将 OneBot v11 事件直接从原始 JSON 解析为强类型 `Event` 结构体，无需中间字符串拷贝。

### 无锁事件总线

事件总线使用 `tokio::sync::broadcast`，发布操作 O(1)，不受订阅者数量影响 — 16 个订阅者零可测开销。

### 高效去重

去重过滤器使用 128 位指纹（非全事件哈希）在 `HashMap` 中 O(1) 查找，配合惰性清理。

### 单二进制优化

Release 构建使用 LTO、单代码生成单元、符号剥离，产出高度优化的原生代码。

## 运行基准测试

```bash
# Criterion 基准测试
cargo bench -p ferroq-gateway

# 内存分析
cargo bench --bench memory_profile -p ferroq-gateway
```

HTML 报告生成在 `target/criterion/report/index.html`。
