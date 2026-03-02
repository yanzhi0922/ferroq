# 插件系统概述

ferroq 支持基于 WASM 的插件，可以在事件和 API 调用流经网关时拦截和修改它们。

## 架构

```
后端 → [解析] → [去重] → [插件链] → [事件总线] → 协议服务器
                              ↑
                       插件 1 → 插件 2 → ...
```

插件从 `.wasm` 文件加载，在沙箱化的 [wasmtime](https://wasmtime.dev/) 运行时中执行。每个插件可以：

- **检查事件** — 记录、过滤或转换消息事件
- **检查 API 调用** — 修改或阻止外发的 API 请求
- **返回结果** — 指示继续、处理或丢弃

## 关键特性

- **沙箱化** — 插件在 WebAssembly 中运行，与宿主进程隔离
- **零信任** — 插件无法访问文件系统、网络或宿主内存
- **语言无关** — 任何编译到 WASM 的语言都可以（Rust、C、AssemblyScript 等）
- **可热管理** — 通过管理 API 启用/禁用插件
- **高性能** — wasmtime JIT 编译 WASM 为原生代码

## 配置

```yaml
plugins:
  - path: "./plugins/echo.wasm"
    enabled: true
    config:                # 以 JSON 形式传入 plugin_init()
      prefix: "[echo] "
      keyword: ""
```

## 插件生命周期

1. **加载** — ferroq 读取 `.wasm` 文件并通过 wasmtime 实例化
2. **信息** — 调用 `ferroq_plugin_info()` 获取插件名称、版本、作者
3. **初始化** — 调用 `ferroq_plugin_init(config_json)` 传入配置
4. **事件处理** — 对每个事件调用 `ferroq_on_event(event_json)`
5. **API 处理** — 对每个 API 调用 `ferroq_on_api_call(request_json)`

## 功能开关

WASM 插件系统在 `wasm-plugins` cargo feature 后面（默认启用）。不需要时可以关闭：

```bash
cargo build --release --no-default-features -p ferroq-gateway
```

这会移除 wasmtime 依赖，显著减小二进制体积。
