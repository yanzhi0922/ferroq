//! # ferroq
//!
//! High-performance QQ Bot unified gateway.
//!
//! This is the CLI entry point.

use std::path::PathBuf;

use clap::Parser;
use tracing::info;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// ferroq — High-performance QQ Bot unified gateway
#[derive(Parser, Debug)]
#[command(name = "ferroq", version, about, long_about = None)]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "config.yaml")]
    config: PathBuf,

    /// Generate a default configuration file and exit.
    #[arg(long)]
    generate_config: bool,

    /// Override log level (debug, info, warn, error).
    #[arg(long)]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Generate config mode
    if cli.generate_config {
        let default_config = include_str!("../../../config.example.yaml");
        let output = cli.config.clone();
        std::fs::write(&output, default_config)?;
        println!("Generated default config at: {}", output.display());
        return Ok(());
    }

    // Init tracing — basic console output first, may be upgraded after config load.
    let log_level_override = cli.log_level.clone();
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let level = log_level_override.as_deref().unwrap_or("info");
        EnvFilter::new(format!("ferroq={level},ferroq_core={level},ferroq_gateway={level},ferroq_web={level}"))
    });

    // Load config first (before full tracing init) to check logging.file.
    let config_path = &cli.config;
    if !config_path.exists() {
        anyhow::bail!(
            "Config file not found: {}. Use --generate-config to create one.",
            config_path.display()
        );
    }

    let config_str = std::fs::read_to_string(config_path)?;
    let config: ferroq_core::config::AppConfig = serde_yaml::from_str(&config_str)?;

    // Initialize tracing with console + optional file output.
    // Must happen after config parse so we can read logging.file.
    let _file_guard = if let Some(ref log_file) = config.logging.file {
        // Use daily-rotating file appender.
        let file_path = std::path::Path::new(log_file);
        let dir = file_path.parent().unwrap_or(std::path::Path::new("."));
        let filename = file_path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "ferroq.log".into());

        let file_appender = tracing_appender::rolling::daily(dir, filename);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let console_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_file(false);

        let file_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_target(true)
            .with_writer(non_blocking);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(console_layer)
            .with(file_layer)
            .init();

        Some(guard)
    } else {
        // Console only.
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(true)
            .with_thread_ids(false)
            .with_file(false)
            .init();
        None
    };

    info!(version = env!("CARGO_PKG_VERSION"), "starting ferroq");
    if let Some(ref log_file) = config.logging.file {
        info!(file = %log_file, "file logging enabled");
    }

    // Validate config before proceeding.
    let issues = ferroq_core::validation::validate(&config);
    for issue in &issues {
        match issue.severity {
            ferroq_core::validation::Severity::Error => {
                tracing::error!("{issue}");
            }
            ferroq_core::validation::Severity::Warning => {
                tracing::warn!("{issue}");
            }
        }
    }
    if ferroq_core::validation::has_errors(&issues) {
        anyhow::bail!(
            "configuration has {} error(s) — fix them and restart",
            issues.iter().filter(|i| i.severity == ferroq_core::validation::Severity::Error).count()
        );
    }

    info!(
        host = %config.server.host,
        port = config.server.port,
        accounts = config.accounts.len(),
        "loaded configuration"
    );

    // Create and start the gateway runtime
    let mut runtime = ferroq_gateway::runtime::GatewayRuntime::new(config.clone());

    // Instantiate backend adapters from config.
    for account in &config.accounts {
        let primary: std::sync::Arc<dyn ferroq_core::adapter::BackendAdapter> =
            match account.backend.backend_type.as_str() {
                "lagrange" | "napcat" => {
                    // Both Lagrange and NapCat expose an OneBot v11 forward WS endpoint.
                    let adapter = ferroq_gateway::adapter::LagrangeAdapter::from_backend_config(
                        &account.name,
                        &account.backend,
                    );
                    info!(
                        name = %account.name,
                        backend = %account.backend.backend_type,
                        url = %account.backend.url,
                        "created backend adapter"
                    );
                    std::sync::Arc::new(adapter)
                }
                other => {
                    tracing::warn!(name = %account.name, backend = %other, "unknown backend type, skipping");
                    continue;
                }
            };

        // If a fallback backend is configured, wrap primary + fallback in a FailoverAdapter.
        let adapter: std::sync::Arc<dyn ferroq_core::adapter::BackendAdapter> =
            if let Some(ref fb_config) = account.fallback {
                let fallback: std::sync::Arc<dyn ferroq_core::adapter::BackendAdapter> =
                    match fb_config.backend_type.as_str() {
                        "lagrange" | "napcat" => {
                            let fb_adapter =
                                ferroq_gateway::adapter::LagrangeAdapter::from_backend_config(
                                    format!("{}-fallback", account.name),
                                    fb_config,
                                );
                            info!(
                                name = %account.name,
                                fallback_backend = %fb_config.backend_type,
                                fallback_url = %fb_config.url,
                                "created fallback adapter"
                            );
                            std::sync::Arc::new(fb_adapter)
                        }
                        other => {
                            tracing::warn!(
                                name = %account.name,
                                fallback_backend = %other,
                                "unknown fallback backend type, ignoring fallback"
                            );
                            // Use primary only, no failover.
                            primary.clone()
                        }
                    };
                // Only wrap if we didn't fall through (fallback is a clone of primary).
                if std::sync::Arc::ptr_eq(&fallback, &primary) {
                    primary
                } else {
                    info!(name = %account.name, "failover enabled");
                    std::sync::Arc::new(
                        ferroq_gateway::adapter::FailoverAdapter::new(
                            &account.name,
                            primary,
                            fallback,
                        ),
                    )
                }
            } else {
                primary
            };

        runtime.add_adapter(adapter);
    }

    // Protocol servers are instantiated below based on config.

    runtime.start().await?;

    // Create the dynamic adapter manager for runtime add/remove/reconnect.
    let adapter_manager = std::sync::Arc::new(
        ferroq_gateway::adapter_manager::AdapterManager::new(
            runtime.bus().clone(),
            runtime.router().clone(),
            runtime.stats().clone(),
            runtime.dedup().clone(),
        ),
    );

    // Build the HTTP server (dashboard + management API + protocol servers)
    let stats = runtime.stats().clone();
    let health_stats = stats.clone();

    // Shared runtime-mutable config (for hot-reload).
    let shared_config = std::sync::Arc::new(
        ferroq_gateway::shared_config::SharedConfig::new(config.server.access_token.clone()),
    );

    // Optional global rate limiter — created upfront so management can reference it.
    let rate_limiter = if config.server.rate_limit.enabled {
        let limiter = ferroq_gateway::middleware::RateLimiter::new(
            config.server.rate_limit.burst,
        );
        limiter.start_refill(config.server.rate_limit.requests_per_second);
        info!(
            rps = config.server.rate_limit.requests_per_second,
            burst = config.server.rate_limit.burst,
            "global rate limiting enabled"
        );
        Some(limiter)
    } else {
        None
    };

    // Management API routes — protected by dynamic access token middleware.
    let mgmt_router = ferroq_gateway::middleware::with_dynamic_auth(
        ferroq_gateway::management::management_routes_with_manager(
            runtime.router().clone(),
            runtime.stats().clone(),
            runtime.store().clone(),
            Some(config_path.clone()),
            std::sync::Arc::clone(&shared_config),
            rate_limiter.clone(),
            Some(std::sync::Arc::clone(&adapter_manager)),
        ),
        std::sync::Arc::clone(&shared_config),
    );

    let metrics_stats = stats.clone();
    let mut app = axum::Router::new()
        .nest("/dashboard", ferroq_web::dashboard_routes())
        .nest("/api", mgmt_router)
        .route(
            "/health",
            axum::routing::get(move || {
                let s = health_stats.clone();
                async move { axum::Json(s.health()) }
            }),
        )
        .route(
            "/metrics",
            axum::routing::get(move || {
                let s = metrics_stats.clone();
                async move {
                    (
                        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
                        s.prometheus_metrics(),
                    )
                }
            }),
        );

    // OneBot v11 protocol server.
    let onebot_v11_server = if let Some(ref ob_config) = config.protocols.onebot_v11 {
        if ob_config.enabled {
            let server = ferroq_gateway::server::OneBotV11Server::new(
                ob_config.clone(),
                std::sync::Arc::clone(&shared_config),
            );
            // Build the sub-router for /onebot/v11/*.
            let ob_router = server.build_router(
                runtime.router().clone(),
                runtime.bus().raw_sender(),
                runtime.stats().clone(),
            );
            app = app.nest("/onebot/v11", ob_router);

            // Start reverse WS and HTTP POST background tasks.
            server.start_background_tasks(
                runtime.router().clone(),
                runtime.bus().raw_sender(),
                runtime.stats().clone(),
            );

            info!("OneBot v11 protocol server enabled");
            Some(server)
        } else {
            None
        }
    } else {
        None
    };

    // Apply CORS middleware (allow all origins for API).
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    // HTTP request/response tracing.
    let trace_layer = tower_http::trace::TraceLayer::new_for_http()
        .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
        .on_response(
            tower_http::trace::DefaultOnResponse::new().level(tracing::Level::INFO),
        );

    // Apply optional rate limit middleware.
    let app = if let Some(ref limiter) = rate_limiter {
        ferroq_gateway::middleware::with_rate_limit(app, limiter.clone())
            .layer(cors)
            .layer(trace_layer)
    } else {
        app.layer(cors).layer(trace_layer)
    };

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!(addr = %addr, "HTTP server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    runtime.shutdown().await?;
    adapter_manager.shutdown().await;
    // Stop OneBot v11 background tasks.
    if let Some(ref server) = onebot_v11_server {
        server.stop_background_tasks();
    }
    info!("ferroq shut down cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("shutdown signal received");
}
