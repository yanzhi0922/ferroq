# 路径 A：Fork mania — 纯 Rust NTQQ 协议实现

> **状态：备选方案，待路径 C 完成后视情况探索。**
> 
> 本方案的核心是 fork 已归档的 [mania](https://github.com/LagrangeDev/mania) 项目，
> 补全协议、更新到 LagrangeV2 最新版本，并加上 OneBot 适配层。

---

## 0. 定位

```
直接移植/续写 mania → 成为"Rust 版 Lagrange"
```

**适合场景**：希望拥有完全自主的协议栈，不依赖任何外部 OneBot 实现。

---

## 1. 优势与劣势

| 优势 | 劣势 |
|------|------|
| 完全自主，端到端纯 Rust | 工作量巨大（2-4 个月全职） |
| 极致性能，无 IPC 开销 | **签名服务是致命外部依赖** |
| 可做 FFI 导出（C ABI / Python 绑定） | 协议逆向需要持续跟进 LagrangeV2 |
| 单二进制、极小内存 | mania 归档时只有 53 stars，叙事难差异化 |
| 可深度定制协议行为 | TLV / Noise / Proto 移植极其琐碎 |

---

## 2. 参考资料

| 优先级 | 仓库 | 状态 | 用途 |
|--------|------|------|------|
| ★★★ | [mania](https://github.com/LagrangeDev/mania) (Rust) | 归档 2026-01 | 直接 fork 基础，加密/TLV/部分 proto 已实现 |
| ★★★ | [LagrangeV2](https://github.com/LagrangeDev/LagrangeV2) (C#) | 活跃 | 协议更新的首要参考 |
| ★★☆ | [Lagrange.Core](https://github.com/LagrangeDev/Lagrange.Core) (C#) | 归档 2025-10 | 旧版 TLV / 包结构参考 |
| ★☆☆ | [LagrangeGo](https://github.com/LagrangeDev/LagrangeGo) (Go) | 活跃 | 流程理解 |

---

## 3. 技术栈

直接继承 mania 的依赖选型，按需升级：

```toml
[workspace]
resolver = "2"
members = ["ferroq-core", "ferroq-onebot", "examples"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "GPL-3.0"

[workspace.dependencies]
# 沿用 mania 选型
tokio           = { version = "1.48", features = ["fs", "net", "io-util", "time", "macros", "rt-multi-thread", "signal"] }
bytes           = { version = "1.11", features = ["serde"] }
prost           = "0.14"
prost-build     = "0.14"
p256            = { version = "0.14.0-pre", features = ["ecdh"] }
elliptic-curve  = "0.14.0-rc"
noise           = "0.9"
serde           = { version = "1", features = ["derive"] }
serde_json      = "1"
reqwest         = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }
axum            = "0.8"
dashmap         = "6"
chrono          = { version = "0.4", features = ["serde"] }
uuid            = { version = "1.18", features = ["serde", "v4"] }
rand            = "0.9"
flate2          = "1.1"
image           = "0.25"
qrcode          = "0.14"
num_enum        = "0.7"
thiserror       = "2"
anyhow          = "1"
tracing         = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
hex             = "0.4"
md-5            = "0.11.0-rc"
sha1            = "0.11.0-rc"
sha2            = "0.10"
arc-swap        = { version = "1.7", features = ["serde"] }
inventory       = "0.3"
surge-ping      = "0.8"
phf             = { version = "0.13", features = ["macros"] }
quick-xml       = { version = "0.38", features = ["serialize"] }
futures         = "0.3"
tokio-util      = "0.7"
```

---

## 4. 目录结构

```
ferroq/
├── crates/
│   ├── ferroq-core/                # 协议 + 客户端一体
│   │   ├── build.rs                # prost-build
│   │   ├── protos/                 # .proto 文件
│   │   │   ├── message/
│   │   │   ├── oidb/
│   │   │   ├── service/
│   │   │   └── action/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── crypto/
│   │       │   ├── mod.rs
│   │       │   ├── tea.rs          # QQ 变种 TEA (从 mania 复用)
│   │       │   ├── ecdh.rs         # P-256 ECDH (从 mania 复用)
│   │       │   └── trisha1.rs      # TripleSHA1 (从 mania 复用)
│   │       ├── protocol/
│   │       │   ├── mod.rs
│   │       │   ├── version.rs      # AppInfo 常量 (需从 LagrangeV2 更新)
│   │       │   ├── device.rs       # 设备信息
│   │       │   └── pb/             # prost 生成
│   │       ├── packet/
│   │       │   ├── mod.rs
│   │       │   ├── sso.rs          # SSO 包
│   │       │   ├── oidb.rs         # OidbSvc
│   │       │   └── uni.rs          # Uni 包
│   │       ├── tlv/
│   │       │   ├── mod.rs
│   │       │   └── builders.rs     # 20+ TLV 构造器
│   │       ├── sign/
│   │       │   └── provider.rs     # 签名服务 HTTP 调用
│   │       ├── store/
│   │       │   ├── key_store.rs
│   │       │   └── device_store.rs
│   │       ├── client/
│   │       │   ├── client.rs
│   │       │   ├── net.rs          # TCP + Noise 握手
│   │       │   ├── login.rs        # 二维码登录状态机
│   │       │   ├── heartbeat.rs
│   │       │   └── highway.rs      # 文件/图片上传
│   │       ├── service/
│   │       │   ├── msg.rs
│   │       │   ├── group.rs
│   │       │   ├── friend.rs
│   │       │   └── system.rs
│   │       ├── event/
│   │       │   └── dispatch.rs
│   │       └── message/
│   │           ├── chain.rs
│   │           └── codec.rs
│   │   └── Cargo.toml
│   │
│   └── ferroq-onebot/              # OneBot v11 适配
│       ├── src/
│       │   ├── lib.rs
│       │   ├── config.rs
│       │   ├── api/
│       │   │   ├── message.rs
│       │   │   ├── group.rs
│       │   │   └── meta.rs
│       │   ├── event.rs
│       │   ├── server.rs
│       │   └── ws.rs
│       └── Cargo.toml
│
├── examples/
│   ├── qrcode_login.rs
│   └── echo_bot.rs
├── Cargo.toml
└── README.md
```

---

## 5. 实现路线图

### Phase 0 — 脚手架（0.5 天）
- [ ] Fork mania，重命名为 ferroq
- [ ] 整理 workspace 结构（mania 是单 crate，需拆分）
- [ ] 升级依赖版本
- [ ] `cargo build` 通过

### Phase 1 — 加密层（2-3 天）
- [ ] 验证 mania 的 TEA / ECDH / TripleSHA1 仍可用
- [ ] 补全缺失的加密模块（对照 LagrangeV2）
- [ ] 单元测试全部通过

### Phase 2 — 协议基础（1 周）
- [ ] 从 LagrangeV2 更新 AppInfo 常量
- [ ] 补全/更新 .proto 文件
- [ ] 设备信息生成 + 持久化
- [ ] SSO 包编解码
- [ ] TLV 构造器（20+ 种）

### Phase 3 — 签名服务（2 天）
- [ ] Sign Provider HTTP 客户端
- [ ] 验证签名服务可用性
- [ ] **⚠️ 如果此步无法完成，整个路径 A 暂停**

### Phase 4 — 连接层（1-2 周）
- [ ] TCP + Noise 握手
- [ ] 心跳（~270s）
- [ ] 断线重连（指数退避）

### Phase 5 — 二维码登录（1-2 周）
- [ ] 登录状态机
- [ ] 终端二维码显示
- [ ] 会话持久化

### Phase 6 — 收发消息（1 周）
- [ ] MessageChain 抽象
- [ ] 接收群/私聊消息
- [ ] 发送群/私聊消息

### Phase 7 — OneBot 适配（1 周）
- [ ] HTTP API
- [ ] 事件上报
- [ ] 正向/反向 WebSocket

**总计：约 6-8 周全职**

---

## 6. 关键风险

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| **签名服务不可用** | **高** | **致命** — 无法登录 | 先验证再开工；准备自建方案 |
| 协议变更 | 高 | 需反复移植 C# 代码 | 跟踪 LagrangeV2 commit |
| Noise 握手细节 | 中 | 连接失败 | 对照 LagrangeV2 + 抓包 |
| mania 代码过时 | 中 | 需大量改写 | 以 mania 为骨架、LagrangeV2 为血肉 |
| Proto 结构不全 | 中 | 部分功能缺失 | 按需从 LagrangeV2 反推 |

---

## 7. 启动前置条件

在开始 Phase 0 之前，必须先验证：

1. **签名服务可用** — 用 lagrange-python 或 LagrangeV2 跑通一次二维码登录
2. **mania 代码可编译** — `git clone && cargo build` 确认基线可用
3. **明确 LagrangeV2 与 mania 的协议差异** — 列出需要更新的模块清单

如果第 1 条无法满足，**不要开始路径 A**。

---

## 8. 与路径 C 的关系

如果路径 C（网关）完成后仍希望拥有自主协议栈：
- 路径 A 的 `ferroq-core` 可作为路径 C 中的一个新 `BackendAdapter`
- 即：ferroq 网关 → ferroq-core 直连 QQ 服务器，而非通过 Lagrange/NapCat 中转
- 这样路径 A 成为路径 C 的"性能极致模式"
