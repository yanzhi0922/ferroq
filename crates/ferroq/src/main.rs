//! # ferroq
//!
//! High-performance QQ Bot unified gateway.
//!
//! This is the CLI entry point.

use std::path::PathBuf;

use clap::Parser;
use tracing::info;
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

    // Init tracing
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let level = cli.log_level.as_deref().unwrap_or("info");
        EnvFilter::new(format!("ferroq={level},ferroq_core={level},ferroq_gateway={level},ferroq_web={level}"))
    });

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .init();

    info!(version = env!("CARGO_PKG_VERSION"), "starting ferroq");

    // Load config
    let config_path = &cli.config;
    if !config_path.exists() {
        anyhow::bail!(
            "Config file not found: {}. Use --generate-config to create one.",
            config_path.display()
        );
    }

    let config_str = std::fs::read_to_string(config_path)?;
    let config: ferroq_core::config::AppConfig = serde_yaml::from_str(&config_str)?;

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
        let adapter = match account.backend.backend_type.as_str() {
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
                std::sync::Arc::new(adapter) as std::sync::Arc<dyn ferroq_core::adapter::BackendAdapter>
            }
            other => {
                tracing::warn!(name = %account.name, backend = %other, "unknown backend type, skipping");
                continue;
            }
        };
        runtime.add_adapter(adapter);
    }

    // TODO: Phase 1.4 — instantiate protocol servers based on config

    runtime.start().await?;

    // Build the HTTP server (dashboard + protocol servers)
    let mut app = axum::Router::new()
        .nest("/dashboard", ferroq_web::dashboard_routes())
        .route("/health", axum::routing::get(|| async { "ok" }));

    // OneBot v11 protocol server.
    let onebot_v11_server = if let Some(ref ob_config) = config.protocols.onebot_v11 {
        if ob_config.enabled {
            let server = ferroq_gateway::server::OneBotV11Server::new(
                ob_config.clone(),
                config.server.access_token.clone(),
            );
            // Build the sub-router for /onebot/v11/*.
            let ob_router = server.build_router(
                runtime.router().clone(),
                runtime.bus().raw_sender(),
            );
            app = app.nest("/onebot/v11", ob_router);

            // Start reverse WS and HTTP POST background tasks.
            server.start_background_tasks(
                runtime.router().clone(),
                runtime.bus().raw_sender(),
            );

            info!("OneBot v11 protocol server enabled");
            Some(server)
        } else {
            None
        }
    } else {
        None
    };

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!(addr = %addr, "HTTP server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    runtime.shutdown().await?;
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
