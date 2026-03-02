//! HTTP middleware layers for ferroq.
//!
//! - **`access_token_auth`** — Bearer token / query-param authentication.

use std::sync::Arc;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

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

/// Wrap a router with access-token authentication.
///
/// This is the easiest way to protect an existing router that already
/// has its own state — no need to change the outer state type.
pub fn with_auth(router: axum::Router, token: String) -> axum::Router {
    let token = Arc::new(token);
    router.layer(axum::middleware::from_fn(move |request: Request, next: Next| {
        let t = Arc::clone(&token);
        async move { check_token(&t, request, next).await }
    }))
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
}
