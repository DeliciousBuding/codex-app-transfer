//! 验证 stop 强制释放端口 + 强制 abort in-flight connection sub-task。
//!
//! 2026-05-18 真机复现:发过 1 条 message 后点停止转发,转发不受影响 →
//! 根因:`axum::serve` 内部 `tokio::spawn` per-connection 出 detached task,
//! 外部无 handle,select drop axum::serve future 只断 accept loop +
//! listener,已建立的 connection sub-task 仍 alive process 同 connection
//! 上的 keep-alive request。
//!
//! 修复:自己写 accept loop,per-connection task 用 `JoinSet.spawn`,stop
//! 时 `connections.shutdown().await` 强制 abort 所有 in-flight sub-task。

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use codex_app_transfer_proxy::{build_router, StaticResolver};
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

/// 复刻 production(proxy_runner.rs::start)的 accept-loop + JoinSet +
/// CancellationToken per-sub-task select 模式,test 路径必须跟 production
/// 一致才能 verify 真实行为。
async fn spawn_production_like(
    rx: oneshot::Receiver<()>,
    cancel: CancellationToken,
) -> std::net::SocketAddr {
    let resolver = Arc::new(StaticResolver::new(None, vec![], None));
    let router = build_router(resolver);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mut rx = rx;
    tokio::spawn(async move {
        let mut connections: JoinSet<()> = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = &mut rx => break,
                accept = listener.accept() => {
                    let Ok((stream, _)) = accept else { continue };
                    let io = TokioIo::new(stream);
                    let tower_service = router.clone().map_request(
                        |req: Request<Incoming>| req.map(Body::new),
                    );
                    let hyper_service = TowerToHyperService::new(tower_service);
                    let conn_cancel = cancel.child_token();
                    connections.spawn(async move {
                        let builder = Builder::new(TokioExecutor::new());
                        let conn = builder.serve_connection_with_upgrades(io, hyper_service);
                        tokio::pin!(conn);
                        tokio::select! {
                            _ = conn.as_mut() => {}
                            _ = conn_cancel.cancelled() => {}
                        }
                    });
                }
            }
        }
        drop(listener);
        connections.shutdown().await;
    });
    addr
}

/// Test 1: shutdown signal 后端口 ≤500ms 释放(基础)。
#[tokio::test]
async fn shutdown_releases_port_immediately() {
    let (tx, rx) = oneshot::channel::<()>();
    let cancel = CancellationToken::new();
    let addr = spawn_production_like(rx, cancel.clone()).await;

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        TcpListener::bind(addr).await.is_err(),
        "port {} should be busy before stop",
        addr.port()
    );

    cancel.cancel();
    let _ = tx.send(());

    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    loop {
        match TcpListener::bind(addr).await {
            Ok(_) => return,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(e) => panic!("port not released within 500ms: {}", e),
        }
    }
}

/// Test 2: **关键 test** —— in-flight keep-alive connection 持有方,stop
/// 后**同一 connection 上的下一个 request 必须失败**(connection 被强制
/// abort)。**之前 PR #209 select! 方案 fail 在这里**:axum::serve 内部
/// 裸 tokio::spawn 出 detached task,future drop 不影响 → 同 connection
/// 上后续 req 仍能 process。
#[tokio::test]
async fn stop_aborts_inflight_keepalive_connection() {
    let (tx, rx) = oneshot::channel::<()>();
    let cancel = CancellationToken::new();
    let addr = spawn_production_like(rx, cancel.clone()).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // client 配 keep-alive,pool 长持 connection
    let client = reqwest::Client::builder()
        .pool_idle_timeout(Some(Duration::from_secs(60)))
        .pool_max_idle_per_host(1)
        .build()
        .unwrap();

    // req1:建立 connection,完成 response,connection 进 keep-alive pool
    let url = format!("http://{}/responses", addr);
    let resp1 = client
        .post(&url)
        .header("content-type", "application/json")
        .body(r#"{"model":"none"}"#)
        .send()
        .await
        .expect("req1 should succeed (proxy running)");
    let _ = resp1.bytes().await; // drain body so connection returns to pool

    // 触发 stop:cancel.cancel() 同步 wake sub-task select → drop conn →
    // socket close;send(()) 让 accept loop break → listener drop
    cancel.cancel();
    let _ = tx.send(());

    // 等 sub-task select arm fire + conn drop + TCP FIN/RST 发出
    tokio::time::sleep(Duration::from_millis(200)).await;

    // req2 用同一 client(同一 keep-alive pool),期望失败:
    //   - 如果 reqwest 复用 pool 里的旧 connection → server 端 sub-task 已被
    //     abort → connection 已 close → client 收 RST → IO error
    //   - 如果 reqwest 判 connection dead 尝试新 connection → listener 已
    //     drop → connection refused
    // **两种情况都必须 error**,不能成功返回 200
    let resp2 = client
        .post(&url)
        .header("content-type", "application/json")
        .body(r#"{"model":"none"}"#)
        .send()
        .await;
    assert!(
        resp2.is_err(),
        "req2 after stop MUST fail (connection abort + port released), got: {:?}",
        resp2.map(|r| r.status())
    );
}
