//! Standalone HTTP server binary for codex-app-transfer.
//!
//! Runs both the admin API + web UI and the proxy router.
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
    /// Admin/Web UI listen port (env: PORT)
    #[arg(short, long, env = "PORT", default_value = "18081")]
    port: u16,

    /// Proxy listen port (env: PROXY_PORT)
    #[arg(long, env = "PROXY_PORT", default_value = "18080")]
    proxy_port: u16,

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

    if let Some(ref path) = args.config {
        std::env::set_var("CODEX_APP_TRANSFER_CONFIG", path);
    }

    info!(
        admin_port = args.port,
        proxy_port = args.proxy_port,
        bind = args.bind,
        "codex-app-transfer-server starting"
    );

    let cfg = match registry_io::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("config load failed (will use empty defaults): {e}");
            serde_json::json!({})
        }
    };

    let providers = cfg
        .get("providers")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    if providers == 0 {
        info!(
            "no providers configured — visit http://{}:{}/ to add one",
            args.bind, args.port
        );
    }

    let typed_cfg: Config = serde_json::from_value(cfg).unwrap_or_default();
    let gateway_key = typed_cfg.gateway_api_key.filter(|s| !s.is_empty());
    let proxy_resolver = Arc::new(StaticResolver::new(
        gateway_key,
        typed_cfg.providers.clone(),
        typed_cfg.active_provider.clone(),
    ));

    let proxy_manager = Arc::new(ProxyManager::new());

    // ── Admin router (API + Web UI) ──
    let admin_state = AdminState {
        proxy_manager: proxy_manager.clone(),
    };
    let admin_app = admin::build_app_router(admin_state)
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .layer(TraceLayer::new_for_http());

    let admin_addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    info!("admin UI listening on http://{admin_addr}");
    let admin_listener = TcpListener::bind(admin_addr).await?;
    tokio::spawn(async move {
        let _ = axum::serve(admin_listener, admin_app.into_make_service()).await;
    });

    // ── Proxy router ──
    let proxy_app = build_router(proxy_resolver)
        .layer(TraceLayer::new_for_http());

    let proxy_addr: SocketAddr = format!("{}:{}", args.bind, args.proxy_port).parse()?;
    info!("proxy listening on http://{proxy_addr}");
    let proxy_listener = TcpListener::bind(proxy_addr).await?;
    axum::serve(proxy_listener, proxy_app.into_make_service()).await?;

    Ok(())
}
