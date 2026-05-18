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
//! - **生命周期**:`start` 时 spawn tokio task 跑 `tokio::select! { axum::serve,
//!   shutdown_rx }`;`stop` / `stop_silent` 通过 `oneshot::Sender::send(())`
//!   触发 select 立刻退 → `axum::serve` future 整个 drop → 内部所有 connection
//!   sub-task + TcpListener **同步销毁** → 端口立即 free。
//! - **为什么不用 `with_graceful_shutdown`**:它等所有 in-flight connection
//!   自然 drain 完才释放 listener,在 keep-alive / SSE 长连接场景永远 drain
//!   不完 → 端口卡占用(2026-05-18 用户真机复现"点停止后还能转发"根因)。
//!   `select!` drop 强制销毁 future stack,不等 in-flight,代价是 in-flight
//!   request 被强断 connection reset,但"用户点停止 = 真要停"是合理 trade-off。

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use codex_app_transfer_proxy::{build_router_with_gate, StaticResolver};
use codex_app_transfer_registry::{config_file, Config};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

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
    /// 停止信号 — `stop` / `stop_silent` send 给跑着的 select! task,触发
    /// `axum::serve` future drop → listener + 所有 connection sub-task 同步
    /// 销毁 → 端口立即 free。
    shutdown_tx: oneshot::Sender<()>,
    /// axum::serve task 的 JoinHandle — 主路径靠 shutdown_tx + select! drop
    /// 释放端口,`task.abort()` 作为最后兜底(以防 select 异常)。
    ///
    /// 历史踩坑(2026-05-18):之前只靠 `with_graceful_shutdown` 在 keep-alive
    /// / SSE 长连接场景永远 drain 不完 → 端口卡占用 → 用户感知"停了但
    /// 仍能转发"。现在改成 `tokio::select! { axum::serve, shutdown_rx }`
    /// 强制 drop future 释放端口,不等 in-flight。
    task: JoinHandle<()>,
    /// Application-level shutdown gate(in-flight request 二保险):虽然 select!
    /// drop 已经销毁 axum::serve future,但**信号 send → task wake → future
    /// drop 之间有微小窗口**(纳秒级,但理论存在)。这个 gate 在 stop 时
    /// 先 set true,router 顶层 middleware 检查后立刻 503 + Connection: close
    /// 任何窗口内还在 process 的 in-flight request,确保用户看到的语义是
    /// "stop 之后再没有任何转发响应"。
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
        let (tx, rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            // 强制释放端口:用 select! 把 axum::serve future 跟 shutdown signal
            // 包一起,signal 来时 select 立刻退 → axum::serve future 整个 drop
            // → 内部所有 connection sub-task + TcpListener 同步销毁 → 端口
            // **立即** free。**不**用 `with_graceful_shutdown`,因为它等所有
            // in-flight connection 自然 drain,在 keep-alive / SSE 长连接场景
            // 永远 drain 不完 → 端口卡占用(2026-05-18 用户真机复现)。
            use std::future::IntoFuture;
            let serve = axum::serve(listener, router.into_make_service()).into_future();
            tokio::pin!(serve);
            tokio::select! {
                _ = &mut serve => {}  // 正常 server lifecycle 退出(listener error 等)
                _ = rx => {}           // shutdown signal:drop serve_future → listener drop
            }
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
