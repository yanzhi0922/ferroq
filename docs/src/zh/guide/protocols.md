# 协议服务器

ferroq 支持同时运行多个入站协议服务器。每个协议服务器在协议的线上格式和 ferroq 内部事件类型之间进行转换。

## OneBot v11

最完整的协议支持。兼容 NoneBot2、Koishi、Yunzai 和大部分 QQ Bot 框架。

### 端点

| 端点 | 类型 | 说明 |
|------|------|------|
| `POST /onebot/v11/api/:action` | HTTP | 调用 OneBot v11 动作 |
| `POST /onebot/v11/api` | HTTP | 旧版端点（action 在请求体中） |
| `WS /onebot/v11/ws` | WebSocket | 正向 WebSocket（双向） |

### HTTP API

通过 HTTP POST 发送动作：

```bash
curl -X POST http://localhost:8080/onebot/v11/api/send_group_msg \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"group_id": 123456, "message": "你好！"}'
```

### 正向 WebSocket

用 WebSocket 客户端连接以接收事件和发送动作：

```
ws://localhost:8080/onebot/v11/ws?access_token=YOUR_TOKEN
```

### 反向 WebSocket

ferroq 主动连接到你的 Bot：

```yaml
protocols:
  onebot_v11:
    ws_reverse:
      - url: "ws://127.0.0.1:8765/onebot/v11/ws"
        access_token: ""
```

支持断连后指数退避重连。

### HTTP POST 推送

ferroq 将事件推送到你的端点：

```yaml
protocols:
  onebot_v11:
    http_post:
      - url: "http://127.0.0.1:5700"
        secret: "your-hmac-secret"
```

设置 `secret` 后，每个请求包含 `X-Signature` 头（HMAC-SHA1 签名）。

---

## OneBot v12

较新的 OneBot 标准，线上格式更规范。

### 端点

| 端点 | 类型 | 说明 |
|------|------|------|
| `POST /onebot/v12/action` | HTTP | 发送动作 |
| `POST /onebot/v12/action/:action` | HTTP | 发送命名动作 |
| `WS /onebot/v12/ws` | WebSocket | 双向 WebSocket |

---

## Satori

[Satori 协议](https://satori.chat) 是一个跨平台 Bot 协议。

### 端点

| 端点 | 类型 | 说明 |
|------|------|------|
| `POST /satori/v1/{resource}.{method}` | HTTP | 调用 Satori API |
| `WS /satori/v1/events` | WebSocket | 事件流（IDENTIFY/READY 握手） |

### WebSocket 握手

1. 客户端连接 `ws://host:port/satori/v1/events`
2. 客户端发送 IDENTIFY：`{"op": 3, "body": {"token": "YOUR_TOKEN"}}`
3. 服务器返回 READY：`{"op": 4, "body": {"logins": [...]}}`
4. 服务器推送 EVENT（`op: 0`）
5. 客户端须定期发送 PING（`op: 1`），服务器回复 PONG（`op: 2`）
