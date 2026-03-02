# Architecture

## High-Level Overview

```
┌──────────────────────────────────────────────────────────┐
│                      Bot Frameworks                       │
│        NoneBot2 / Koishi / Yunzai / Custom Bot            │
└─────────────────────────┬────────────────────────────────┘
                          │  OneBot v11 / v12 / Satori
                          ▼
┌──────────────────────────────────────────────────────────┐
│                     ⚡ ferroq                             │
│  ┌──────────────┐  ┌──────────┐  ┌────────────────────┐ │
│  │ Protocol     │  │ Event    │  │ Backend            │ │
│  │ Servers      │◄─┤ Bus      │◄─┤ Adapters           │ │
│  │ (inbound)    │  │          │  │ (outbound)         │ │
│  │              │  │ broadcast│  │ • Lagrange WS      │ │
│  │ • OneBot v11 │  │ + dedup  │  │ • NapCat WS        │ │
│  │ • OneBot v12 │  │ + plugin │  │ • Official API     │ │
│  │ • Satori     │  │          │  │                    │ │
│  └──────────────┘  └──────────┘  └────────────────────┘ │
│  ┌──────────┐  ┌────────────┐  ┌────────────────────┐   │
│  │Dashboard │  │ Management │  │ Message Storage    │   │
│  │ (Web UI) │  │ REST API   │  │ (SQLite)           │   │
│  └──────────┘  └────────────┘  └────────────────────┘   │
│  ┌──────────────────────────────────────────────────┐    │
│  │           WASM Plugin Engine (wasmtime)           │    │
│  └──────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────┘
```

## Crate Structure

```
ferroq/
├── crates/
│   ├── ferroq-core/       # Core types, traits, config (no I/O)
│   │   ├── adapter.rs     # BackendAdapter trait
│   │   ├── api.rs         # ApiRequest / ApiResponse
│   │   ├── config.rs      # AppConfig, validation
│   │   ├── error.rs       # GatewayError enum
│   │   ├── event.rs       # Event, MessageEvent, ...
│   │   ├── message.rs     # MessageSegment enum
│   │   └── plugin.rs      # Plugin interface types
│   │
│   ├── ferroq-gateway/    # Gateway logic (all I/O)
│   │   ├── adapter/       # Backend adapters (Lagrange, Failover)
│   │   ├── server/        # Protocol servers (OneBot v11/v12, Satori)
│   │   ├── bus.rs         # Event bus (tokio broadcast)
│   │   ├── router.rs      # API router (self_id → adapter)
│   │   ├── dedup.rs       # Event deduplication filter
│   │   ├── forward.rs     # Event forwarding logic
│   │   ├── stats.rs       # Runtime stats, Prometheus, health
│   │   ├── storage.rs     # SQLite message store
│   │   ├── management.rs  # REST management API
│   │   ├── middleware.rs   # Auth + rate limiting
│   │   ├── plugin_engine.rs # WASM plugin runtime
│   │   └── runtime.rs     # Gateway lifecycle
│   │
│   ├── ferroq-web/        # Embedded web dashboard
│   └── ferroq/            # CLI binary entry point
```

## Data Flow

### Event Flow (Backend → Bot Framework)

```
1. Backend WebSocket message arrives
2. Lagrange/NapCat adapter receives raw JSON
3. onebot_v11::parse_event() converts to internal Event
4. DedupFilter checks fingerprint (drop if duplicate)
5. PluginEngine.process_event() runs plugin chain
6. EventBus.publish() broadcasts to all subscribers
7. Protocol server receives via bus.subscribe()
8. Event serialized to protocol format (v11/v12/Satori)
9. Sent to connected bot framework via WS/HTTP POST
```

### API Flow (Bot Framework → Backend)

```
1. Bot framework sends API request (HTTP POST or WS message)
2. Protocol server parses action + params
3. Auth middleware validates token
4. Rate limiter checks token bucket
5. PluginEngine.process_api_call() runs plugin chain
6. ApiRouter resolves self_id → adapter
7. Adapter forwards to backend WebSocket
8. Response returned to bot framework
```

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **tokio broadcast channel** for event bus | O(1) publish regardless of subscriber count; subscribers get their own cursor |
| **128-bit fingerprint** for dedup | Avoids hashing full JSON; O(1) HashMap lookup |
| **trait BackendAdapter** | Adapter pattern enables Lagrange, NapCat, Mock, Failover without code changes |
| **wasmtime for plugins** | Industry-standard WASM runtime; JIT compilation; proper sandboxing |
| **Feature-gated WASM** | wasmtime is large; users who don't need plugins can exclude it |
| **parking_lot mutexes** | Faster than std Mutex for uncontended locks (most gateway operations) |
| **Arc\<EventBus\>** shared state | Event bus is read-heavy; Arc + broadcast avoids lock contention |
