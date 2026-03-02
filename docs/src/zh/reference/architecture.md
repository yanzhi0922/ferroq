# 架构设计

## 总体概览

```
┌──────────────────────────────────────────────────────────┐
│                      Bot 框架                             │
│        NoneBot2 / Koishi / Yunzai / 自定义 Bot            │
└─────────────────────────┬────────────────────────────────┘
                          │  OneBot v11 / v12 / Satori
                          ▼
┌──────────────────────────────────────────────────────────┐
│                     ⚡ ferroq                             │
│  ┌──────────────┐  ┌──────────┐  ┌────────────────────┐ │
│  │ 协议服务器    │  │ 事件总线  │  │ 后端适配器         │ │
│  │ (入站)       │◄─┤          │◄─┤ (出站)             │ │
│  │              │  │ 广播+去重 │  │ • Lagrange WS      │ │
│  │ • OneBot v11 │  │ +插件    │  │ • NapCat WS        │ │
│  │ • OneBot v12 │  │          │  │ • 官方 API         │ │
│  │ • Satori     │  │          │  │                    │ │
│  └──────────────┘  └──────────┘  └────────────────────┘ │
│  ┌──────────┐  ┌────────────┐  ┌────────────────────┐   │
│  │ 仪表盘   │  │ 管理 API   │  │ 消息存储           │   │
│  │ (Web UI) │  │ REST       │  │ (SQLite)           │   │
│  └──────────┘  └────────────┘  └────────────────────┘   │
│  ┌──────────────────────────────────────────────────┐    │
│  │           WASM 插件引擎 (wasmtime)               │    │
│  └──────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────┘
```

## Crate 结构

```
ferroq/
├── crates/
│   ├── ferroq-core/       # 核心类型、trait、配置（无 I/O）
│   ├── ferroq-gateway/    # 网关逻辑（所有 I/O）
│   │   ├── adapter/       # 后端适配器（Lagrange、故障转移）
│   │   ├── server/        # 协议服务器（OneBot v11/v12、Satori）
│   │   ├── bus.rs         # 事件总线（tokio broadcast）
│   │   ├── router.rs      # API 路由器（self_id → 适配器）
│   │   ├── dedup.rs       # 事件去重过滤器
│   │   ├── storage.rs     # SQLite 消息存储
│   │   ├── management.rs  # REST 管理 API
│   │   ├── middleware.rs   # 认证 + 速率限制
│   │   ├── plugin_engine.rs # WASM 插件运行时
│   │   └── runtime.rs     # 网关生命周期
│   ├── ferroq-web/        # 嵌入式 Web 仪表盘
│   └── ferroq/            # CLI 二进制入口
```

## 数据流

### 事件流（后端 → Bot 框架）

1. 后端 WebSocket 消息到达
2. 适配器接收原始 JSON
3. `onebot_v11::parse_event()` 转换为内部 Event
4. DedupFilter 检查指纹（重复则丢弃）
5. PluginEngine 运行插件链
6. EventBus 广播给所有订阅者
7. 协议服务器序列化为协议格式
8. 发送给已连接的 Bot 框架

### API 流（Bot 框架 → 后端）

1. Bot 框架发送 API 请求
2. 协议服务器解析 action + params
3. 认证中间件校验 token
4. 速率限制器检查令牌桶
5. PluginEngine 运行插件链
6. ApiRouter 解析 self_id → 适配器
7. 适配器转发到后端
8. 响应返回给 Bot 框架

## 关键设计决策

| 决策 | 理由 |
|------|------|
| tokio broadcast 事件总线 | 发布 O(1)，不受订阅者数量影响 |
| 128 位指纹去重 | 避免对完整 JSON 哈希，HashMap O(1) 查找 |
| trait BackendAdapter | 适配器模式支持多后端无需改代码 |
| wasmtime 插件 | 工业级 WASM 运行时，JIT 编译，沙箱隔离 |
| Feature 门控 WASM | wasmtime 体积大，不需要的用户可排除 |
| parking_lot 互斥锁 | 无竞争场景比 std Mutex 更快 |
