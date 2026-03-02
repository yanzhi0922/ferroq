# 编写你的第一个插件

本指南带你用 Rust 创建一个简单的 ferroq WASM 插件。

## 前置条件

- Rust 1.85+ 并安装 WASM target：
  ```bash
  rustup target add wasm32-unknown-unknown
  ```

## 第 1 步：创建项目

```bash
cargo new --lib my_plugin
cd my_plugin
```

编辑 `Cargo.toml`：

```toml
[package]
name = "my-plugin"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[workspace]
```

## 第 2 步：编写插件

在 `src/lib.rs` 中：

```rust
use std::cell::RefCell;

// === ferroq 提供的宿主函数 ===
unsafe extern "C" {
    fn ferroq_set_result(ptr: *const u8, len: i32);
    fn ferroq_log(level: i32, ptr: *const u8, len: i32);
}

// === 全局状态 ===
thread_local! {
    static CONFIG: RefCell<serde_json::Value> = RefCell::new(serde_json::Value::Null);
}

// === 必须：插件信息 ===
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_plugin_info() -> i32 {
    let info = serde_json::json!({
        "name": "my-plugin",
        "version": "0.1.0",
        "description": "我的第一个 ferroq 插件",
        "author": "me"
    });
    let bytes = serde_json::to_vec(&info).unwrap();
    unsafe { ferroq_set_result(bytes.as_ptr(), bytes.len() as i32); }
    0
}

// === 必须：内存分配 ===
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_alloc(size: i32) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

#[unsafe(no_mangle)]
pub extern "C" fn ferroq_dealloc(ptr: *mut u8, size: i32) {
    let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { std::alloc::dealloc(ptr, layout); }
}

// === 可选：使用配置初始化 ===
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_plugin_init(config_ptr: *const u8, config_len: i32) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts(config_ptr, config_len as usize) };
    if let Ok(config) = serde_json::from_slice::<serde_json::Value>(slice) {
        CONFIG.with(|c| *c.borrow_mut() = config);
    }
    log(2, "my-plugin 初始化完成！");
    0
}

// === 可选：处理事件 ===
#[unsafe(no_mangle)]
pub extern "C" fn ferroq_on_event(event_ptr: *const u8, event_len: i32) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts(event_ptr, event_len as usize) };
    if let Ok(event) = serde_json::from_slice::<serde_json::Value>(slice) {
        let post_type = event.get("post_type").and_then(|v| v.as_str()).unwrap_or("");
        log(1, &format!("收到事件：post_type={post_type}"));
    }
    0 // Continue — 传给下一个插件
}

fn log(level: i32, msg: &str) {
    unsafe { ferroq_log(level, msg.as_ptr(), msg.len() as i32); }
}
```

## 第 3 步：编译

```bash
cargo build --release --target wasm32-unknown-unknown
```

输出位于 `target/wasm32-unknown-unknown/release/my_plugin.wasm`。

## 第 4 步：配置

复制 `.wasm` 文件并添加到 ferroq 配置：

```yaml
plugins:
  - path: "./plugins/my_plugin.wasm"
    enabled: true
    config: {}
```

## 第 5 步：运行

启动 ferroq，你应该看到插件的日志输出。

## 返回码

| 码 | 含义 | 行为 |
|----|------|------|
| `0` | Continue | 传给下一个插件/处理器 |
| `1` | Handled | 停止插件链，使用 `ferroq_set_result` 的结果 |
| `2` | Drop | 完全丢弃事件或 API 调用 |
| `-1` | Error | 记录错误，继续处理 |
