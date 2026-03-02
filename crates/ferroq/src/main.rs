//! # ferroq
//!
//! High-performance QQ Bot unified gateway.
//!
//! This is the CLI entry point.

use std::path::PathBuf;

use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

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
        EnvFilter::new(format!(
            "ferroq={level},ferroq_core={level},ferroq_gateway={level},ferroq_web={level}"
        ))
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
            issues
                .iter()
                .filter(|i| i.severity == ferroq_core::validation::Severity::Error)
                .count()
        );
    }

    info!(
        host = %config.server.host,
        port = config.server.port,
        accounts = config.accounts.len(),
        "loaded configuration"
    );

    // Initialize the WASM plugin engine
    let plugin_engine = std::sync::Arc::new(
        ferroq_gateway::plugin_engine::PluginEngine::new()
            .map_err(|e| anyhow::anyhow!("failed to create plugin engine: {}", e))?,
    );

    // Load plugins from config
    if !config.plugins.is_empty() {
        info!(count = config.plugins.len(), "loading WASM plugins");
        if let Err(e) = plugin_engine.load_plugins(&config.plugins) {
            tracing::warn!(error = %e, "some plugins failed to load");
        }
        for plugin_info in plugin_engine.list_plugins() {
            info!(
                name = %plugin_info.name,
                version = %plugin_info.version,
                "plugin ready"
            );
        }
    }

    // Create and start the gateway runtime
    let mut runtime = ferroq_gateway::runtime::GatewayRuntime::new(config.clone());

    // Instantiate backend adapters from config.
    for account in &config.accounts {
        let primary: std::sync::Arc<dyn ferroq_core::adapter::BackendAdapter> = match account
            .backend
            .backend_type
            .as_str()
        {
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
            "official" => {
                let adapter = ferroq_gateway::adapter::OfficialAdapter::from_backend_config(
                    &account.name,
                    &account.backend,
                )
                .map_err(|e| anyhow::anyhow!("failed to create official adapter: {}", e))?;
                info!(
                    name = %account.name,
                    backend = %account.backend.backend_type,
                    url = %account.backend.url,
                    "created official backend adapter"
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
                        "official" => {
                            let fb_adapter =
                                ferroq_gateway::adapter::OfficialAdapter::from_backend_config(
                                    format!("{}-fallback", account.name),
                                    fb_config,
                                )
                                .map_err(|e| {
                                    anyhow::anyhow!(
                                        "failed to create official fallback adapter: {}",
                                        e
                                    )
                                })?;
                            info!(
                                name = %account.name,
                                fallback_backend = %fb_config.backend_type,
                                fallback_url = %fb_config.url,
                                "created official fallback adapter"
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
                    std::sync::Arc::new(ferroq_gateway::adapter::FailoverAdapter::new(
                        &account.name,
                        primary,
                        fallback,
                    ))
                }
            } else {
                primary
            };

        runtime.add_adapter(adapter);
    }

    // Protocol servers are instantiated below based on config.

    // Create the adapter manager before starting the runtime so that startup
    // adapters are registered with it (enabling management API operations on
    // all adapters, not just dynamically added ones).
    let adapter_manager =
        std::sync::Arc::new(ferroq_gateway::adapter_manager::AdapterManager::new(
            runtime.bus().clone(),
            runtime.router().clone(),
            runtime.stats().clone(),
            runtime.dedup().clone(),
        ));

    runtime
        .start(&adapter_manager, std::sync::Arc::clone(&adapter_manager))
        .await?;

    // Build the HTTP server (dashboard + management API + protocol servers)
    let stats = runtime.stats().clone();
    let health_stats = stats.clone();

    // Shared runtime-mutable config (for hot-reload).
    let shared_config = std::sync::Arc::new(ferroq_gateway::shared_config::SharedConfig::new(
        config.server.access_token.clone(),
    ));

    // Optional global rate limiter — created upfront so management can reference it.
    let rate_limiter = if config.server.rate_limit.enabled {
        let limiter = ferroq_gateway::middleware::RateLimiter::new(config.server.rate_limit.burst);
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
                        [(
                            axum::http::header::CONTENT_TYPE,
                            "text/plain; version=0.0.4; charset=utf-8",
                        )],
                        s.prometheus_metrics(),
                    )
                }
            }),
        );

    if config.server.dashboard {
        app = app
            .route(
                "/dashboard/",
                axum::routing::get(|| async { axum::response::Redirect::temporary("/dashboard") }),
            )
            .nest("/dashboard", ferroq_web::dashboard_routes());
        info!("embedded dashboard enabled");
    } else {
        info!("embedded dashboard disabled by config");
    }

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

    // OneBot v12 protocol server.
    if let Some(ref ob12_config) = config.protocols.onebot_v12 {
        if ob12_config.enabled {
            let server = ferroq_gateway::server::OneBotV12Server::new(
                ob12_config.clone(),
                std::sync::Arc::clone(&shared_config),
            );
            let ob12_router = server.build_router(
                runtime.router().clone(),
                runtime.bus().raw_sender(),
                runtime.stats().clone(),
            );
            app = app.nest("/onebot/v12", ob12_router);
            info!("OneBot v12 protocol server enabled");
        }
    }

    // Satori protocol server.
    if let Some(ref satori_config) = config.protocols.satori {
        if satori_config.enabled {
            let server = ferroq_gateway::server::SatoriServer::new(
                satori_config.clone(),
                std::sync::Arc::clone(&shared_config),
            );
            let satori_router = server.build_router(
                runtime.router().clone(),
                runtime.bus().raw_sender(),
                runtime.stats().clone(),
            );
            app = app.nest("/satori/v1", satori_router);
            info!("Satori protocol server enabled");
        }
    }

    // Apply CORS middleware (allow all origins for API).
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    // HTTP request/response tracing.
    let trace_layer = tower_http::trace::TraceLayer::new_for_http()
        .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
        .on_response(tower_http::trace::DefaultOnResponse::new().level(tracing::Level::INFO));

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

    // Shut down in order: protocol servers → adapters → runtime background tasks.
    if let Some(ref server) = onebot_v11_server {
        server.stop_background_tasks();
    }
    adapter_manager.shutdown().await;
    runtime.shutdown().await?;
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
