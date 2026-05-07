//! 后台 async 服务 — W6 接通所有 async action。
//!
//! 设计:UI thread 通过 [`Bg::dispatch`] 把 [`UiAction`] 投到 mpsc channel,
//! 后台 tokio runtime 拉起 task 执行 → 完成后把 [`BgEvent`] 发回主 channel。
//! App.update() 每帧 try_recv 处理 BgEvent → 更新 state → request_repaint。
//!
//! 优点:UI 线程永不 block;async 任务用 spawn 不互相阻塞;状态更新有序。
//! 简化:暂时单 channel,所有任务复用一个 BgEvent enum;若任务数多可拆。

use std::sync::Arc;

use codex_app_transfer_proxy_runner::ProxyManager;
use serde_json::Value;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use crate::state::AppState;

/// UI 触发的意图。
#[derive(Debug, Clone)]
pub enum UiAction {
    StartProxy,
    StopProxy,
    ApplyDesktop,
    ClearDesktop,
    RestartCodex,
    TestProvider {
        base_url: String,
        api_key: String,
        model: String,
    },
    FetchModels {
        base_url: String,
        api_key: String,
    },
    CheckUpdate,
    InstallUpdate {
        url: String,
    },
    BackupConfig,
    ExportConfig,
    ImportConfig,
    SubmitFeedback {
        title: String,
        body: String,
        include_diagnostics: bool,
    },
    OpenLogDir,
    CopyToClipboard(String),
}

/// 后台任务回传给 UI 的事件。
#[derive(Debug, Clone)]
pub enum BgEvent {
    Toast {
        kind: ToastKind,
        message: String,
    },
    /// proxy 启动结果(更新 UI 上 running / port 显示)
    ProxyStarted {
        port: u16,
    },
    ProxyStopped,
    /// fetch /v1/models 返回的模型 id 列表
    AvailableModels(Vec<String>),
    /// check-update 返回的最新版本(若有)
    UpdateAvailable {
        version: String,
        url: Option<String>,
    },
    /// 备份成功后的文件路径
    BackupCreated {
        path: String,
    },
    /// 导入成功提示
    ImportSucceeded,
    /// 反馈提交成功的 server-side ID
    FeedbackSucceeded {
        id: String,
    },
    /// 任意触发 reload 的事件
    NeedsReload,
}

#[derive(Debug, Clone, Copy)]
pub enum ToastKind {
    Info,
    Success,
    Warn,
    Error,
}

pub struct Bg {
    runtime: Arc<Runtime>,
    pub proxy_manager: Arc<ProxyManager>,
    bg_tx: mpsc::UnboundedSender<BgEvent>,
    pub bg_rx: mpsc::UnboundedReceiver<BgEvent>,
    /// egui 上下文用于跨线程 request_repaint(没事件时退到 idle 不浪费 CPU)
    egui_ctx: Option<egui::Context>,
}

impl Bg {
    pub fn new() -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .thread_name("cas-bg")
                .build()
                .expect("build tokio runtime"),
        );
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            runtime,
            proxy_manager: Arc::new(ProxyManager::new()),
            bg_tx: tx,
            bg_rx: rx,
            egui_ctx: None,
        }
    }

    pub fn set_egui_ctx(&mut self, ctx: egui::Context) {
        self.egui_ctx = Some(ctx);
    }

    /// UI 线程调用;非阻塞;执行结果以 BgEvent 形式回到 bg_rx。
    pub fn dispatch(&self, action: UiAction) {
        let tx = self.bg_tx.clone();
        let proxy_manager = self.proxy_manager.clone();
        let ctx = self.egui_ctx.clone();
        self.runtime.spawn(async move {
            run_action(action, proxy_manager, &tx).await;
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }
}

async fn run_action(
    action: UiAction,
    proxy_manager: Arc<ProxyManager>,
    tx: &mpsc::UnboundedSender<BgEvent>,
) {
    match action {
        UiAction::StartProxy => {
            // 默认从配置读端口(load 与 UI 用同一个 ~/.codex-app-transfer/config.json)
            let port = read_proxy_port().unwrap_or(18080);
            match proxy_manager.start(port).await {
                Ok(_) => {
                    let _ = tx.send(BgEvent::ProxyStarted { port });
                    let _ = tx.send(BgEvent::Toast {
                        kind: ToastKind::Success,
                        message: format!("✓ proxy started on 127.0.0.1:{port}"),
                    });
                }
                Err(e) => {
                    let _ = tx.send(BgEvent::Toast {
                        kind: ToastKind::Error,
                        message: format!("启动 proxy 失败: {e}"),
                    });
                }
            }
        }
        UiAction::StopProxy => match proxy_manager.stop() {
            Ok(_) => {
                let _ = tx.send(BgEvent::ProxyStopped);
                let _ = tx.send(BgEvent::Toast {
                    kind: ToastKind::Info,
                    message: "proxy stopped".into(),
                });
            }
            Err(e) => {
                let _ = tx.send(BgEvent::Toast {
                    kind: ToastKind::Error,
                    message: format!("停止 proxy 失败: {e}"),
                });
            }
        },
        UiAction::ApplyDesktop => {
            // 真实 apply 需要从 config.json 读 active provider + gateway key,
            // 这里用同步 IO 装载后调用 codex_integration::apply_provider。
            match apply_active_provider() {
                Ok(_) => {
                    let _ = tx.send(BgEvent::Toast {
                        kind: ToastKind::Success,
                        message: "✓ Codex CLI 配置已应用".into(),
                    });
                    let _ = tx.send(BgEvent::NeedsReload);
                }
                Err(e) => {
                    let _ = tx.send(BgEvent::Toast {
                        kind: ToastKind::Error,
                        message: format!("apply 失败: {e}"),
                    });
                }
            }
        }
        UiAction::ClearDesktop => {
            let paths = codex_app_transfer_codex_integration::CodexPaths::from_home_dir(
                home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")),
            );
            match codex_app_transfer_codex_integration::restore_codex_state(&paths) {
                Ok(restored) => {
                    let _ = tx.send(BgEvent::Toast {
                        kind: ToastKind::Success,
                        message: if restored {
                            "✓ Codex 配置已还原"
                        } else {
                            "Codex 配置无快照可还原"
                        }
                        .into(),
                    });
                    let _ = tx.send(BgEvent::NeedsReload);
                }
                Err(e) => {
                    let _ = tx.send(BgEvent::Toast {
                        kind: ToastKind::Error,
                        message: format!("还原失败: {e}"),
                    });
                }
            }
        }
        UiAction::RestartCodex => {
            let result = if cfg!(target_os = "macos") {
                std::process::Command::new("sh")
                    .arg("-c")
                    .arg("pkill -x Codex 2>/dev/null; sleep 0.4; open -na Codex")
                    .status()
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            } else {
                Err("非 macOS 暂未实现 restart Codex".to_owned())
            };
            match result {
                Ok(_) => {
                    let _ = tx.send(BgEvent::Toast {
                        kind: ToastKind::Success,
                        message: "✓ Codex App 已重启".into(),
                    });
                }
                Err(e) => {
                    let _ = tx.send(BgEvent::Toast {
                        kind: ToastKind::Warn,
                        message: format!("restart Codex: {e}"),
                    });
                }
            }
        }
        UiAction::TestProvider {
            base_url,
            api_key,
            model,
        } => match test_provider_http(&base_url, &api_key, &model).await {
            Ok(took_ms) => {
                let _ = tx.send(BgEvent::Toast {
                    kind: ToastKind::Success,
                    message: format!("✓ test {model} OK · {took_ms}ms"),
                });
            }
            Err(e) => {
                let _ = tx.send(BgEvent::Toast {
                    kind: ToastKind::Error,
                    message: format!("test 失败: {e}"),
                });
            }
        },
        UiAction::FetchModels { base_url, api_key } => {
            match fetch_models_http(&base_url, &api_key).await {
                Ok(models) => {
                    let _ = tx.send(BgEvent::AvailableModels(models));
                }
                Err(e) => {
                    let _ = tx.send(BgEvent::Toast {
                        kind: ToastKind::Error,
                        message: format!("fetch models: {e}"),
                    });
                }
            }
        }
        UiAction::CheckUpdate => {
            let url = read_update_url().unwrap_or_else(|| {
                "https://github.com/Cmochance/codex-app-transfer/releases/latest/download/latest.json".to_owned()
            });
            match fetch_latest_json(&url).await {
                Ok((version, download_url)) => {
                    let cur = env!("CARGO_PKG_VERSION");
                    if version != cur {
                        let _ = tx.send(BgEvent::UpdateAvailable {
                            version,
                            url: download_url,
                        });
                    } else {
                        let _ = tx.send(BgEvent::Toast {
                            kind: ToastKind::Info,
                            message: format!("已是最新 {cur}"),
                        });
                    }
                }
                Err(e) => {
                    let _ = tx.send(BgEvent::Toast {
                        kind: ToastKind::Warn,
                        message: format!("check update: {e}"),
                    });
                }
            }
        }
        UiAction::InstallUpdate { url } => {
            // W7 切到 self_update 真实下载替换 .app;W6 临时:打开浏览器到下载 URL
            let _ = opener::open(&url);
            let _ = tx.send(BgEvent::Toast {
                kind: ToastKind::Info,
                message: "已在浏览器打开下载页(W7 接 self_update 自动替换)".into(),
            });
        }
        UiAction::BackupConfig => match backup_config_now() {
            Ok(path) => {
                let _ = tx.send(BgEvent::BackupCreated { path: path.clone() });
                let _ = tx.send(BgEvent::Toast {
                    kind: ToastKind::Success,
                    message: format!("✓ 备份: {path}"),
                });
            }
            Err(e) => {
                let _ = tx.send(BgEvent::Toast {
                    kind: ToastKind::Error,
                    message: format!("备份失败: {e}"),
                });
            }
        },
        UiAction::ExportConfig => {
            let task = rfd::AsyncFileDialog::new()
                .set_file_name("cas-export.json")
                .add_filter("JSON", &["json"])
                .save_file();
            let chosen = task.await;
            if let Some(handle) = chosen {
                match export_config_to(handle.path()) {
                    Ok(_) => {
                        let _ = tx.send(BgEvent::Toast {
                            kind: ToastKind::Success,
                            message: "✓ 导出完成".into(),
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(BgEvent::Toast {
                            kind: ToastKind::Error,
                            message: format!("导出失败: {e}"),
                        });
                    }
                }
            }
        }
        UiAction::ImportConfig => {
            let task = rfd::AsyncFileDialog::new()
                .add_filter("JSON", &["json"])
                .pick_file();
            let chosen = task.await;
            if let Some(handle) = chosen {
                match import_config_from(handle.path()) {
                    Ok(_) => {
                        let _ = tx.send(BgEvent::ImportSucceeded);
                        let _ = tx.send(BgEvent::Toast {
                            kind: ToastKind::Success,
                            message: "✓ 导入完成".into(),
                        });
                        let _ = tx.send(BgEvent::NeedsReload);
                    }
                    Err(e) => {
                        let _ = tx.send(BgEvent::Toast {
                            kind: ToastKind::Error,
                            message: format!("导入失败: {e}"),
                        });
                    }
                }
            }
        }
        UiAction::SubmitFeedback {
            title,
            body,
            include_diagnostics,
        } => match submit_feedback_http(&title, &body, include_diagnostics).await {
            Ok(id) => {
                let _ = tx.send(BgEvent::FeedbackSucceeded { id });
            }
            Err(e) => {
                let _ = tx.send(BgEvent::Toast {
                    kind: ToastKind::Error,
                    message: format!("反馈提交失败: {e}"),
                });
            }
        },
        UiAction::OpenLogDir => {
            if let Some(dir) = codex_app_transfer_proxy::proxy_log_dir() {
                let _ = opener::open(&dir);
            } else {
                let _ = tx.send(BgEvent::Toast {
                    kind: ToastKind::Warn,
                    message: "log dir 未定位".into(),
                });
            }
        }
        UiAction::CopyToClipboard(text) => match arboard::Clipboard::new() {
            Ok(mut cb) => {
                let _ = cb.set_text(text);
                let _ = tx.send(BgEvent::Toast {
                    kind: ToastKind::Success,
                    message: "✓ 已复制到剪贴板".into(),
                });
            }
            Err(e) => {
                let _ = tx.send(BgEvent::Toast {
                    kind: ToastKind::Error,
                    message: format!("剪贴板: {e}"),
                });
            }
        },
    }
}

// ── helpers(同步 + 工具)──────────────────────────────────

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

fn read_config_value() -> Result<Value, String> {
    let path = codex_app_transfer_registry::config_file()
        .ok_or_else(|| "config_file 路径定位失败".to_owned())?;
    if !path.exists() {
        return Err("config.json 不存在".into());
    }
    codex_app_transfer_registry::load_raw_config(&path).map_err(|e| format!("load: {e}"))
}

fn read_proxy_port() -> Option<u16> {
    let v = read_config_value().ok()?;
    v.get("settings")?
        .get("proxyPort")?
        .as_u64()
        .map(|n| n as u16)
}

fn read_update_url() -> Option<String> {
    let v = read_config_value().ok()?;
    v.get("settings")?
        .get("updateUrl")?
        .as_str()
        .map(|s| s.to_owned())
}

fn apply_active_provider() -> Result<(), String> {
    use codex_app_transfer_codex_integration::{apply_provider, ApplyConfig, CodexPaths};
    let v = read_config_value()?;
    let active_id = v
        .get("activeProvider")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "未设置 active provider".to_owned())?;
    let providers = v.get("providers").and_then(|x| x.as_array()).cloned();
    let prov = providers
        .and_then(|arr| {
            arr.into_iter()
                .find(|p| p.get("id").and_then(|i| i.as_str()) == Some(active_id))
        })
        .ok_or_else(|| format!("active provider {active_id} 不存在"))?;

    let provider_name = prov
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("Provider");
    let default_model = prov
        .get("models")
        .and_then(|m| m.get("default"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let base_url = prov
        .get("baseUrl")
        .and_then(|x| x.as_str())
        .unwrap_or("http://127.0.0.1:18080/v1");
    let gateway_api_key = v
        .get("gatewayApiKey")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let supports_1m = should_set_1m(default_model);

    let paths =
        CodexPaths::from_home_dir(home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")));
    let cfg = ApplyConfig {
        provider_name,
        default_model,
        base_url: "http://127.0.0.1:18080/v1",
        gateway_api_key,
        supports_1m,
        model_mappings: prov.get("models"),
        model_capabilities: prov.get("modelCapabilities"),
        app_version: env!("CARGO_PKG_VERSION"),
    };
    apply_provider(&paths, &cfg).map_err(|e| format!("apply_provider: {e}"))?;
    let _ = base_url;
    Ok(())
}

fn should_set_1m(model: &str) -> bool {
    let lc = model.to_ascii_lowercase();
    lc.starts_with("deepseek-v4-") || lc.starts_with("qwen3.6-") || lc.contains("[1m]")
}

fn backup_config_now() -> Result<String, String> {
    let src =
        codex_app_transfer_registry::config_file().ok_or_else(|| "config_file 失败".to_owned())?;
    if !src.exists() {
        return Err("config.json 不存在".into());
    }
    // backups_dir 暂时手工拼:~/.codex-app-transfer/backups/
    let cfg_dir =
        codex_app_transfer_registry::config_dir().ok_or_else(|| "config_dir 失败".to_owned())?;
    let dir = cfg_dir.join("backups");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let dst = dir.join(format!("config-{stamp}.json"));
    std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
    Ok(dst.display().to_string())
}

fn export_config_to(dst: &std::path::Path) -> Result<(), String> {
    let src =
        codex_app_transfer_registry::config_file().ok_or_else(|| "config_file 失败".to_owned())?;
    if !src.exists() {
        return Err("config.json 不存在".into());
    }
    std::fs::copy(&src, dst).map_err(|e| e.to_string())?;
    Ok(())
}

fn import_config_from(src: &std::path::Path) -> Result<(), String> {
    let bytes = std::fs::read(src).map_err(|e| format!("read: {e}"))?;
    let _: Value = serde_json::from_slice(&bytes).map_err(|e| format!("not valid JSON: {e}"))?;
    let dst =
        codex_app_transfer_registry::config_file().ok_or_else(|| "config_file 失败".to_owned())?;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&dst, bytes).map_err(|e| e.to_string())?;
    Ok(())
}

async fn test_provider_http(base_url: &str, api_key: &str, model: &str) -> Result<u128, String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 1,
        "stream": false,
    });
    let started = std::time::Instant::now();
    let resp = reqwest::Client::new()
        .post(&url)
        .bearer_auth(api_key)
        .timeout(std::time::Duration::from_secs(10))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let took = started.elapsed().as_millis();
    if resp.status().is_success() {
        Ok(took)
    } else {
        let status = resp.status();
        let txt = resp.text().await.unwrap_or_default();
        Err(format!("HTTP {status} · {}", &txt[..txt.len().min(120)]))
    }
}

async fn fetch_models_http(base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth(api_key)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let models = json
        .get("data")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(models)
}

async fn fetch_latest_json(url: &str) -> Result<(String, Option<String>), String> {
    let resp = reqwest::Client::new()
        .get(url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let version = json
        .get("version")
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| "latest.json 缺 version 字段".to_owned())?;
    let download_url = json
        .get("platforms")
        .and_then(|p| {
            if cfg!(target_os = "macos") {
                p.get("darwin-aarch64").or_else(|| p.get("darwin-x86_64"))
            } else if cfg!(target_os = "windows") {
                p.get("windows-x86_64")
            } else {
                p.get("linux-x86_64")
            }
        })
        .and_then(|x| x.get("url"))
        .and_then(|x| x.as_str())
        .map(String::from);
    Ok((version, download_url))
}

async fn submit_feedback_http(
    title: &str,
    body: &str,
    include_diagnostics: bool,
) -> Result<String, String> {
    const ENDPOINT: &str = "https://codex-app-transfer-feedback.alysechencn.workers.dev";
    let mut form = reqwest::multipart::Form::new()
        .text("title", title.to_owned())
        .text("body", body.to_owned());
    if include_diagnostics {
        let diag = format!(
            "version={}\nos={}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
        );
        form = form.text("diagnostics", diag);
    }
    let resp = reqwest::Client::new()
        .post(ENDPOINT)
        .timeout(std::time::Duration::from_secs(20))
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let id = json
        .get("id")
        .and_then(|x| x.as_str())
        .map(String::from)
        .unwrap_or_else(|| "OK".to_owned());
    Ok(id)
}

/// UI 帧循环里调:把 BgEvent 应用到 AppState,Toast 类事件 caller 自己处理。
pub fn drain_into(state: &mut AppState, bg: &mut Bg) -> Vec<BgEvent> {
    let mut out = Vec::new();
    while let Ok(ev) = bg.bg_rx.try_recv() {
        match &ev {
            BgEvent::NeedsReload => state.reload_now(),
            BgEvent::ProxyStarted { .. } => state.proxy_running = true,
            BgEvent::ProxyStopped => state.proxy_running = false,
            BgEvent::AvailableModels(models) => state.available_models = models.clone(),
            BgEvent::UpdateAvailable { version, url } => {
                state.update_available = Some((version.clone(), url.clone()));
            }
            BgEvent::FeedbackSucceeded { .. } => {
                state.show_feedback_modal = false;
                state.feedback_title.clear();
                state.feedback_body.clear();
            }
            _ => {}
        }
        out.push(ev);
    }
    out
}
