# 管理 API

ferroq 在 `/api` 前缀下暴露 REST API 用于运行时管理。配置了 `access_token` 时所有端点需要认证。

## 认证方式

- **请求头**：`Authorization: Bearer YOUR_TOKEN`
- **查询参数**：`?access_token=YOUR_TOKEN`

## 端点

### 健康检查

```
GET /health
```

返回网关健康状态（无需认证）。包含运行时间、计数器、适配器快照等信息。

### Prometheus 指标

```
GET /metrics
```

返回 Prometheus 文本格式的指标。

### 列出账户

```
GET /api/accounts
```

返回所有已注册的后端适配器及当前状态。

### 添加适配器

```
POST /api/accounts/add
Content-Type: application/json

{
  "name": "backup",
  "backend": {
    "type": "napcat",
    "url": "ws://127.0.0.1:3001",
    "reconnect_interval": 5
  }
}
```

在运行时动态添加新的后端适配器。

### 移除适配器

```
POST /api/accounts/{name}/remove
```

断开并移除指定适配器。

### 重连适配器

```
POST /api/accounts/{name}/reconnect
```

强制重连指定适配器。

### 运行时统计

```
GET /api/stats
```

返回详细的运行时统计信息，包含各适配器计数器。

### 查询消息

```
GET /api/messages?group_id=900001&limit=50&offset=0
```

查询已存储的消息。需要 `storage.enabled: true`。

| 参数 | 类型 | 说明 |
|------|------|------|
| `group_id` | i64 | 按群过滤 |
| `user_id` | i64 | 按发送者过滤 |
| `keyword` | string | 全文搜索 |
| `limit` | u32 | 最大结果数（默认 50） |
| `offset` | u32 | 分页偏移 |

### 查看配置

```
GET /api/config
```

返回当前配置（敏感信息已脱敏）。

### 热重载

```
POST /api/reload
```

从配置文件重新加载 access_token 和速率限制参数，无需重启。
