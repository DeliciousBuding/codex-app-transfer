//! Admin HTTP API — 从 src-tauri/src/admin/ 整体迁出独立 crate (W1)。
//!
//! 设计:不绑端口,通过 Tauri 的自定义 URI scheme(`cas://localhost/`)将
//! webview 请求路由进 axum router(`tower::ServiceExt::oneshot`),全程同
//! 进程,无 TCP 往返。
//!
//! 后续 desktop_app(W2+)直接 in-process 调 ProxyManager / Codex 集成,
//! 不走 HTTP;但 admin_api 仍保留供 headless tools / CLI 用户脚本调用。

pub mod admin;

pub use admin::{build_app_router, AdminState};
