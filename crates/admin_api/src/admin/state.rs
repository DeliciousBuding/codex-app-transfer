//! Admin server 共享状态.

use std::sync::Arc;

use codex_app_transfer_proxy_runner::ProxyManager;

#[derive(Clone)]
pub struct AdminState {
    pub proxy_manager: Arc<ProxyManager>,
}
