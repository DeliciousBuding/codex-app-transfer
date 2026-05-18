//! Verify shutdown signal drops axum::serve future → listener immediately freed.
//!
//! 2026-05-18 fix:用 tokio::select! 把 axum::serve future 跟 shutdown signal
//! 包一起 → signal 来时 future drop → listener 同步销毁 → 端口立即 free。
//!
//! 之前用 `with_graceful_shutdown` 在长连接 / keep-alive 场景永远 drain 不完
//! 导致端口卡占用(用户真机:点停止转发后仍能转发)。

use std::future::IntoFuture;
use std::sync::Arc;
use std::time::Duration;

use codex_app_transfer_proxy::{build_router, StaticResolver};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// 用跟 production(proxy_runner.rs::start)一样的 select! 模式 spawn server。
async fn spawn_with_signal(rx: oneshot::Receiver<()>) -> std::net::SocketAddr {
    let resolver = Arc::new(StaticResolver::new(None, vec![], None));
    let router = build_router(resolver);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let serve = axum::serve(listener, router.into_make_service()).into_future();
        tokio::pin!(serve);
        tokio::select! {
            _ = &mut serve => {}
            _ = rx => {}
        }
    });
    addr
}

/// 核心 verify:signal 来后端口必须在 500ms 内 free。
#[tokio::test]
async fn shutdown_signal_releases_port_immediately() {
    let (tx, rx) = oneshot::channel::<()>();
    let addr = spawn_with_signal(rx).await;

    // Step 1: verify port is occupied while server runs.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let bind_before = TcpListener::bind(addr).await;
    assert!(
        bind_before.is_err(),
        "expected port {} busy while server running, got: {:?}",
        addr.port(),
        bind_before
    );

    // Step 2: signal stop.
    let _ = tx.send(());

    // Step 3: verify port is free within 500ms.
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    loop {
        match TcpListener::bind(addr).await {
            Ok(_) => return,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(e) => panic!(
                "port {} not released within 500ms after stop signal: {}",
                addr.port(),
                e
            ),
        }
    }
}

/// 模拟长连接场景:有 keep-alive client 持有 connection 时,signal 来必须
/// 仍然能强制释放端口(不被 in-flight connection 卡住)。
#[tokio::test]
async fn shutdown_releases_port_even_with_active_keepalive() {
    let (tx, rx) = oneshot::channel::<()>();
    let addr = spawn_with_signal(rx).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // 建立 keep-alive client 发一个 request 占住 connection。
    let client = reqwest::Client::builder()
        .pool_idle_timeout(Some(Duration::from_secs(60)))
        .build()
        .unwrap();
    // 请求会返非 200(没 provider),但 connection 建立 + 进 keep-alive pool。
    let _ = client
        .post(format!("http://{}/responses", addr))
        .header("content-type", "application/json")
        .body(r#"{"model":"none"}"#)
        .send()
        .await;

    // 此刻 axum::serve 内部有 active connection sub-task,如果用
    // with_graceful_shutdown 会卡这里;用 select! drop 必须立刻 free。

    let _ = tx.send(());

    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    loop {
        match TcpListener::bind(addr).await {
            Ok(_) => return,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(e) => panic!(
                "port {} not released within 500ms with active keep-alive: {}",
                addr.port(),
                e
            ),
        }
    }
}
