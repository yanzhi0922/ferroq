# ferroq — 高性能 QQ Bot 统一网关

> ferro = 拉丁语"铁"，呼应 Rust（铁锈）; q = QQ。
> **不是又一个协议实现，而是协议实现之上的高性能基础设施。**

---

## 0. 核心定位

```
                          ferroq 在生态中的位置

  ┌──────────┐  ┌──────────┐  ┌──────────┐
  │ NoneBot  │  │ Koishi   │  │ 你的Bot  │    ← 上层：Bot 框架 / 用户应用
  └────┬─────┘  └────┬─────┘  └────┬─────┘
       │              │             │
       │    OneBot v11 / v12 / Milky / Satori
       │              │             │
  ╔════╧══════════════╧═════════════╧════════╗
  ║              f e r r o q                 ║  ← 我们：统一网关
  ║  多后端 · 多账号 · 多协议 · 高性能       ║
  ╚════╤══════════════╤═════════════╤════════╝
       │              │             │
  ┌────┴────┐  ┌──────┴─────┐  ┌───┴──────┐
  │Lagrange │  │  NapCat    │  │ 官方 Bot │    ← 下层：协议后端（不由我们维护）
  │ V2 (WS) │  │  (WS/HTTP) │  │   API    │
  └─────────┘  └────────────┘  └──────────┘
```

**一句话描述**：把多个 QQ Bot 后端统一成一个高性能、单二进制、零配置的网关服务。类比 nginx 之于后端服务。

---

## 1. 为什么这个项目会拿 Stars

### 当前生态痛点

| 痛点 | 影响范围 | 现有解法 |
|------|---------|---------|
| go-cqhttp 归档，没有"下载即用"的替代 | 所有 QQ bot 开发者 | 手动配 Lagrange/NapCat，门槛高 |
| Lagrange.OneBot 需 .NET 运行时 | VPS 用户 | Docker（但 512MB 内存机器跑不起来） |
| NapCat 需安装 QQ 客户端，吃 300MB+ 内存 | 低配服务器用户 | 无 |
| 多账号需跑多个进程 | 群组/社区管理者 | 写 shell 脚本管理多进程 |
| 后端挂了没有故障转移 | 需要高可用的场景 | 无 |
| OneBot v11 / v12 / Milky / Satori 割裂 | 框架/插件开发者 | 每个协议写一套适配 |

### ferroq 的卖点

| 特性 | 对比现有方案 |
|------|-------------|
| **单二进制** | `curl -LO && chmod +x && ./ferroq` — go-cqhttp 体验回来了 |
| **< 20MB 内存** | 比 NapCat (300MB+) 或 Lagrange Docker (100MB+) 少一个数量级 |
| **多后端** | 同时接 Lagrange + NapCat + 官方API，一个挂了自动切换 |
| **多账号** | 一个进程管 N 个 QQ 账号 |
| **多协议输出** | 同一个后端同时暴露 OneBot v11 + v12 + Milky + Satori |
| **Web Dashboard** | 浏览器配置，不用改 YAML 文件 |
| **消息持久化** | 内置 SQLite，支持历史消息查询 |
| **WASM 插件** | 任何语言写插件（Rust/Go/JS/Python→WASM） |
| **10k+ msg/s** | tokio 异步 I/O，比 Node.js/Python 网关快 10 倍+ |

### 竞品分析

| 项目 | 语言 | 定位 | 弱点 |
|------|------|------|------|
| `onebots` | Node.js | 多平台多协议启动器 | 26 downloads/week，性能差，Node 依赖 |
| `overflow` | Kotlin | Mirai → OneBot 桥接 | 只支持 Mirai 一个后端 |
| Lagrange.OneBot | C# | 协议实现 + OneBot | 只支持自己的协议实现 |
| **ferroq** | **Rust** | **纯网关层** | **我们：零协议风险、极致性能** |

---

## 2. 技术栈

```toml
[workspace]
resolver = "2"
members = [
    "crates/ferroq-core",      # 核心数据类型 + trait 定义
    "crates/ferroq-gateway",    # 网关主逻辑
    "crates/ferroq-web",        # Web Dashboard
    "crates/ferroq",            # 主二进制
]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "MIT"

[workspace.dependencies]
# === 异步运行时 ===
tokio        = { version = "1.48", features = ["full"] }
futures      = "0.3"
tokio-util   = "0.7"

# === 网络 / WebSocket ===
tokio-tungstenite = { version = "0.26", features = ["rustls-tls-webpki-roots"] }
reqwest           = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }
hyper             = { version = "1", features = ["full"] }

# === Web 框架 ===
axum             = { version = "0.8", features = ["ws"] }
axum-extra       = { version = "0.10", features = ["typed-header"] }
tower            = "0.5"
tower-http       = { version = "0.6", features = ["cors", "fs", "compression-gzip", "trace"] }

# === 序列化 ===
serde            = { version = "1", features = ["derive"] }
serde_json       = "1"
serde_yaml       = "0.9"
toml             = "0.8"

# === 数据存储 ===
rusqlite         = { version = "0.32", features = ["bundled"] }  # SQLite 消息持久化
dashmap          = "6"

# === WASM 插件 ===
wasmtime         = "29"       # WASM 插件运行时

# === 配置 / CLI ===
clap             = { version = "4", features = ["derive"] }
config           = "0.14"

# === 日志 ===
tracing            = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender   = "0.2"

# === 错误处理 ===
anyhow           = "1"
thiserror        = "2"

# === 工具 ===
bytes            = { version = "1.11", features = ["serde"] }
chrono           = { version = "0.4", features = ["serde"] }
uuid             = { version = "1.18", features = ["serde", "v4"] }
rand             = "0.9"
arc-swap         = { version = "1.7", features = ["serde"] }
once_cell        = "1"
rust-embed       = "8"       # 嵌入 Web UI 静态资源到二进制
url              = "2"
regex            = "1"
parking_lot      = "0.12"

[profile.release]
opt-level = 2
lto = true
codegen-units = 1
strip = true
incremental = false
```

### 选型要点

| 选择 | 理由 |
|------|------|
| `tokio-tungstenite` 而非 `axum::ws` 做出站连接 | 需要作为 WS **客户端**连接后端 |
| `rusqlite` bundled | 零依赖编译 SQLite，musl 友好 |
| `wasmtime` | WASM 插件生态最成熟的 Rust 运行时，支持 WASI |
| `rust-embed` | 把前端 dist/ 编译进二进制，真正单文件分发 |
| MIT license | 网关类工具用 MIT 比 GPL 更容易被接受 |

---

## 3. 架构

```
┌──────────────────────────── ferroq 进程 ─────────────────────────────┐
│                                                                      │
│  ┌─────────────────── Inbound (对外暴露) ───────────────────────┐    │
│  │                                                               │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐ │    │
│  │  │ OneBot   │  │ OneBot   │  │  Milky   │  │   Satori     │ │    │
│  │  │  v11     │  │  v12     │  │    v1    │  │    v1        │ │    │
│  │  │ HTTP+WS  │  │ HTTP+WS  │  │ HTTP+WS  │  │  HTTP+WS    │ │    │
│  │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────┬───────┘ │    │
│  │       └──────────────┼─────────────┼───────────────┘         │    │
│  └──────────────────────┼─────────────┼─────────────────────────┘    │
│                         │             │                              │
│                    ┌────┴─────────────┴────┐                         │
│                    │       EventBus        │ ← tokio::broadcast      │
│                    │   (统一内部事件格式)    │                         │
│                    └────┬─────────────┬────┘                         │
│                         │             │                              │
│  ┌──────────────────────┼─────────────┼──────────────────────────┐   │
│  │             中间件层  │             │                          │   │
│  │  ┌──────────┐  ┌─────┴────┐  ┌─────┴──────┐  ┌────────────┐ │   │
│  │  │ 消息持久 │  │ WASM     │  │  限流 /    │  │  消息格式  │ │   │
│  │  │ 化(SQLite)│  │ 插件引擎 │  │  去重      │  │  转换      │ │   │
│  │  └──────────┘  └──────────┘  └────────────┘  └────────────┘ │   │
│  └──────────────────────┼─────────────┼──────────────────────────┘   │
│                         │             │                              │
│  ┌──────────────────────┼─────────────┼──────────────────────────┐   │
│  │  Outbound (对下连接)  │             │           路由器          │   │
│  │                ┌──────┴─────────────┴──────┐                  │   │
│  │                │     BackendRouter          │                  │   │
│  │                │  (健康检查 + 故障转移)      │                  │   │
│  │                └──┬──────────┬──────────┬───┘                  │   │
│  │  ┌────────────────┴┐  ┌─────┴────────┐ │  ┌───────────────┐  │   │
│  │  │ LagrangeAdapter │  │ NapCatAdapter │ │  │ OfficialAPI   │  │   │
│  │  │   (WS Client)   │  │ (WS Client)  │ │  │   Adapter     │  │   │
│  │  └─────────────────┘  └──────────────┘ │  └───────────────┘  │   │
│  └────────────────────────────────────────┘──────────────────────┘   │
│                                                                      │
│  ┌─────────── Web Dashboard (rust-embed) ───────────────────────┐   │
│  │  账号管理 · 后端状态 · 消息日志 · 配置编辑 · 性能监控         │   │
│  └──────────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────────┘
```

### 关键 trait 设计

```rust
/// 下游后端适配器（连接 Lagrange/NapCat/官方API）
#[async_trait]
pub trait BackendAdapter: Send + Sync + 'static {
    /// 适配器名称，如 "lagrange", "napcat"
    fn name(&self) -> &str;

    /// 连接后端
    async fn connect(&mut self) -> Result<()>;

    /// 发送 API 调用
    async fn call_api(&self, action: &str, params: serde_json::Value) -> Result<ApiResponse>;

    /// 接收事件流
    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>>;

    /// 健康检查
    async fn health_check(&self) -> bool;

    /// 断线重连
    async fn reconnect(&mut self) -> Result<()>;
}

/// 上游协议输出（暴露给 Bot 框架）
#[async_trait]
pub trait ProtocolServer: Send + Sync + 'static {
    /// 协议名称，如 "onebot_v11"
    fn name(&self) -> &str;

    /// 启动服务（HTTP / WS）
    async fn start(&self, event_rx: broadcast::Receiver<Event>) -> Result<()>;

    /// 处理来自客户端的 API 请求，路由到 BackendAdapter
    async fn handle_api(&self, request: ApiRequest) -> Result<ApiResponse>;
}

/// WASM 插件接口
pub trait Plugin: Send + Sync {
    /// 在事件到达上游之前处理（可修改/过滤/扩充）
    fn on_event(&self, event: &mut Event) -> PluginResult;

    /// 在 API 请求到达后端之前处理
    fn on_api_call(&self, action: &str, params: &mut serde_json::Value) -> PluginResult;
}
```

---

## 4. 目录结构

```
ferroq/
├── crates/
│   ├── ferroq-core/               # 核心类型 + trait 定义（无 IO）
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── event.rs           # 统一内部事件类型（OneBot v11 超集）
│   │   │   ├── api.rs             # 统一 API 请求/响应类型
│   │   │   ├── message.rs         # 消息段（CQ 码 / 数组格式统一抽象）
│   │   │   ├── adapter.rs         # BackendAdapter trait
│   │   │   ├── protocol.rs        # ProtocolServer trait
│   │   │   ├── plugin.rs          # Plugin trait + WASM ABI 定义
│   │   │   ├── config.rs          # 配置结构体
│   │   │   └── error.rs           # thiserror 错误类型
│   │   └── Cargo.toml
│   │
│   ├── ferroq-gateway/            # 网关主逻辑
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── bus.rs             # EventBus (broadcast channel)
│   │   │   ├── router.rs          # BackendRouter (负载均衡 + 故障转移)
│   │   │   ├── store.rs           # SQLite 消息持久化
│   │   │   ├── plugin_engine.rs   # WASM 插件加载与执行
│   │   │   ├── rate_limit.rs      # 限流器
│   │   │   ├── dedup.rs           # 消息去重
│   │   │   ├── backend/           # 后端适配器实现
│   │   │   │   ├── mod.rs
│   │   │   │   ├── lagrange.rs    # LagrangeV2 OneBot WS 客户端
│   │   │   │   ├── napcat.rs      # NapCat WS/HTTP 客户端
│   │   │   │   ├── official.rs    # QQ 官方 Bot API 客户端
│   │   │   │   └── mock.rs        # Mock 后端（测试/开发用）
│   │   │   ├── inbound/           # 对外协议服务
│   │   │   │   ├── mod.rs
│   │   │   │   ├── onebot_v11.rs  # OneBot v11 HTTP + WS
│   │   │   │   ├── onebot_v12.rs  # OneBot v12 HTTP + WS
│   │   │   │   ├── milky.rs       # Milky 协议
│   │   │   │   └── satori.rs      # Satori 协议
│   │   │   └── account.rs         # 多账号管理器
│   │   └── Cargo.toml
│   │
│   ├── ferroq-web/                # Web Dashboard（可选编译）
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── routes.rs          # REST API（账号/后端/配置 CRUD）
│   │   │   └── ws_log.rs          # 实时日志 WebSocket 推送
│   │   ├── frontend/              # 前端（Vue / Solid / 纯 HTML 均可）
│   │   │   ├── index.html
│   │   │   ├── dashboard.js
│   │   │   └── style.css
│   │   └── Cargo.toml
│   │
│   └── ferroq/                    # 主二进制 crate
│       ├── src/
│       │   └── main.rs            # CLI 入口 + 启动编排
│       └── Cargo.toml
│
├── plugins/                       # 示例 WASM 插件
│   └── echo/                      # 最简复读插件
│       ├── src/lib.rs
│       └── Cargo.toml
│
├── config.example.yaml            # 配置示例
├── Cargo.toml                     # workspace
├── PLAN.md                        # 本文件
├── BENCHMARK.md                   # 性能测试结果（发布时用）
└── README.md
```

---

## 5. 配置文件设计

```yaml
# config.yaml — ferroq 配置
server:
  host: "0.0.0.0"
  port: 8080                    # 主端口（API + WS + Dashboard）
  access_token: ""              # 全局鉴权 token
  dashboard: true               # 是否启用 Web Dashboard

# 多账号配置 — 每个账号绑定一个后端
accounts:
  - name: "主号"
    backend:
      type: lagrange             # lagrange | napcat | official
      url: "ws://127.0.0.1:8081" # 后端 OneBot WS 地址
      access_token: ""
      reconnect_interval: 5      # 秒
      health_check_interval: 30  # 秒

  - name: "小号"
    backend:
      type: napcat
      url: "ws://127.0.0.1:8082"
      access_token: ""

  # 同一账号多后端（故障转移）
  - name: "高可用号"
    backend:
      type: lagrange
      url: "ws://127.0.0.1:8083"
    fallback:
      type: napcat
      url: "ws://127.0.0.1:8084"

# 对外暴露协议
protocols:
  onebot_v11:
    enabled: true
    http: true
    ws: true                     # 正向 WS
    ws_reverse:                  # 反向 WS
      - url: "ws://127.0.0.1:9000/ws"
        access_token: ""
    http_post:                   # HTTP 上报
      - url: "http://127.0.0.1:9000/post"
        secret: ""

  onebot_v12:
    enabled: false

  milky:
    enabled: false

  satori:
    enabled: false

# 消息存储
storage:
  enabled: true
  path: "./data/messages.db"
  max_days: 30                   # 保留天数

# 插件
plugins:
  - path: "./plugins/echo.wasm"
    enabled: true

# 日志
logging:
  level: "info"                  # trace | debug | info | warn | error
  file: "./logs/ferroq.log"
  max_size_mb: 50
  console: true
```

---

## 6. 实现路线图

### Phase 1 — 骨架 + 最小转发（2-3 周）→ 发布 v0.1.0 ✅

目标：能转发消息，跑起来就能用。**发到 GitHub 开始收 stars。**

- [x] **P1.1 workspace 脚手架**
  - 创建 4 个 crate 骨架
  - `rust-toolchain.toml`（stable 1.85+）
  - `.rustfmt.toml` + `clippy.toml` + `.github/workflows/ci.yml`
  - `cargo build` 通过

- [x] **P1.2 核心类型（ferroq-core）**
  - `Event` 枚举（消息/通知/请求/元事件）
  - `ApiRequest` / `ApiResponse` 结构体
  - `MessageSegment` 类型（text / image / at / face / reply / ...）
  - `BackendAdapter` trait
  - `ProtocolServer` trait
  - 配置结构体 + YAML 解析
  - 错误类型

- [x] **P1.3 Lagrange 后端适配器**
  - 实现 `BackendAdapter` for `LagrangeAdapter`
  - 作为 WebSocket **客户端** 连接 Lagrange.OneBot 正向 WS
  - 接收事件 → 解析为内部 `Event`
  - 转发 API 调用到后端
  - 自动重连（指数退避 1s → 60s）
  - 健康检查（WebSocket ping/pong）

- [x] **P1.4 OneBot v11 入站协议**
  - HTTP Server（axum）
    - `POST /send_msg`、`POST /send_group_msg`、`POST /send_private_msg`
    - `GET /get_login_info`、`GET /get_status`
    - 鉴权 middleware（access_token）
  - 正向 WebSocket `/ws`
    - 事件推送 + API 调用
  - 反向 WebSocket 客户端
    - 连接上游，推送事件
  - HTTP POST 上报

- [x] **P1.5 CLI + 集成**
  - `ferroq --config config.yaml` 启动
  - `ferroq --generate-config` 生成默认配置
  - 启动时打印 ASCII banner + 连接状态
  - graceful shutdown (Ctrl+C)

- [x] **P1.6 README + CI/CD**
  - README：架构图 + 功能对比表 + 一键安装命令 + 截图
  - GitHub Actions：test → build → release（Linux/macOS/Windows 三平台）
  - `cargo install ferroq` 支持

**v0.1.0 发布物**：
- GitHub Release 附带三平台预编译二进制
- Docker 镜像（alpine based, < 20MB）
- README 有 GIF 演示

---

### Phase 2 — 差异化功能（3-4 周）→ v0.5.0

- [~] **P2.1 多后端支持** (Lagrange + NapCat 完成，官方 Bot API 待做)
  - NapCat 适配器（OneBot WS 协议基本相同，适配差异字段）
  - QQ 官方 Bot API 适配器（HTTP API，需要不同的事件映射）
  - 后端路由器：按账号路由、按优先级路由

- [x] **P2.2 多账号管理**
  - 多个账号 → 多个后端连接 → 统一 EventBus
  - API 调用时通过 `self_id` 参数路由到正确后端
  - 账号动态增删（运行时 API，不重启）

- [x] **P2.3 故障转移**
  - 主后端断线 → 自动切换到 fallback
  - 主后端恢复 → 自动切回
  - 健康检查 + 状态上报

- [x] **P2.4 消息持久化**
  - 收到的消息写入 SQLite
  - `GET /get_msg` 支持从本地 DB 查询（后端通常不存历史消息）
  - `GET /get_group_msg_history` 本地实现
  - 自动清理过期数据

- [x] **P2.5 Web Dashboard**
  - 极简前端（纯 HTML + JS，不用重型框架）
  - 通过 `rust-embed` 编译进二进制
  - 页面：
    - 总览（账号列表、后端状态、消息吞吐量图表）
    - 消息日志（实时滚动 + 搜索）
    - 配置编辑（在线修改 YAML，热重载）
    - 后端管理（添加/移除/重连）

- [x] **P2.6 Docker + 一键部署**
  - `docker run -v ./config.yaml:/etc/ferroq/config.yaml ghcr.io/xxx/ferroq`
  - docker-compose 样例（ferroq + Lagrange 一起跑）

---

### Phase 3 — 护城河（4-6 周）→ v1.0.0

- [x] **P3.1 WASM 插件系统**
  - 定义 `Plugin` WASI 接口
  - wasmtime 沙箱加载
  - 示例插件：
    - `echo.wasm` — 复读机
    - `keyword_reply.wasm` — 关键词自动回复
    - `rate_limiter.wasm` — 频率限制
  - 插件可通过 Dashboard 管理（上传/启停）

- [~] **P3.2 更多协议输出**
  - OneBot v12 实现 ✅
  - Milky 协议实现（LagrangeV2 的新协议）— 缺乏公开文档，暂跳过
  - Satori 协议实现 ✅

- [x] **P3.3 性能工程** ✅
  - 专项 benchmark 套件：
    - 消息转发延迟（p50/p95/p99）
    - 吞吐量（msg/s at 1KB payload）
    - 内存占用 profile
  - 发布 BENCHMARK.md（附图表）
  - 与 Node.js 方案 `onebots` 做对比测试

- [ ] **P3.4 文档站**
  - mdbook 或 VitePress
  - 快速开始、配置参考、插件开发指南、API 参考
  - 中英文双语

---

## 7. 代码规范

```
✅ 所有 pub 项必须有 `///` 文档注释
✅ 错误消息用英文
✅ 禁止 unwrap() / expect()（测试代码除外）
✅ CI：cargo fmt --check && cargo clippy -- -D warnings && cargo test
✅ 日志统一 tracing 宏（trace!/debug!/info!/warn!/error!）
✅ 每个模块配套 #[cfg(test)] mod tests
✅ unsafe 代码需附 // SAFETY: 注释
✅ trait 设计优先于具体类型：BackendAdapter、ProtocolServer 等通过 trait object 分发
✅ 配置变更支持热重载（SIGHUP / API 触发）
```

---

## 8. Star 增长策略

这不是"写完放着"的项目。Stars 需要经营。

### 发布时（v0.1.0）
- [ ] V2EX "分享创造" 版块发帖
- [ ] QQ Bot 开发者群/Telegram 群宣传
- [ ] Reddit r/rust 发帖（英文）
- [ ] Hacker News Show HN（英文）
- [ ] README 必备元素：
  - Logo / Banner 图
  - 一行安装命令
  - 功能对比表（vs. 直接用 Lagrange/NapCat）
  - 架构图（ASCII art）
  - GIF 演示
  - Badge（CI status, crate version, license, stars）

### 持续运营
- [ ] 每周更新 changelog
- [ ] 及时回复 issue（24h 内）
- [ ] 设置 `good first issue` 标签吸引贡献者
- [ ] 在 NoneBot / Koishi 社区发"ferroq 适配指南"
- [ ] 做性能对比视频/文章（知乎/掘金/Bilibili）

---

## 9. 风险与缓解

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| 后端 WS 协议变更 | 中 | API 不兼容 | BackendAdapter 抽象，版本协商；OneBot v11 已非常稳定 |
| Lagrange/NapCat 停更 | 中 | 失去后端 | 多后端设计天然容灾；只要有一个活着就能用 |
| 竞品出现 (Node/Go 写的同类网关) | 中 | 分流 | Rust 的性能优势 + 单二进制 + 内存占用是不可复制的护城河 |
| wasmtime 太重 | 低 | 二进制变大 | WASM 作为可选 feature，默认不编译 |
| 前端 Dashboard 工作量溢出 | 中 | 拖延发布 | v0.1 不含 Dashboard，纯 CLI 先发布 |

---

## 10. 关键指标

| 指标 | v0.1.0 目标 | v1.0.0 目标 |
|------|-------------|-------------|
| 启动到可用 | < 1s | < 500ms |
| 消息转发延迟 (p99) | < 5ms | < 2ms |
| 吞吐量 | 1k msg/s | 10k msg/s |
| 内存占用（空载） | < 10MB | < 10MB |
| 内存占用（100群活跃） | < 30MB | < 20MB |
| 二进制大小 (Linux x64) | < 15MB | < 20MB（含 WASM） |
| Docker 镜像 | < 20MB | < 25MB |

---

## 11. 当前状态

项目目录 `ferroq/` 已创建，含本文件。

**下一步：执行 Phase 1.1 — 创建 workspace 脚手架。**
确认后发送 `"开始 Phase 1"` 即可。