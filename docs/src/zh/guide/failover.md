# 故障转移与去重

ferroq 支持每个账户在主后端和备用后端之间自动故障转移，并内置事件去重防止重复消息。

## 故障转移

### 配置

```yaml
accounts:
  - name: "main"
    backend:
      type: lagrange
      url: "ws://127.0.0.1:8081/onebot/v11/ws"
    fallback:
      type: napcat
      url: "ws://127.0.0.1:3001"
```

### 行为

配置了 fallback 后，ferroq 将主后端 + 备用后端包装为 **FailoverAdapter**：

- **API 调用**：先尝试主后端，连接失败时自动重试备用后端
- **事件**：同时从两个后端接收事件，确保即使一个断连也不会丢失事件
- **健康检查**：任一后端健康即视为健康
- 两个后端独立进行指数退避重连

### 重连机制

每个后端适配器使用指数退避：

1. 检测到断连
2. 等待 `reconnect_interval` 秒（默认 5）
3. 尝试重连
4. 失败：间隔翻倍（上限 `max_reconnect_interval`）
5. 成功：重置为基础间隔

## 事件去重

故障转移激活时，同一事件可能从主后端和备用后端分别到达。

### 工作原理

```yaml
dedup:
  enabled: true
  window_secs: 60
```

去重过滤器维护一个时间窗口内的事件**指纹**集合：

| 事件类型 | 指纹组成 |
|----------|----------|
| 消息 | `(self_id, message_id)` |
| 通知 | `(self_id, notice_type, sub_type, time_sec, extra_hash)` |
| 请求 | `(self_id, request_type, sub_type, time_sec, extra_hash)` |
| 元事件 | `(self_id, meta_event_type, time_sec)` |

`window_secs` 内具有相同指纹的事件会被静默丢弃。

### 性能

去重过滤器每事件 O(1) — 使用 128 位哈希键在 `HashMap` 中查找，配合惰性清理。基准测试结果：

- 唯一事件 ~670 ns（缓存未命中）
- 重复事件 ~420 ns（缓存命中）
- 10,000+ 缓存条目时无性能退化

### 监控

`/health` 端点报告 `events_deduplicated` — 自启动以来抑制的重复事件总数。
