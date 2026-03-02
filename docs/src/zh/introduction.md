# ferroq

> **高性能 QQ Bot 统一网关** — 纯 Rust 编写

[![CI](https://github.com/yanzhi0922/ferroq/actions/workflows/ci.yml/badge.svg)](https://github.com/yanzhi0922/ferroq/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/yanzhi0922/ferroq)](https://github.com/yanzhi0922/ferroq/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/yanzhi0922/ferroq/blob/main/LICENSE)

**ferroq** 是一个高性能的 QQ Bot 协议网关，位于 QQ 协议后端（Lagrange、NapCat、官方 API）和 Bot 框架（NoneBot2、Koishi、Yunzai 等）之间。

ferroq 不重新实现 QQ 协议，而是作为**统一代理/路由器**，提供：

- ⚡ **极致性能** — 异步 Rust，端到端转发延迟 <20µs，吞吐量 290K+ msg/s
- 🔄 **多协议支持** — OneBot v11（完整）、OneBot v12、Satori
- 🔌 **后端无关** — Lagrange.OneBot、NapCat — 热切换无需重启
- 🧩 **WASM 插件** — 用沙箱化的 WebAssembly 扩展自定义逻辑
- 🛡️ **高可靠** — 指数退避重连、故障转移、事件去重
- 💾 **消息存储** — 可选的 SQLite 持久化，支持搜索和分页
- 🔒 **安全** — Bearer 认证、HMAC-SHA1 签名、令牌热重载
- 📦 **单二进制** — 一个 `ferroq` 二进制文件，无运行时依赖，<15MB

## 适用场景

- **Bot 开发者**：想用一个网关抽象多个后端
- **平台运维**：运行多个 QQ 账号，需要集中化的事件路由
- **插件作者**：想用 Rust/WASM 编写消息处理逻辑
- 厌倦了在 Lagrange 和 NapCat 之间切换时修改 Bot 代码的人

## 语言

本文档提供以下语言：
- [English](https://github.com/yanzhi0922/ferroq/tree/main/docs/src/en)
- **简体中文**（当前页面）
