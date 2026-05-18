//! 内嵌 axum 代理生命周期管理(Stage 4.3 + Stage 5).
//!
//! Tauri 主进程启动时构造一个 [`ProxyManager`] 注入到 `State<T>`,前端通过
//! `start_proxy` / `stop_proxy` / `proxy_status` 命令操控,Tauri 主进程
//! 退出时通过 [`ProxyManager::stop_silent`] **同步**关闭代理。
//!
//! 设计要点:
//! - 内部 `std::sync::Mutex<Option<ProxyHandle>>` —— 锁持有时间极短(只读/写
//!   单个 Option),没有跨 await,**stop / status / stop_silent 全部是同步方法**,
//!   方便从 Tauri 的 `RunEvent::Exit` 同步路径调用而不需要 `block_on`。
//! - **`start` 是 async**(TcpListener::bind 必需),但锁取放都在显式 scope 里,
//!   不跨越 await。
//! - **生命周期**:`start` 时 spawn tokio task 跑自己写的 accept loop —
//!   每个 connection 用 `JoinSet.spawn` 而**不**是裸 `tokio::spawn`(关键区别)。
//!   `stop` / `stop_silent` 通过 `oneshot::Sender::send(())` 触发 select 退 →
//!   listener drop(端口立即 free)→ `joinset.abort_all()` 强制 cancel **所有**
//!   in-flight connection sub-task → reqwest stream / SSE 同步被打断 →
//!   client 收 FIN/RST connection 立即断。
//! - **为什么不用 `axum::serve`**:它内部对每个 connection 用裸 `tokio::spawn`
//!   出 detached task,外部无 handle,即使 outer future drop sub-task 仍 alive
//!   继续 process 同 connection 上的 keep-alive request → 用户感知"停止后还
//!   在转发"(2026-05-18 真机复现:发过 1 条 message 后 stop 转发不受影响,
//!   PR #209 select! 方案不够强制)。`JoinSet.abort_all()` 是唯一保证所有
//!   in-flight connection 同步 abort 的方式。

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::Request;
use codex_app_transfer_proxy::{build_router_with_gate, StaticResolver};
use codex_app_transfer_registry::{config_file, Config};
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::{JoinHandle, JoinSet};
use tower::ServiceExt;

#[derive(Debug, Serialize, Clone)]
pub struct ProxyStatus {
    pub running: bool,
    pub addr: Option<String>,
    /// 当前生效的 gateway 鉴权状态 —— 仅当代理 running 且配置了 gateway_api_key
    /// 时才是 `true`;running 但未配 key 表示"无鉴权调试模式"。
    pub gateway_auth: bool,
    pub provider_count: usize,
    pub active_provider: Option<String>,
}

struct ProxyHandle {
    addr: SocketAddr,
    /// 停止信号 — `stop` / `stop_silent` send 给跑着的 accept-loop task,
    /// task 收到后 break accept loop → listener drop(端口立即 free)→
    /// `connections.shutdown().await`(abort_all + 等所有 connection sub-task
    /// 死)→ in-flight reqwest stream / SSE 全部被打断。
    shutdown_tx: oneshot::Sender<()>,
    /// accept-loop task 的 JoinHandle — 主路径靠 shutdown_tx + JoinSet
    /// abort_all 同步释放端口 + cancel 所有 in-flight,`task.abort()` 作为
    /// 最后兜底。
    ///
    /// 历史踩坑(2026-05-18):
    /// - PR #207 task.abort:tokio cancellation 异步,不传到 spawn 出去的
    ///   sub-task,且 abort 滞后
    /// - PR #209 select! + axum::serve:select drop axum::serve future 只断
    ///   outer accept loop + listener,但 axum::serve 内部用裸 tokio::spawn
    ///   出的 per-connection sub-task 是 detached,future drop 不影响 →
    ///   已建立的 connection 仍 process keep-alive request → 端口释放但
    ///   旧 connection 上"还在转发"。本 PR 自己写 accept loop 用 JoinSet
    ///   spawn 拿到所有 connection task handle,abort_all 同步 cancel。
    task: JoinHandle<()>,
    /// Application-level shutdown gate(in-flight request 二保险):虽然
    /// `connections.shutdown()` 会 abort 所有 sub-task,但 signal send →
    /// task wake → abort_all 生效之间有窗口。这个 gate 在 stop 时先 set
    /// true,router 顶层 middleware 检查后立刻 503 + Connection: close 任
    /// 何窗口内的 req。
    gate: Arc<AtomicBool>,
    gateway_auth: bool,
    provider_count: usize,
    active_provider: Option<String>,
}

#[derive(Default)]
pub struct ProxyManager {
    handle: Mutex<Option<ProxyHandle>>,
}

impl ProxyManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 启动代理监听 `127.0.0.1:<port>`。已 running 时沿用旧版语义返回当前状态。
    pub async fn start(&self, port: u16) -> Result<ProxyStatus, String> {
        // 1. 预检查(短锁)
        {
            let guard = self.handle.lock().unwrap();
            if let Some(h) = guard.as_ref() {
                return Ok(ProxyStatus {
                    running: true,
                    addr: Some(h.addr.to_string()),
                    gateway_auth: h.gateway_auth,
                    provider_count: h.provider_count,
                    active_provider: h.active_provider.clone(),
                });
            }
        }

        // 2. 装载 resolver + 绑定 listener(async)
        let snapshot = load_resolver_snapshot()?;
        let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .map_err(|e| format!("bind 127.0.0.1:{port} failed: {e}"))?;
        let addr = listener
            .local_addr()
            .map_err(|e| format!("cannot read listener address: {e}"))?;
        // Application-level gate:start 时 false(放行),stop 时 set true →
        // router 顶层 middleware 检查后 503 + close 任何 in-flight sub-task
        // 的后续 request。
        let gate = Arc::new(AtomicBool::new(false));
        let router = build_router_with_gate(Arc::new(snapshot.resolver), gate.clone());
        let (tx, mut rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            // 自己写 accept loop 替代 axum::serve,关键差异:
            // 1. per-connection task 用 `JoinSet.spawn` 而**不**是裸
            //    `tokio::spawn`(axum::serve 内部用裸 spawn,task detached 无 handle)
            // 2. shutdown signal → break accept loop → listener drop → 然后
            //    `connections.abort_all()` 同步强制 cancel **所有** in-flight
            //    connection sub-task → reqwest stream / SSE / keep-alive
            //    request 全部立即被打断,client 收 FIN/RST connection 关闭
            // 3. drop(listener) 后端口立即 free,不等任何 in-flight drain
            //
            // 之前两种方案都失败(2026-05-18 真机复现):
            // - PR #207 `task.abort()`:tokio 异步 cancellation 不传到 spawn 出去
            //   的 sub-task,且 abort 滞后
            // - PR #209 `tokio::select! { axum::serve, rx }`:select drop axum::serve
            //   future 只断 outer accept loop + listener,但 axum::serve 内部
            //   `tokio::spawn` 的 per-connection sub-task 是 detached,future drop
            //   不影响 → 用户发过 1 条 message 后 stop,connection sub-task 仍 alive
            //   process 后续 keep-alive request → 看到"停止后还在转发"
            let mut connections: JoinSet<()> = JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = &mut rx => break,
                    accept = listener.accept() => {
                        let Ok((stream, _peer)) = accept else { continue };
                        let io = TokioIo::new(stream);
                        // Router 是 Service<Request<Body>>,hyper serve_connection
                        // 要 Service<Request<Incoming>>,用 map_request 桥接 body 类型
                        let tower_service = router.clone().map_request(
                            |req: Request<Incoming>| req.map(Body::new),
                        );
                        let hyper_service = TowerToHyperService::new(tower_service);
                        connections.spawn(async move {
                            let builder = Builder::new(TokioExecutor::new());
                            let _ = builder
                                .serve_connection_with_upgrades(io, hyper_service)
                                .await;
                        });
                    }
                }
            }
            // Listener 在这里 drop → 端口立即 free(新 connection refused)
            drop(listener);
            // 强制 cancel 所有 in-flight connection sub-task — 这是 fix 的核心
            connections.shutdown().await;
        });

        // 3. 落盘 handle(短锁;若期间被另一路径插入,关掉自己回滚)
        let new_handle = ProxyHandle {
            addr,
            shutdown_tx: tx,
            task,
            gate,
            gateway_auth: snapshot.gateway_auth,
            provider_count: snapshot.provider_count,
            active_provider: snapshot.active_provider.clone(),
        };
        let mut guard = self.handle.lock().unwrap();
        if guard.is_some() {
            // race condition,自己的 listener 让出去:set gate + send shutdown + abort task
            // (gate set 先于 abort 让 in-flight sub-task 立刻拒新 request)
            new_handle.gate.store(true, Ordering::Release);
            let _ = new_handle.shutdown_tx.send(());
            new_handle.task.abort();
            return Err("proxy already started by another path".to_owned());
        }
        *guard = Some(new_handle);
        Ok(ProxyStatus {
            running: true,
            addr: Some(addr.to_string()),
            gateway_auth: snapshot.gateway_auth,
            provider_count: snapshot.provider_count,
            active_provider: snapshot.active_provider,
        })
    }

    /// 触发 graceful shutdown 后立刻 abort task 释放端口。未 running 时报错。
    ///
    /// **为什么两步**:`shutdown_tx.send(())` 给 axum graceful drain 机会让
    /// 正常完成的 request 跑完;紧跟 `task.abort()` 同步 drop server future
    /// → listener drop → **端口立刻释放**,不会卡在 in-flight SSE / long
    /// polling / hung connection 上(那种 connection 可能永远 drain 不完)。
    /// 代价:in-flight connection 被强断,client 见 connection reset —
    /// 但"用户点停止 = 真要停",这是合理 trade-off。
    #[allow(dead_code)]
    pub fn stop(&self) -> Result<(), String> {
        let mut guard = self.handle.lock().unwrap();
        match guard.take() {
            Some(h) => {
                // 顺序重要:gate 先 set true 让 in-flight sub-task 下次
                // process request 立刻 hit middleware 503,然后再 send graceful
                // shutdown signal + abort task 释放端口。
                h.gate.store(true, Ordering::Release);
                let _ = h.shutdown_tx.send(());
                h.task.abort();
                Ok(())
            }
            None => Err("proxy is not running".to_owned()),
        }
    }

    /// 静默 stop:用于 app exit / 异常路径,不报错只尽力关。
    /// 同样走 gate set true + send signal + abort 三保险确保 in-flight
    /// keep-alive connection 也被 reject + 端口释放(详见 [`Self::stop`])。
    pub fn stop_silent(&self) {
        let mut guard = self.handle.lock().unwrap();
        if let Some(h) = guard.take() {
            h.gate.store(true, Ordering::Release);
            let _ = h.shutdown_tx.send(());
            h.task.abort();
        }
    }

    pub fn status(&self) -> ProxyStatus {
        let guard = self.handle.lock().unwrap();
        match guard.as_ref() {
            Some(h) => ProxyStatus {
                running: true,
                addr: Some(h.addr.to_string()),
                gateway_auth: h.gateway_auth,
                provider_count: h.provider_count,
                active_provider: h.active_provider.clone(),
            },
            None => ProxyStatus {
                running: false,
                addr: None,
                gateway_auth: false,
                provider_count: 0,
                active_provider: None,
            },
        }
    }
}

struct ResolverSnapshot {
    resolver: StaticResolver,
    gateway_auth: bool,
    provider_count: usize,
    active_provider: Option<String>,
}

fn load_resolver_snapshot() -> Result<ResolverSnapshot, String> {
    let path = config_file().ok_or_else(|| "cannot locate config directory".to_owned())?;
    if !path.exists() {
        return Err(
            "config file ~/.codex-app-transfer/config.json does not exist; add a provider on the Providers page first".to_owned(),
        );
    }
    let s = std::fs::read_to_string(&path).map_err(|e| format!("read config.json failed: {e}"))?;
    // 先 raw Value 解析 + healing(强制覆盖 builtin provider 的 apiFormat /
    // authScheme / extraHeaders),再转 typed Config。proxy 这条路径**不写回
    // 磁盘**(避免与 admin 路径写盘竞争),仅在内存中保证当前 resolver 拿到
    // 修过的配置;真正的盘写入由 admin/registry_io.rs::load 在用户打开应用
    // 时触发。详见 registry::healing 模块说明。
    let mut raw: serde_json::Value =
        serde_json::from_str(&s).map_err(|e| format!("parse config.json failed: {e}"))?;
    codex_app_transfer_registry::heal_builtin_provider_fields(&mut raw);
    let cfg: Config =
        serde_json::from_value(raw).map_err(|e| format!("config.json schema mismatch: {e}"))?;
    if cfg.providers.is_empty() {
        return Err("no providers configured; add one first".to_owned());
    }
    let gateway_key = cfg.gateway_api_key.filter(|s| !s.is_empty());
    let gateway_auth = gateway_key.is_some();
    Ok(ResolverSnapshot {
        provider_count: cfg.providers.len(),
        active_provider: cfg.active_provider.clone(),
        resolver: StaticResolver::new(gateway_key, cfg.providers, cfg.active_provider),
        gateway_auth,
    })
}
