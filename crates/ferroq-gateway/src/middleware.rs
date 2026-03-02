//! HTTP middleware layers for ferroq.
//!
//! - **`access_token_auth`** — Bearer token / query-param authentication.
//! - **`RateLimiter`** — Global token-bucket rate limiter.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::shared_config::SharedConfig;

/// Shared state that holds the expected access token.
#[derive(Clone)]
pub struct AuthState {
    /// The expected token. If empty, authentication is disabled.
    pub token: Arc<String>,
}

/// Axum middleware function that enforces access-token authentication.
///
/// The token can be supplied in two ways (checked in order):
/// 1. `Authorization: Bearer <token>` header
/// 2. `?access_token=<token>` query parameter
///
/// If the configured token is empty, all requests pass through.
pub async fn access_token_auth(
    axum::extract::State(auth): axum::extract::State<AuthState>,
    request: Request,
    next: Next,
) -> Response {
    check_token(&auth.token, request, next).await
}

/// Wrap a router with access-token authentication (static token).
///
/// For a dynamically reloadable token, use [`with_dynamic_auth`] instead.
pub fn with_auth(router: axum::Router, token: String) -> axum::Router {
    let token = Arc::new(token);
    router.layer(axum::middleware::from_fn(
        move |request: Request, next: Next| {
            let t = Arc::clone(&token);
            async move { check_token(&t, request, next).await }
        },
    ))
}

/// Wrap a router with access-token authentication backed by [`SharedConfig`].
///
/// The token is read from `SharedConfig` on every request, so changes made via
/// hot-reload take effect immediately.
pub fn with_dynamic_auth(router: axum::Router, shared: Arc<SharedConfig>) -> axum::Router {
    router.layer(axum::middleware::from_fn(
        move |request: Request, next: Next| {
            let cfg = Arc::clone(&shared);
            async move {
                let token = cfg.access_token();
                check_token(&token, request, next).await
            }
        },
    ))
}

async fn check_token(token: &str, request: Request, next: Next) -> Response {
    // If no token is configured, skip authentication.
    if token.is_empty() {
        return next.run(request).await;
    }

    // 1. Check Authorization header.
    if let Some(header) = request.headers().get("authorization") {
        if let Ok(value) = header.to_str() {
            if let Some(bearer) = value.strip_prefix("Bearer ") {
                if bearer == token {
                    return next.run(request).await;
                }
            }
            // Also accept plain token (without "Bearer " prefix).
            if value == token {
                return next.run(request).await;
            }
        }
    }

    // 2. Check query parameter.
    if let Some(query) = request.uri().query() {
        for pair in query.split('&') {
            if let Some(("access_token", val)) = pair.split_once('=') {
                if val == token {
                    return next.run(request).await;
                }
            }
        }
    }

    (StatusCode::UNAUTHORIZED, "access token required or invalid").into_response()
}

// ---------------------------------------------------------------------------
// Rate limiter
// ---------------------------------------------------------------------------

/// A global token-bucket rate limiter.
///
/// Call [`RateLimiter::start_refill`] to spawn the background refill task, then
/// use [`with_rate_limit`] to wrap an axum Router.
///
/// Both `rps` and `burst` are stored as atomics so they can be updated at
/// runtime via [`update_config`](RateLimiter::update_config) during a
/// hot-reload without restarting the refill task.
#[derive(Clone)]
pub struct RateLimiter {
    tokens: Arc<AtomicU32>,
    burst: Arc<AtomicU32>,
    rps: Arc<AtomicU32>,
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// - `burst`: maximum number of tokens (= requests) in the bucket.
    pub fn new(burst: u32) -> Self {
        Self {
            tokens: Arc::new(AtomicU32::new(burst)),
            burst: Arc::new(AtomicU32::new(burst)),
            rps: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Spawn a background task that refills tokens every second.
    ///
    /// The task reads `rps` and `burst` from atomics each tick, so
    /// changes made via [`update_config`](Self::update_config) take
    /// effect within one second.
    pub fn start_refill(&self, rps: u32) -> tokio::task::JoinHandle<()> {
        self.rps.store(rps, Ordering::Relaxed);
        let tokens = Arc::clone(&self.tokens);
        let burst = Arc::clone(&self.burst);
        let rps_atomic = Arc::clone(&self.rps);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                let r = rps_atomic.load(Ordering::Relaxed);
                let b = burst.load(Ordering::Relaxed);
                tokens
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                        Some(current.saturating_add(r).min(b))
                    })
                    .ok();
            }
        })
    }

    /// Try to consume one token. Returns `true` if allowed.
    pub fn try_acquire(&self) -> bool {
        self.tokens
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                if current > 0 { Some(current - 1) } else { None }
            })
            .is_ok()
    }

    /// Update rate limit parameters at runtime (hot-reload).
    ///
    /// The background refill task picks up the new values on its next tick.
    pub fn update_config(&self, rps: u32, burst: u32) {
        self.rps.store(rps, Ordering::Relaxed);
        self.burst.store(burst, Ordering::Relaxed);
    }
}

/// Wrap a router with global rate limiting.
///
/// Returns HTTP 429 when the bucket is empty.
pub fn with_rate_limit(router: axum::Router, limiter: RateLimiter) -> axum::Router {
    router.layer(axum::middleware::from_fn(
        move |request: Request, next: Next| {
            let rl = limiter.clone();
            async move {
                if rl.try_acquire() {
                    next.run(request).await
                } else {
                    let mut response =
                        (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded").into_response();
                    response.headers_mut().insert(
                        axum::http::header::RETRY_AFTER,
                        axum::http::HeaderValue::from_static("1"),
                    );
                    response
                }
            }
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::middleware;
    use axum::routing::get;
    use tower::ServiceExt;

    fn build_app(token: &str) -> axum::Router {
        let auth = AuthState {
            token: Arc::new(token.to_string()),
        };
        axum::Router::new()
            .route("/protected", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(
                auth.clone(),
                access_token_auth,
            ))
            .with_state(auth)
    }

    #[tokio::test]
    async fn no_token_configured_allows_all() {
        let app = build_app("");
        let req = HttpRequest::builder()
            .uri("/protected")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn valid_bearer_header() {
        let app = build_app("secret123");
        let req = HttpRequest::builder()
            .uri("/protected")
            .header("Authorization", "Bearer secret123")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn valid_query_param() {
        let app = build_app("secret123");
        let req = HttpRequest::builder()
            .uri("/protected?access_token=secret123")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn invalid_token_rejected() {
        let app = build_app("secret123");
        let req = HttpRequest::builder()
            .uri("/protected")
            .header("Authorization", "Bearer wrong")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn missing_token_rejected() {
        let app = build_app("secret123");
        let req = HttpRequest::builder()
            .uri("/protected")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn plain_header_token_accepted() {
        let app = build_app("mytoken");
        let req = HttpRequest::builder()
            .uri("/protected")
            .header("Authorization", "mytoken")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rate_limiter_allows_within_burst() {
        let rl = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(rl.try_acquire());
        }
        // 6th request exceeds burst — should fail.
        assert!(!rl.try_acquire());
    }

    #[tokio::test]
    async fn rate_limiter_refill() {
        let rl = RateLimiter::new(2);
        // Exhaust tokens.
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        assert!(!rl.try_acquire());

        // Start refill — 10 per second.
        let _handle = rl.start_refill(10);
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

        // After 1 second, tokens should be refilled.
        assert!(rl.try_acquire());
    }

    #[tokio::test]
    async fn rate_limiter_middleware_returns_429() {
        let rl = RateLimiter::new(1);
        let app = with_rate_limit(
            axum::Router::new().route("/test", get(|| async { "ok" })),
            rl,
        );

        // First request should pass.
        let req1 = HttpRequest::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        let app2 = app.clone();
        let resp1 = app.oneshot(req1).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);

        // Second request should be rate limited.
        let req2 = HttpRequest::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        let resp2 = app2.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);

        // Verify the Retry-After header is present.
        assert_eq!(
            resp2
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()),
            Some("1"),
        );
    }
}
