# 常见问题

## 基本问题

### ferroq 是什么？

ferroq 是一个用 Rust 编写的高性能 QQ Bot 协议网关。它位于 QQ 协议后端（如 Lagrange.OneBot 或 NapCat）和 Bot 框架（如 NoneBot2、Koishi、Yunzai）之间，提供统一的事件路由、多协议支持和可靠性功能。

### 为什么不直接连接 Lagrange/NapCat？

ferroq 在以下场景有价值：
- **多后端** — 同时连接 Lagrange 和 NapCat
- **故障转移** — 主后端和备用后端自动切换
- **多协议** — 从同一个后端同时提供 OneBot v11、v12、Satori
- **可观测性** — 健康检查、Prometheus 指标、Web 仪表盘
- **插件** — WASM 自定义消息处理
- **消息存储** — SQLite 消息持久化

如果只有一个后端和一个 Bot 框架，直接连接更简单。

### ferroq 实现了 QQ 协议吗？

没有。ferroq 是**网关/代理**，不是协议实现。它依赖 Lagrange.OneBot 或 NapCat 等后端处理实际的 QQ 协议。

## 性能

### ferroq 有多快？

端到端事件转发延迟约 17.5 µs（小事件）/ 37.8 µs（1KB 事件）。吞吐量超过 290K msg/s。空载内存 5 MB 以下。

### 内存占用多少？

- 空载：~5 MB RSS
- 处理 100K 事件后：~10 MB
- 缓冲 1K × 1KB 事件时：~13 MB

## 插件

### 可以用什么语言写插件？

任何编译到 WebAssembly 的语言。Rust 最自然，C、C++、AssemblyScript 等也可以。

### 插件能访问网络或文件系统吗？

不能。插件在沙箱化的 wasmtime 环境中运行，无 WASI 能力。

### 能关闭插件系统吗？

可以。不带 `wasm-plugins` feature 编译即可：
```bash
cargo build --release --no-default-features -p ferroq-gateway
```

## 运维

### 怎么不重启更新配置？

```bash
curl -X POST http://localhost:8080/api/reload \
  -H "Authorization: Bearer YOUR_TOKEN"
```

### 怎么运行时添加后端？

```bash
curl -X POST http://localhost:8080/api/accounts/add \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "backup", "backend": {"type": "napcat", "url": "ws://..."}}'
```

### 怎么监控 ferroq？

- **健康检查**：`GET /health`
- **Prometheus**：`GET /metrics`
- **仪表盘**：`GET /dashboard/`
- **日志**：通过 `tracing` 结构化日志
