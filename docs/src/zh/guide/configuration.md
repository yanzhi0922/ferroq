# 配置参考

ferroq 使用 YAML 配置文件（默认 `config.yaml`）。本页面列出所有配置项。

## 服务器

```yaml
server:
  host: "0.0.0.0"       # 监听地址
  port: 8080             # 监听端口
  access_token: ""       # 全局认证令牌（空 = 不启用认证）
  dashboard: true        # 启用 Web 仪表盘
  rate_limit:
    enabled: false       # 启用全局速率限制
    requests_per_second: 100
    burst: 200           # 令牌桶突发容量
```

| 键 | 类型 | 默认值 | 说明 |
|----|------|--------|------|
| `host` | string | `"0.0.0.0"` | 绑定地址 |
| `port` | u16 | `8080` | 绑定端口 |
| `access_token` | string | `""` | 全局 Bearer 令牌，空则不启用认证 |
| `dashboard` | bool | `true` | 在 `/dashboard`（兼容 `/dashboard/`）提供 Web 仪表盘 |
| `rate_limit.enabled` | bool | `false` | 启用令牌桶速率限制 |
| `rate_limit.requests_per_second` | u64 | `100` | 每秒补充速率 |
| `rate_limit.burst` | u64 | `200` | 最大突发容量 |

## 账户

每个账户对应一个 QQ 协议后端。

```yaml
accounts:
  - name: "main"
    backend:
      type: lagrange             # lagrange, napcat, official, mock
      url: "ws://127.0.0.1:8081/onebot/v11/ws"
      access_token: ""
      reconnect_interval: 5      # 秒（指数退避基数）
      max_reconnect_interval: 120
      health_check_interval: 30
      connect_timeout: 15
      api_timeout: 30
    # 可选故障转移后端：
    # fallback:
    #   type: napcat
    #   url: "ws://127.0.0.1:3001"
```

| 键 | 类型 | 默认值 | 说明 |
|----|------|--------|------|
| `name` | string | 必填 | 账户唯一名称 |
| `backend.type` | string | 必填 | 后端类型 |
| `backend.url` | string | 必填 | 后端地址（`lagrange/napcat` 使用 `ws://`，`official` 使用 `http(s)://`） |
| `backend.access_token` | string | `""` | 后端认证令牌 |
| `backend.reconnect_interval` | u64 | `5` | 基础重连间隔（秒） |
| `backend.max_reconnect_interval` | u64 | `120` | 指数退避最大间隔 |
| `backend.health_check_interval` | u64 | `30` | 健康检查间隔（秒） |
| `backend.connect_timeout` | u64 | `15` | WS 连接超时（秒） |
| `backend.api_timeout` | u64 | `30` | API 调用超时（秒） |
| `fallback` | object | 无 | 可选故障转移后端 |

说明：
- `official` 后端是 HTTP API 优先适配器，API 调用和健康检查走 HTTP。
- 事件推送能力取决于后端本身；官方 HTTP 适配器不会主动建立持久 WS 事件流。

## 协议

配置上游 Bot 框架如何连接 ferroq。

### OneBot v11

```yaml
protocols:
  onebot_v11:
    enabled: true
    http: true          # POST /onebot/v11/api/:action
    ws: true            # ws://host:port/onebot/v11/ws
    ws_reverse: []      # 反向 WS（ferroq 主动连接）
    http_post: []       # HTTP POST 推送事件
```

### OneBot v12

```yaml
protocols:
  onebot_v12:
    enabled: false
    http: true          # POST /onebot/v12/action
    ws: true            # ws://host:port/onebot/v12/ws
```

### Satori

```yaml
protocols:
  satori:
    enabled: false
    http: true          # POST /satori/v1/{resource}.{method}
    ws: true            # ws://host:port/satori/v1/events
```

## 消息存储

```yaml
storage:
  enabled: false
  path: "./data/messages.db"   # SQLite 数据库路径
  max_days: 30                 # 自动清理天数
```

## 事件去重

```yaml
dedup:
  enabled: true
  window_secs: 60     # 指纹窗口时间
```

## 插件

```yaml
plugins:
  - path: "./plugins/echo.wasm"
    enabled: true
    config:
      prefix: "[echo] "
```

详见[插件系统概述](../plugins/overview.md)。

## 日志

```yaml
logging:
  level: "info"          # trace, debug, info, warn, error
  console: true
```

## 环境变量

| 变量 | 说明 |
|------|------|
| `FERROQ_CONFIG` | 配置文件路径（默认 `config.yaml`） |
| `RUST_LOG` | 覆盖日志过滤器 |
| `FERROQ_WS_OUTBOUND_QUEUE_CAPACITY` | 每连接 WS 出站队列容量（`64..65536`，默认 `1024`） |
| `FERROQ_WS_API_MAX_IN_FLIGHT` | 每连接 WS API 并发上限（`1..8192`，默认 `64`） |
