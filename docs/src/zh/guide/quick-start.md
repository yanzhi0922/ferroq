# 快速开始

5 分钟内启动 ferroq。

## 前置条件

- 一个运行中的 QQ 协议后端（如 [Lagrange.OneBot](https://github.com/LagrangeDev/Lagrange.Core) 或 [NapCat](https://github.com/NapNeko/NapCatQQ)）
- 后端的 WebSocket 地址（如 `ws://127.0.0.1:8081/onebot/v11/ws`）

## 安装

### Docker（推荐）

```bash
# 下载示例配置
curl -LO https://raw.githubusercontent.com/yanzhi0922/ferroq/main/config.example.yaml
mv config.example.yaml config.yaml

# 编辑 config.yaml —— 设置你的后端地址
# 然后运行：
docker run -d \
  --name ferroq \
  -p 8080:8080 \
  -v $(pwd)/config.yaml:/app/config.yaml:ro \
  -v $(pwd)/data:/app/data \
  ghcr.io/yanzhi0922/ferroq:latest
```

### 预编译二进制

从 [GitHub Releases](https://github.com/yanzhi0922/ferroq/releases) 下载：

```bash
# Linux x86_64
curl -LO https://github.com/yanzhi0922/ferroq/releases/latest/download/ferroq-linux-x86_64.tar.gz
tar xzf ferroq-linux-x86_64.tar.gz
chmod +x ferroq

# 生成默认配置
./ferroq --generate-config

# 编辑 config.yaml，然后：
./ferroq
```

### 从源码编译

```bash
git clone https://github.com/yanzhi0922/ferroq.git
cd ferroq
cargo build --release
./target/release/ferroq --generate-config
./target/release/ferroq
```

## 最小配置

在工作目录创建 `config.yaml`：

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  access_token: ""        # 生产环境请设置 token！
  dashboard: true

accounts:
  - name: "main"
    backend:
      type: lagrange
      url: "ws://127.0.0.1:8081/onebot/v11/ws"
      access_token: ""
      reconnect_interval: 5

protocols:
  onebot_v11:
    enabled: true
    http: true
    ws: true
```

## 验证运行

1. 启动后端（如 Lagrange.OneBot）
2. 启动 ferroq：`./ferroq`
3. 检查健康状态：`curl http://localhost:8080/health`
4. 打开仪表盘：`http://localhost:8080/dashboard`（或 `/dashboard/`）

你应该在健康响应中看到后端状态为 "connected"。

## 连接 Bot 框架

将你的 Bot 框架指向 ferroq 而非直接连接后端：

| 框架 | 配置项 |
|------|--------|
| NoneBot2 | `ONEBOT_WS_URLS=["ws://127.0.0.1:8080/onebot/v11/ws"]` |
| Koishi | 连接地址：`ws://127.0.0.1:8080/onebot/v11/ws` |
| Yunzai | WebSocket 地址：`ws://127.0.0.1:8080/onebot/v11/ws` |

## 下一步

- [配置参考](./configuration.md) — 所有配置项详解
- [协议服务器](./protocols.md) — OneBot v11/v12、Satori 设置
- [故障转移与去重](./failover.md) — 高可用配置
