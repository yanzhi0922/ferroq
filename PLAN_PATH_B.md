# 路径 B：Rust 壳 + LagrangeV2 NativeAPI 内核

> **状态：备选方案，待路径 C 完成后视情况探索。**
> 
> 本方案利用 LagrangeV2 提供的 `Lagrange.Core.NativeAPI`（.NET NativeAOT 编译为
> C ABI 动态库），用 Rust 做高性能外壳（异步 I/O、OneBot 服务器、Web 管理面板），
> 通过 FFI 调用 C# 协议核心。

---

## 0. 定位

```
LagrangeV2 NativeAPI (.NET NativeAOT → .so/.dll)
           ↕ C FFI
Rust 外壳：tokio 异步事件循环 + OneBot 服务 + Web Dashboard
           ↓
        单一分发包
```

**适合场景**：希望有自主二进制分发，但不想自己维护协议层。

---

## 1. 优势与劣势

| 优势 | 劣势 |
|------|------|
| 协议层白嫖 LagrangeV2 的持续维护 | 依赖 .NET NativeAOT 工具链编译 .so/.dll |
| 签名服务由 LagrangeV2 内部处理 | FFI 边界复杂，内存管理需谨慎 |
| Rust 擅长的领域（网络 I/O / 低内存）发挥到位 | NativeAOT 产物约 15-30MB，总二进制较大 |
| 比路径 A 工作量小很多（不碰协议） | musl 交叉编译 NativeAOT 有坑 |
| 仍能做到"单文件分发"体验 | 调试 FFI 问题困难 |
| 比路径 C 少一层网络跳转（无 WS IPC） | Lagrange NativeAPI 可能不稳定（仍在开发中） |

---

## 2. 参考资料

| 优先级 | 仓库 / 文档 | 用途 |
|--------|------------|------|
| ★★★ | [LagrangeV2/Lagrange.Core.NativeAPI](https://github.com/LagrangeDev/LagrangeV2/tree/main/Lagrange.Core.NativeAPI) | NativeAPI C ABI 接口定义 |
| ★★★ | [Lagrange.Doc v2 NativeAPI](https://lagrangedev.github.io/Lagrange.Doc/v2/Lagrange.Core.NativeAPI/) | 官方文档 |
| ★★☆ | [LagrangeV2/Lagrange.Core](https://github.com/LagrangeDev/LagrangeV2/tree/main/Lagrange.Core) | 理解 Core API 对应的功能 |
| ★☆☆ | [.NET NativeAOT 文档](https://learn.microsoft.com/en-us/dotnet/core/deploying/native-aot/) | 了解 NativeAOT 编译流程 |

---

## 3. 技术栈

```toml
[workspace]
resolver = "2"
members = [
    "crates/ferroq-ffi",       # C FFI 绑定 (unsafe 层)
    "crates/ferroq-core",      # 安全 Rust 封装层
    "crates/ferroq-onebot",    # OneBot v11 适配
    "crates/ferroq-web",       # Web Dashboard
    "crates/ferroq",           # 主二进制
]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "MIT"

[workspace.dependencies]
# === 异步 ===
tokio        = { version = "1.48", features = ["full"] }
futures      = "0.3"

# === FFI ===
libloading   = "0.8"           # 运行时加载 .so/.dll
# 或在编译时 link（需要 build.rs）

# === Web ===
axum             = { version = "0.8", features = ["ws"] }
tower-http       = { version = "0.6", features = ["cors", "fs", "trace"] }
tokio-tungstenite = "0.26"

# === 序列化 ===
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
serde_yaml   = "0.9"

# === 日志 / 错误 ===
tracing            = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow             = "1"
thiserror          = "2"

# === 工具 ===
clap         = { version = "4", features = ["derive"] }
rust-embed   = "8"
bytes        = "1.11"
chrono       = { version = "0.4", features = ["serde"] }
dashmap      = "6"
```

---

## 4. 架构

```
┌─────────────────────────── ferroq 进程 ──────────────────────────┐
│                                                                   │
│  ┌───────────────────── Rust (tokio) ──────────────────────────┐ │
│  │                                                              │ │
│  │  ┌──────────┐  ┌──────────┐  ┌───────────┐  ┌───────────┐  │ │
│  │  │ OneBot   │  │   Web    │  │  多账号   │  │  消息     │  │ │
│  │  │ v11 API  │  │Dashboard │  │  管理器   │  │  持久化   │  │ │
│  │  └────┬─────┘  └──────────┘  └─────┬─────┘  └───────────┘  │ │
│  │       │                            │                         │ │
│  │  ┌────┴────────────────────────────┴────┐                    │ │
│  │  │          ferroq-core (Safe Rust)     │                    │ │
│  │  │   BotContext / Event / Message 类型  │                    │ │
│  │  └────────────────┬─────────────────────┘                    │ │
│  │                   │                                          │ │
│  │  ┌────────────────┴─────────────────────┐                    │ │
│  │  │       ferroq-ffi (unsafe 边界)       │                    │ │
│  │  │   C function 声明 + 类型映射          │                    │ │
│  │  └────────────────┬─────────────────────┘                    │ │
│  └───────────────────┼──────────────────────────────────────────┘ │
│                      │ C ABI (函数调用,无网络开销)                  │
│  ┌───────────────────┴──────────────────────────────────────────┐ │
│  │         Lagrange.Core.NativeAPI (.so / .dll)                 │ │
│  │         .NET NativeAOT 编译的原生库                           │ │
│  │         ┌──────────────────────────────────┐                 │ │
│  │         │  NTQQ 协议 · 签名 · 加密 · 连接  │                 │ │
│  │         └──────────────────────────────────┘                 │ │
│  └──────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────┘
```

### FFI 接口示例

根据 Lagrange.Core.NativeAPI 的 C ABI 导出，Rust 侧需要声明：

```rust
// crates/ferroq-ffi/src/lib.rs

use std::ffi::{c_char, c_int, c_void};

extern "C" {
    /// 创建 Bot 上下文
    fn lagrange_create_context(config_json: *const c_char) -> *mut c_void;

    /// 销毁 Bot 上下文
    fn lagrange_destroy_context(ctx: *mut c_void);

    /// 二维码登录 — 获取二维码
    fn lagrange_fetch_qrcode(
        ctx: *mut c_void,
        qrcode_buf: *mut u8,
        buf_len: *mut usize,
    ) -> c_int;

    /// 二维码登录 — 查询状态
    fn lagrange_query_qrcode_state(ctx: *mut c_void) -> c_int;

    /// 发送群消息
    fn lagrange_send_group_message(
        ctx: *mut c_void,
        group_id: u64,
        message_json: *const c_char,
    ) -> c_int;

    /// 注册事件回调
    fn lagrange_set_event_callback(
        ctx: *mut c_void,
        callback: extern "C" fn(*const c_char, usize, *mut c_void),
        user_data: *mut c_void,
    );

    // ... 其他 API
}
```

安全封装层：

```rust
// crates/ferroq-core/src/bot.rs

pub struct BotContext {
    raw: *mut c_void,
}

// SAFETY: LagrangeV2 NativeAPI 内部使用线程安全的并发模型
unsafe impl Send for BotContext {}
unsafe impl Sync for BotContext {}

impl BotContext {
    pub fn new(config: &BotConfig) -> Result<Self> { ... }
    pub async fn login_qrcode(&self) -> Result<QrCodeData> { ... }
    pub async fn send_group_message(&self, group_id: u64, msg: &MessageChain) -> Result<()> { ... }
}

impl Drop for BotContext {
    fn drop(&mut self) {
        unsafe { lagrange_destroy_context(self.raw); }
    }
}
```

---

## 5. 目录结构

```
ferroq/
├── crates/
│   ├── ferroq-ffi/                # C FFI unsafe 绑定
│   │   ├── src/
│   │   │   ├── lib.rs             # extern "C" 声明
│   │   │   └── types.rs           # C 结构体映射
│   │   ├── build.rs               # link 配置 / bindgen
│   │   └── Cargo.toml
│   │
│   ├── ferroq-core/               # 安全 Rust 封装
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── bot.rs             # BotContext 安全封装
│   │   │   ├── event.rs           # 事件类型（从 JSON callback 解析）
│   │   │   ├── message.rs         # 消息类型
│   │   │   ├── config.rs          # 配置
│   │   │   └── error.rs           # 错误类型
│   │   └── Cargo.toml
│   │
│   ├── ferroq-onebot/             # OneBot v11 适配
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── api.rs             # API 路由
│   │   │   ├── event.rs           # 事件转换
│   │   │   ├── server.rs          # HTTP + WS
│   │   │   └── ws.rs
│   │   └── Cargo.toml
│   │
│   ├── ferroq-web/                # Web Dashboard
│   │   ├── src/
│   │   │   └── lib.rs
│   │   ├── frontend/
│   │   └── Cargo.toml
│   │
│   └── ferroq/                    # 主二进制
│       ├── src/
│       │   └── main.rs
│       └── Cargo.toml
│
├── vendor/                        # 预编译的 NativeAPI 库
│   ├── linux-x64/
│   │   └── libLagrange.Core.NativeAPI.so
│   ├── darwin-arm64/
│   │   └── libLagrange.Core.NativeAPI.dylib
│   └── windows-x64/
│       └── Lagrange.Core.NativeAPI.dll
│
├── scripts/
│   └── build_nativeapi.sh         # 从源码编译 NativeAPI 的脚本
│
├── Cargo.toml
└── README.md
```

---

## 6. 构建流程

### 方式一：运行时动态加载（推荐）

```rust
// 用 libloading 在运行时加载 .so/.dll
// 优点：Rust 编译不依赖 .NET 工具链
// 缺点：分发时需附带 .so/.dll 或用户自行编译

let lib = unsafe { libloading::Library::new("./libLagrange.Core.NativeAPI.so")? };
let create_ctx: Symbol<unsafe extern "C" fn(*const c_char) -> *mut c_void>
    = unsafe { lib.get(b"lagrange_create_context")? };
```

### 方式二：编译时链接

```bash
# 1. 先编 NativeAPI
cd LagrangeV2
dotnet publish Lagrange.Core.NativeAPI -c Release -r linux-x64 --self-contained

# 2. 再编 ferroq（指定链接路径）
RUSTFLAGS="-L path/to/nativeapi/output" cargo build --release
```

### Docker 构建（完整一体）

```dockerfile
# Stage 1: 编译 NativeAPI
FROM mcr.microsoft.com/dotnet/sdk:10.0 AS dotnet-build
COPY LagrangeV2/ /src/
RUN dotnet publish /src/Lagrange.Core.NativeAPI \
    -c Release -r linux-x64 --self-contained \
    -o /out/nativeapi

# Stage 2: 编译 ferroq
FROM rust:1.85 AS rust-build
COPY . /src/ferroq
COPY --from=dotnet-build /out/nativeapi/*.so /usr/lib/
RUN cd /src/ferroq && cargo build --release

# Stage 3: 最终镜像
FROM debian:bookworm-slim
COPY --from=rust-build /src/ferroq/target/release/ferroq /usr/local/bin/
COPY --from=dotnet-build /out/nativeapi/*.so /usr/lib/
CMD ["ferroq"]
```

---

## 7. 实现路线图

### Phase 0 — 环境准备（2-3 天）
- [ ] 克隆 LagrangeV2，编译 NativeAPI 为各平台动态库
- [ ] 确认 NativeAPI 的 C ABI 接口列表
- [ ] 验证：用 C 写一个最小调用确认 FFI 可用
- [ ] 创建 Rust workspace 骨架

### Phase 1 — FFI 绑定（1 周）
- [ ] `ferroq-ffi`：声明所有 extern "C" 函数
- [ ] `ferroq-core`：安全封装 `BotContext`
- [ ] 事件回调 → tokio channel 桥接
- [ ] 单元测试（mock 模式）

### Phase 2 — 基本功能（1 周）
- [ ] 二维码登录流程
- [ ] 收发群消息
- [ ] 事件分发

### Phase 3 — OneBot 适配（1 周）
- [ ] HTTP API（send_msg / get_login_info / ...）
- [ ] 正向 WS + 反向 WS
- [ ] HTTP POST 事件上报

### Phase 4 — 打磨（1 周）
- [ ] 多账号支持
- [ ] Web Dashboard
- [ ] Docker 构建 + CI/CD
- [ ] README + 文档

**总计：约 4-5 周**

---

## 8. 关键风险

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| NativeAPI 接口不稳定/文档不全 | 高 | FFI 频繁变更 | 密切跟踪 LagrangeV2 commits；pinversion |
| NativeAOT 产物体积大 (15-30MB) | 确定 | 总分发包 > 30MB | 可接受；strip + UPX 压缩 |
| FFI 内存安全问题 | 中 | 段错误 / 内存泄漏 | unsafe 集中在 ferroq-ffi，充分测试 |
| musl 编译 NativeAOT 困难 | 高 | 无法做极小 Alpine 镜像 | 改用 debian-slim 基础镜像 |
| .NET 版本升级影响 ABI | 中 | 需重新编译 | CI 自动构建 NativeAPI |
| 回调线程模型不匹配 | 中 | 事件丢失/死锁 | 回调中仅做 channel send，不做 async |

---

## 9. 与路径 C 的关系

路径 B 完成后，可以作为路径 C 的一个 **内置后端**：

```
ferroq 网关 (路径 C)
  ├── LagrangeAdapter (WS Client)     ← 外部进程
  ├── NapCatAdapter (WS Client)       ← 外部进程  
  └── NativeAdapter (FFI in-process)  ← 路径 B，同进程，零网络开销
```

这样用户有两种使用模式：
1. **网关模式**（路径 C）：对接外部 Lagrange/NapCat，灵活但多一层 IPC
2. **内嵌模式**（路径 B）：直接 FFI 调 NativeAPI，极致性能但耦合 .NET 产物

---

## 10. 启动前置条件

1. **确认 NativeAPI 可用** — 能编译出 .so/.dll 并成功调用至少一个函数
2. **确认 ABI 稳定性** — 与 LagrangeV2 维护者沟通或查看 API 变更频率
3. **确认跨平台需求** — NativeAOT 编译需要对应平台的 .NET SDK

如果 NativeAPI 仍处于早期开发阶段且接口频繁变动，建议等其稳定后再启动路径 B。
