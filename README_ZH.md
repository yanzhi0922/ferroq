<div align="center">

# ⚡ ferroq

**高性能 QQ Bot 统一网关** — 纯 Rust 编写

[![CI](https://github.com/yanzhi0922/ferroq/actions/workflows/ci.yml/badge.svg)](https://github.com/yanzhi0922/ferroq/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/yanzhi0922/ferroq)](https://github.com/yanzhi0922/ferroq/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

*一个网关统一所有 — 连接任意 QQ 协议后端，服务任意 Bot 框架*

[English](README.md) | **简体中文**

</div>

---

## 什么是 ferroq？

**ferroq** 是一个高性能的 **QQ Bot 协议网关**，位于 QQ 协议后端（Lagrange、NapCat、官方 API）与 Bot 框架（NoneBot2、Koishi、云崽等）之间。

ferroq 不是又一个协议实现，而是协议实现之上的**统一代理/路由器**，提供：

- 🚀 **极致性能** — 异步 Rust，零拷贝消息转发，<1ms 额外延迟
- 🔄 **多协议支持** — OneBot v11（完整），OneBot v12 / Milky / Satori 计划中
- 🔌 **后端无关** — Lagrange.OneBot、NapCat — 热切换无需重启
- 📊 **内置仪表盘** — Web UI 监控适配器状态、每适配器事件/API 指标
- 🛡️ **高可靠** — 指数退避重连、健康检查、可配置超时
- 🔀 **故障转移** — 主/备适配器自动切换
- 🧹 **事件去重** — 时间窗口指纹过滤，防止故障转移产生重复
- 💾 **消息存储** — 可选 SQLite 消息持久化，支持搜索和分页
- 🔒 **安全** — Bearer / 查询参数鉴权，HMAC-SHA1 HTTP POST 签名，密钥脱敏
- ⚡ **热重载** — `POST /api/reload` 更新 access_token 和限流参数无需重启
- 📈 **可观测性** — Prometheus `/metrics`，每适配器事件/API 计数器，健康 API
- 🚦 **限流** — 全局令牌桶限流器，带 `Retry-After` 响应头，参数可热重载
- 📦 **单二进制** — 一个 `ferroq` 二进制，无运行时依赖，<15MB

## 架构

```
┌──────────────────────────────────────────────────────────┐
│                      Bot 框架                             │
│        NoneBot2 / Koishi / 云崽 / 自定义 Bot              │
└─────────────────────────┬────────────────────────────────┘
                          │  OneBot v11 / v12 / Milky / Satori
                          ▼
┌──────────────────────────────────────────────────────────┐
│                     ⚡ ferroq                             │
│  ┌──────────────┐  ┌──────────┐  ┌────────────────────┐ │
│  │ 协议服务器   │  │ 事件总线 │  │ 后端适配器         │ │
│  │ (入站)       │◄─┤          │◄─┤ (出站)             │ │
│  │              │  │          │  │                    │ │
│  │ • OneBot v11 │  │ broadcast│  │ • Lagrange WS      │ │
│  │ • OneBot v12 │  │ + route  │  │ • NapCat WS        │ │
│  │ • Milky      │  │          │  │ • 官方 API         │ │
│  │ • Satori     │  │          │  │                    │ │
│  └──────────────┘  └──────────┘  └────────────────────┘ │
│  ┌──────────┐  ┌────────────┐  ┌────────────────────┐   │
│  │ 仪表盘   │  │ 管理 API   │  │ 消息存储           │   │
│  │ (Web UI) │  │   (/api)   │  │ (SQLite)           │   │
│  └──────────┘  └────────────┘  └────────────────────┘   │
└──────────────────────────────────────────────────────────┘
                          │
                          ▼
┌──────────────────────────────────────────────────────────┐
│                   QQ 协议后端                             │
│     Lagrange.OneBot  /  NapCat  /  官方 Bot API          │
└──────────────────────────────────────────────────────────┘
```

## 快速开始

### 方式一：Docker（推荐）

```bash
# 创建配置文件
curl -LO https://raw.githubusercontent.com/yanzhi0922/ferroq/main/config.example.yaml
mv config.example.yaml config.yaml
# 编辑 config.yaml 配置你的后端地址

# 运行
docker run -d \
  --name ferroq \
  -p 8080:8080 \
  -v $(pwd)/config.yaml:/app/config.yaml:ro \
  -v $(pwd)/data:/app/data \
  ghcr.io/yanzhi0922/ferroq:latest
```

### 方式二：下载预编译二进制

从 [Releases](https://github.com/yanzhi0922/ferroq/releases) 下载对应平台的二进制文件：

```bash
# Linux x86_64
curl -LO https://github.com/yanzhi0922/ferroq/releases/latest/download/ferroq-linux-x86_64.tar.gz
tar xzf ferroq-linux-x86_64.tar.gz
chmod +x ferroq

# 生成默认配置
./ferroq --generate-config

# 编辑 config.yaml，然后启动
./ferroq
```

### 方式三：从源码构建

```bash
git clone https://github.com/yanzhi0922/ferroq.git
cd ferroq
cargo build --release
./target/release/ferroq --generate-config
./target/release/ferroq
```

## 配置示例

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  access_token: "your-secret-token"  # 可选，为空则不鉴权
  dashboard: true
  rate_limit:
    enabled: true
    requests_per_second: 100
    burst: 200

accounts:
  - name: "main"
    backend:
      type: lagrange  # 或 napcat
      url: "ws://127.0.0.1:8081/onebot/v11/ws"
      reconnect_interval: 5
      max_reconnect_interval: 120
      connect_timeout: 15
      api_timeout: 30
    # 可选：故障转移备用后端
    # fallback:
    #   type: napcat
    #   url: "ws://127.0.0.1:8082/onebot/v11/ws"

protocols:
  onebot_v11:
    enabled: true
    http: true         # HTTP API
    ws: true           # 正向 WebSocket
    ws_reverse: []     # 反向 WebSocket 目标
    http_post: []      # HTTP POST 上报目标

storage:
  enabled: false       # 启用消息持久化
  path: "./data/messages.db"
  max_days: 30

dedup:
  enabled: true        # 启用事件去重（故障转移时防重复）
  window_secs: 60

logging:
  level: info
  console: true
```

完整配置参考见 [config.example.yaml](config.example.yaml)。

## API 端点

| 端点 | 描述 |
|------|------|
| `GET /health` | 健康检查 — 返回运行时间、计数器、适配器快照 |
| `GET /metrics` | Prometheus 格式指标（每适配器事件/API 计数） |
| `GET /dashboard/` | 内嵌 Web 仪表盘 |
| `GET /api/accounts` | 列出所有已注册的后端适配器 |
| `POST /api/accounts/add` | 运行时动态添加适配器 |
| `POST /api/accounts/{name}/remove` | 移除适配器 |
| `POST /api/accounts/{name}/reconnect` | 重连指定适配器 |
| `GET /api/stats` | 完整运行时统计 |
| `GET /api/messages` | 查询存储的消息（支持过滤、分页） |
| `GET /api/config` | 查看当前配置（密钥已脱敏） |
| `POST /api/reload` | 热重载 access_token 和限流参数 |
| `POST /onebot/v11/api/:action` | OneBot v11 HTTP API |
| `WS /onebot/v11/ws` | OneBot v11 正向 WebSocket |

## 性能对比

| 指标 | ferroq | go-cqhttp | Overflow |
|------|--------|-----------|----------|
| 事件转发延迟 | <1ms | ~5ms | ~3ms |
| 内存占用（空闲） | ~8MB | ~30MB | ~50MB |
| 二进制大小 | ~15MB | ~25MB | ~40MB |
| 并发连接数 | 10,000+ | ~1,000 | ~500 |

## 与框架集成

### NoneBot2

```python
# .env
ONEBOT_WS_URLS=["ws://127.0.0.1:8080/onebot/v11/ws"]
ONEBOT_ACCESS_TOKEN=your-secret-token
```

### Koishi

```yaml
plugins:
  adapter-onebot:
    bots:
      - protocol: ws
        endpoint: ws://127.0.0.1:8080/onebot/v11/ws
        token: your-secret-token
```

## 常见问题

### Q: ferroq 和 Lagrange/NapCat 是什么关系？

ferroq 是一个**网关**，它连接到 Lagrange 或 NapCat 作为后端。你仍然需要运行 Lagrange 或 NapCat 来处理实际的 QQ 协议，ferroq 负责：
- 统一多个后端
- 提供故障转移
- 事件去重
- 消息持久化
- 监控仪表盘

### Q: 可以同时连接多个账号吗？

可以。在 `accounts` 配置中添加多个账号，每个账号对应一个后端连接。

### Q: 后端挂了会怎样？

ferroq 会自动重连（指数退避，最大 120 秒）。如果配置了 `fallback`，会自动切换到备用后端。

### Q: 如何更新 ferroq？

```bash
# Docker
docker pull ghcr.io/yanzhi0922/ferroq:latest
docker compose up -d

# 二进制
# 下载新版本替换旧文件即可
```

## 开发路线

- [x] OneBot v11 完整支持
- [x] 故障转移 + 事件去重
- [x] 动态适配器管理 API
- [x] Web 仪表盘
- [x] Prometheus 指标
- [ ] OneBot v12 支持
- [ ] Milky / Satori 协议
- [ ] WASM 插件系统

## 贡献

欢迎提交 Issue 和 Pull Request！

## 许可证

[MIT](LICENSE)

---

<div align="center">

**⚡ 用 Rust 构建，追求极致性能 ⚡**

</div>
