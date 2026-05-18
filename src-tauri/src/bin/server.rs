//! Standalone HTTP server binary for codex-app-transfer.
//!
//! Starts both the admin API + web UI and the proxy router on a single TCP port.
//! No Tauri dependency — deploy on any VPS or container.

use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use codex_app_transfer_app::admin::{self, registry_io, state::AdminState};
use codex_app_transfer_app::proxy_runner::ProxyManager;
use codex_app_transfer_app::telemetry_bridge;
use codex_app_transfer_proxy::{build_router, StaticResolver};
use codex_app_transfer_registry::Config;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

#[derive(Parser)]
#[command(name = "codex-app-transfer-server", version)]
struct Args {
    /// Listen port (env: PORT)
    #[arg(short, long, env = "PORT", default_value = "18081")]
    port: u16,

    /// Bind address
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,

    /// Config file path override (default: ~/.codex-app-transfer/config.json)
    #[arg(long, env = "CODEX_APP_TRANSFER_CONFIG")]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry_bridge::init_global_subscriber();
    let args = Args::parse();

    // Override config path if specified
    if let Some(ref path) = args.config {
        std::env::set_var("CODEX_APP_TRANSFER_CONFIG", path);
    }

    info!(
        port = args.port,
        bind = args.bind.as_str(),
        "codex-app-transfer-server starting"
    );

    // Load config (auto-create with defaults if missing)
    let cfg = match registry_io::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("config load failed (will use empty defaults): {e}");
            serde_json::json!({})
        }
    };

    let proxy_port = cfg
        .get("settings")
        .and_then(|s| s.get("proxyPort"))
        .and_then(|v| v.as_u64())
        .and_then(|p| u16::try_from(p).ok())
        .unwrap_or(18080);

    // Bootstrap config if no providers exist
    let providers = cfg
        .get("providers")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    if providers == 0 {
        tracing::info!(
            "no providers configured — visit http://{}:{}/ to add one via the web UI",
            args.bind,
            args.port
        );
    }

    // Build resolver for proxy router
    let typed_cfg: Config = serde_json::from_value(cfg.clone())
        .unwrap_or_else(|_| Config::default());
    let gateway_key = typed_cfg.gateway_api_key.filter(|s| !s.is_empty());
    let proxy_resolver = Arc::new(StaticResolver::new(
        gateway_key,
        typed_cfg.providers.clone(),
        typed_cfg.active_provider.clone(),
    ));

    // Build routers
    let proxy_manager = Arc::new(ProxyManager::new());
    let admin_state = AdminState {
        proxy_manager: proxy_manager.clone(),
    };
    let admin_router = admin::build_app_router(admin_state);
    let proxy_router = build_router(proxy_resolver);

    // Merge: admin routes first (API + frontend), proxy fallback catches /responses etc.
    let app = admin_router
        .merge(proxy_router)
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .layer(TraceLayer::new_for_http());

    // Auto-start proxy on the configured proxy port
    match proxy_manager.start(proxy_port).await {
        Ok(s) => {
            info!(
                proxy_port = proxy_port,
                running = s.running,
                "proxy auto-started"
            );
        }
        Err(e) => {
            tracing::warn!(error = %e, proxy_port = proxy_port, "proxy auto-start failed (will retry on demand)");
        }
    }

    let addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    info!("listening on http://{addr}");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}
