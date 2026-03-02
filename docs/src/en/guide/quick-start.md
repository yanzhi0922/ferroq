# Quick Start

Get ferroq up and running in under 5 minutes.

## Prerequisites

- A running QQ protocol backend (e.g. [Lagrange.OneBot](https://github.com/LagrangeDev/Lagrange.Core) or [NapCat](https://github.com/NapNeko/NapCatQQ))
- The backend's WebSocket URL (e.g. `ws://127.0.0.1:8081/onebot/v11/ws`)

## Installation

### Docker (Recommended)

```bash
# Download example config
curl -LO https://raw.githubusercontent.com/yanzhi0922/ferroq/main/config.example.yaml
mv config.example.yaml config.yaml

# Edit config.yaml — set your backend URL
# Then run:
docker run -d \
  --name ferroq \
  -p 8080:8080 \
  -v $(pwd)/config.yaml:/app/config.yaml:ro \
  -v $(pwd)/data:/app/data \
  ghcr.io/yanzhi0922/ferroq:latest
```

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/yanzhi0922/ferroq/releases):

```bash
# Linux x86_64
curl -LO https://github.com/yanzhi0922/ferroq/releases/latest/download/ferroq-linux-x86_64.tar.gz
tar xzf ferroq-linux-x86_64.tar.gz
chmod +x ferroq

# Generate default config
./ferroq --generate-config

# Edit config.yaml, then:
./ferroq
```

### From Source

```bash
git clone https://github.com/yanzhi0922/ferroq.git
cd ferroq
cargo build --release
./target/release/ferroq --generate-config
./target/release/ferroq
```

## Minimal Configuration

Create a `config.yaml` in the working directory:

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  access_token: ""        # Set a token for production!
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

## Verify it Works

1. Start your backend (e.g. Lagrange.OneBot)
2. Start ferroq: `./ferroq`
3. Check health: `curl http://localhost:8080/health`
4. Open dashboard: `http://localhost:8080/dashboard/`

You should see your backend listed as "connected" in the health response.

## Connect Your Bot Framework

Point your bot framework to ferroq instead of the backend directly:

| Framework | Setting |
|-----------|---------|
| NoneBot2 | `ONEBOT_WS_URLS=["ws://127.0.0.1:8080/onebot/v11/ws"]` |
| Koishi | Connection URL: `ws://127.0.0.1:8080/onebot/v11/ws` |
| Yunzai | WebSocket URL: `ws://127.0.0.1:8080/onebot/v11/ws` |

## Next Steps

- [Configuration Reference](./configuration.md) — all config options explained
- [Protocol Servers](./protocols.md) — OneBot v11/v12, Satori setup
- [Failover & Deduplication](./failover.md) — high-availability setup
