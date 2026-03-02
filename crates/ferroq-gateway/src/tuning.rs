//! Runtime tuning knobs for hot-path protocol server behavior.
//!
//! These values are intentionally environment-driven so operators can tune
//! performance/latency trade-offs without rebuilding binaries.

use std::sync::OnceLock;

use tracing::warn;

const DEFAULT_WS_OUTBOUND_QUEUE_CAPACITY: usize = 1024;
const DEFAULT_WS_API_MAX_IN_FLIGHT: usize = 64;

fn parse_env_usize(name: &str, default: usize, min: usize, max: usize) -> usize {
    match std::env::var(name) {
        Ok(raw) => match raw.parse::<usize>() {
            Ok(value) if value >= min && value <= max => value,
            Ok(value) => {
                let clamped = value.clamp(min, max);
                warn!(
                    env = name,
                    configured = value,
                    min = min,
                    max = max,
                    applied = clamped,
                    "runtime tuning value out of range, clamping"
                );
                clamped
            }
            Err(_) => {
                warn!(
                    env = name,
                    value = %raw,
                    default = default,
                    "invalid runtime tuning value, falling back to default"
                );
                default
            }
        },
        Err(_) => default,
    }
}

/// Per-connection outbound queue capacity for WS pushes/responses.
///
/// Environment override: `FERROQ_WS_OUTBOUND_QUEUE_CAPACITY` (range: 64..65536).
pub fn ws_outbound_queue_capacity() -> usize {
    static VALUE: OnceLock<usize> = OnceLock::new();
    *VALUE.get_or_init(|| {
        parse_env_usize(
            "FERROQ_WS_OUTBOUND_QUEUE_CAPACITY",
            DEFAULT_WS_OUTBOUND_QUEUE_CAPACITY,
            64,
            65_536,
        )
    })
}

/// Per-connection max concurrent WS API calls.
///
/// Environment override: `FERROQ_WS_API_MAX_IN_FLIGHT` (range: 1..8192).
pub fn ws_api_max_in_flight() -> usize {
    static VALUE: OnceLock<usize> = OnceLock::new();
    *VALUE.get_or_init(|| {
        parse_env_usize(
            "FERROQ_WS_API_MAX_IN_FLIGHT",
            DEFAULT_WS_API_MAX_IN_FLIGHT,
            1,
            8_192,
        )
    })
}
