# 插件 API 参考

本页列出插件必须/可以导出的所有函数，以及插件可用的宿主函数。

## 必须导出

### `ferroq_plugin_info() -> i32`

返回插件元数据。须通过 `ferroq_set_result()` 传回 JSON：

```json
{
  "name": "插件名",
  "version": "0.1.0",
  "description": "功能说明",
  "author": "作者"
}
```

### `ferroq_alloc(size: i32) -> *mut u8`

在插件线性内存中分配 `size` 字节。宿主用此写入事件/请求数据。

### `ferroq_dealloc(ptr: *mut u8, size: i32)`

释放之前由 `ferroq_alloc` 分配的内存。

## 可选导出

### `ferroq_plugin_init(config_ptr: *const u8, config_len: i32) -> i32`

加载后调用一次。`config_ptr` 指向 `config.yaml` 中该插件 `config` 的 UTF-8 JSON 字符串。

### `ferroq_on_event(event_ptr: *const u8, event_len: i32) -> i32`

对流经网关的每个事件调用。`event_ptr` 指向完整内部事件的 UTF-8 JSON。

### `ferroq_on_api_call(req_ptr: *const u8, req_len: i32) -> i32`

对路由的每个 API 请求调用。

## 返回码

| 码 | 名称 | 行为 |
|----|------|------|
| `0` | **Continue** | 传给插件链中的下一个 |
| `1` | **Handled** | 停止链。若调用了 `ferroq_set_result()`，使用该数据 |
| `2` | **Drop** | 完全丢弃事件/请求 |
| `-1` | **Error** | 记录错误，按 Continue 处理 |

## 宿主函数

### `ferroq_set_result(ptr: *const u8, len: i32)`

向宿主写回结果数据。用于返回插件信息、修改后的事件或 API 请求。

### `ferroq_log(level: i32, ptr: *const u8, len: i32)`

通过 ferroq 日志系统记录消息。

| 值 | 级别 |
|----|------|
| `0` | trace |
| `1` | debug |
| `2` | info |
| `3` | warn |
| `4` | error |

## 内存模型

```
宿主 (ferroq)                    插件 (WASM)
──────────                       ──────────
1. 调用 ferroq_alloc(N)  ───►   分配 N 字节 → ptr
2. 写入数据到 ptr        ───►   （宿主写入线性内存）
3. 调用 on_event(ptr, N) ───►   处理事件
                         ◄───   调用 ferroq_set_result(out_ptr, out_len)
4. 从 WASM 读取结果      ◄───   （宿主从线性内存读取）
5. 调用 ferroq_dealloc() ───►   释放内存
```
