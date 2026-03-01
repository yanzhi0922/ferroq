//! # ferroq-web
//!
//! Web dashboard for ferroq — provides a browser-based UI for monitoring
//! and managing the gateway.

/// Placeholder module — the web dashboard will be implemented in Phase 2.
pub fn dashboard_routes() -> axum::Router {
    use axum::routing::get;
    use axum::response::Html;

    axum::Router::new().route(
        "/",
        get(|| async {
            Html(
                r#"<!DOCTYPE html>
<html>
<head><title>ferroq</title><meta charset="utf-8">
<style>
  body { font-family: -apple-system, sans-serif; display: flex; align-items: center;
         justify-content: center; height: 100vh; margin: 0; background: #0d1117; color: #c9d1d9; }
  .card { text-align: center; }
  h1 { font-size: 3em; margin-bottom: 0.2em; }
  p { color: #8b949e; }
  code { background: #161b22; padding: 2px 6px; border-radius: 4px; }
</style>
</head>
<body>
  <div class="card">
    <h1>⚡ ferroq</h1>
    <p>High-performance QQ Bot unified gateway</p>
    <p>Dashboard coming in <code>Phase 2</code></p>
  </div>
</body>
</html>"#,
            )
        }),
    )
}
