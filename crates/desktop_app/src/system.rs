//! W6.2 系统集成:tray-icon + muda(macOS native menu)+ single-instance + cas:// + auto-launch.
//!
//! 设计:
//! - `init_macos_app_menu()` 在 [`App::new`] 中调用一次,muda 安装到 NSApplication
//! - [`Tray`] 在 App 内持有,菜单按需 [`Tray::rebuild_menu`] 重建(provider 列表变化时)
//! - tray 事件 + menu 事件用 muda/tray-icon 的全局 crossbeam channel,
//!   主 update() 每帧 [`Tray::poll_events`] 拉取
//! - cas:// URL 在 [`parse_cas_url`] 解析,目前只识别 `cas://providers/add?baseUrl=...&apiKey=...`
//! - single-instance 在 [`acquire_single_instance`] 持锁;次实例直接 exit(W6.2 暂不
//!   做 IPC URL 转发,W6-A 决策点后视情况添加)
//! - auto-launch 在 [`set_auto_launch`] 包装,失败返回 `Result<(),String>` 给 toast

use std::sync::OnceLock;

#[cfg(target_os = "macos")]
use muda::{AboutMetadata, Menu, PredefinedMenuItem, Submenu};
use single_instance::SingleInstance;
use tray_icon::{
    menu::{Menu as TrayMenu, MenuEvent, MenuId, MenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

/// 解析 cas:// URL 的结果。命中支持的路径才返回 Some。
#[derive(Debug, Clone)]
pub enum CasAction {
    /// `cas://providers/add?baseUrl=...&apiKey=...&name=...`
    AddProvider {
        name: Option<String>,
        base_url: String,
        api_key: Option<String>,
    },
    /// `cas://desktop/apply?provider=<id>`
    ApplyProvider {
        provider_id: String,
    },
    /// `cas://proxy/start` / `cas://proxy/stop`
    ProxyStart,
    ProxyStop,
}

pub fn parse_cas_url(input: &str) -> Option<CasAction> {
    let rest = input.strip_prefix("cas://")?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let path = path.trim_end_matches('/').trim_start_matches('/');

    let mut params: Vec<(String, String)> = Vec::new();
    for kv in query.split('&').filter(|s| !s.is_empty()) {
        if let Some((k, v)) = kv.split_once('=') {
            let k = urlencoding::decode(k).map(|c| c.into_owned()).ok()?;
            let v = urlencoding::decode(v).map(|c| c.into_owned()).ok()?;
            params.push((k, v));
        }
    }
    let get = |key: &str| -> Option<String> {
        params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };

    match path {
        "providers/add" => {
            let base_url = get("baseUrl").or_else(|| get("base_url"))?;
            Some(CasAction::AddProvider {
                name: get("name"),
                base_url,
                api_key: get("apiKey").or_else(|| get("api_key")),
            })
        }
        "desktop/apply" => Some(CasAction::ApplyProvider {
            provider_id: get("provider")?,
        }),
        "proxy/start" => Some(CasAction::ProxyStart),
        "proxy/stop" => Some(CasAction::ProxyStop),
        _ => None,
    }
}

/// 启动时取 argv 找 cas:// URL(macOS GUI 通过 LSItemContentTypes/CFBundleURLTypes
/// 拉起时第一个非 binary 参数即 URL,Windows 通过 registry 注册的 OpenCommand
/// `"...exe" "%1"` 也会落到 argv[1])。
pub fn cas_url_from_argv() -> Option<String> {
    std::env::args().skip(1).find(|a| a.starts_with("cas://"))
}

/// 持锁直到进程退出。返回 false 表示已有实例在跑(应当退出)。
///
/// `single-instance` crate 在 Unix 上会把 name 当作文件路径(fcntl flock 文件锁),
/// 必须传绝对路径,否则会在 CWD 留空文件。Windows 用 named mutex,name 用作 mutex
/// 名,不落盘。
pub fn acquire_single_instance() -> bool {
    let lock_path = single_instance_lock_path();
    let path_str = lock_path.to_str().unwrap_or("codex-app-transfer-v3");
    let instance = match SingleInstance::new(path_str) {
        Ok(v) => v,
        Err(_) => return true,
    };
    if !instance.is_single() {
        return false;
    }
    static GUARD: OnceLock<SingleInstance> = OnceLock::new();
    let _ = GUARD.set(instance);
    true
}

#[cfg(target_family = "unix")]
fn single_instance_lock_path() -> std::path::PathBuf {
    // ~/.codex-app-transfer/.singleton.lock(目录已存在,W3 起读写 config.json 用同一处)
    if let Some(home) = std::env::var_os("HOME") {
        let mut p = std::path::PathBuf::from(home);
        p.push(".codex-app-transfer");
        let _ = std::fs::create_dir_all(&p);
        p.push(".singleton.lock");
        p
    } else {
        std::path::PathBuf::from("/tmp/codex-app-transfer-v3.lock")
    }
}

#[cfg(not(target_family = "unix"))]
fn single_instance_lock_path() -> std::path::PathBuf {
    // Windows 走 named mutex,name 仅作为标识不落盘
    std::path::PathBuf::from("codex-app-transfer-v3")
}

/// macOS 安装 native app menu 到 NSApp。在 eframe 启动后调用(NSApplication 已就绪)。
#[cfg(target_os = "macos")]
pub fn init_macos_app_menu() {
    let menu = Menu::new();

    let app_submenu = Submenu::new("Codex App Transfer", true);
    let about = PredefinedMenuItem::about(
        Some("About Codex App Transfer"),
        Some(AboutMetadata {
            name: Some("Codex App Transfer".into()),
            version: Some(env!("CARGO_PKG_VERSION").into()),
            ..Default::default()
        }),
    );
    let _ = app_submenu.append_items(&[
        &about,
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::services(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::hide_others(None),
        &PredefinedMenuItem::show_all(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(None),
    ]);

    let edit_submenu = Submenu::new("Edit", true);
    let _ = edit_submenu.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::select_all(None),
    ]);

    let window_submenu = Submenu::new("Window", true);
    let _ = window_submenu.append_items(&[
        &PredefinedMenuItem::minimize(None),
        &PredefinedMenuItem::maximize(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::close_window(None),
    ]);

    let _ = menu.append_items(&[&app_submenu, &edit_submenu, &window_submenu]);
    menu.init_for_nsapp();
}

#[cfg(not(target_os = "macos"))]
pub fn init_macos_app_menu() {}

/// 系统托盘 + 动态 provider 菜单。
///
/// 菜单结构:
/// - 显示 / 隐藏窗口
/// - ─────────
/// - [active marker] Provider A
/// - [active marker] Provider B
/// - ...
/// - ─────────
/// - 启动 / 停止 proxy
/// - 退出
pub struct Tray {
    icon: Option<TrayIcon>,
    show_id: MenuId,
    quit_id: MenuId,
    proxy_toggle_id: MenuId,
    /// (menu_id, provider_id)
    provider_items: Vec<(MenuId, String)>,
}

impl Tray {
    pub fn new() -> Option<Self> {
        let menu = TrayMenu::new();
        let show_item = MenuItem::new("显示 / 隐藏窗口", true, None);
        let proxy_toggle = MenuItem::new("启动 / 停止 proxy", true, None);
        let quit_item = MenuItem::new("退出 Codex App Transfer", true, None);
        let _ = menu.append_items(&[
            &show_item,
            &tray_icon::menu::PredefinedMenuItem::separator(),
            &proxy_toggle,
            &tray_icon::menu::PredefinedMenuItem::separator(),
            &quit_item,
        ]);
        let icon = match build_tray_icon(menu) {
            Ok(t) => Some(t),
            Err(_) => None,
        };
        Some(Self {
            icon,
            show_id: show_item.id().clone(),
            quit_id: quit_item.id().clone(),
            proxy_toggle_id: proxy_toggle.id().clone(),
            provider_items: Vec::new(),
        })
    }

    /// 重建菜单(provider 列表 / active 状态变化时调用)。
    /// providers: (id, name, is_active)
    pub fn rebuild_menu(&mut self, providers: &[(String, String, bool)]) {
        let Some(icon) = self.icon.as_ref() else {
            return;
        };
        let menu = TrayMenu::new();
        let show_item = MenuItem::new("显示 / 隐藏窗口", true, None);
        self.show_id = show_item.id().clone();
        let _ = menu.append(&show_item);
        let _ = menu.append(&tray_icon::menu::PredefinedMenuItem::separator());

        self.provider_items.clear();
        for (id, name, active) in providers {
            let label = if *active {
                format!("● {}", name)
            } else {
                format!("  {}", name)
            };
            let item = MenuItem::new(label, true, None);
            self.provider_items.push((item.id().clone(), id.clone()));
            let _ = menu.append(&item);
        }
        if !providers.is_empty() {
            let _ = menu.append(&tray_icon::menu::PredefinedMenuItem::separator());
        }

        let proxy_toggle = MenuItem::new("启动 / 停止 proxy", true, None);
        self.proxy_toggle_id = proxy_toggle.id().clone();
        let _ = menu.append(&proxy_toggle);
        let _ = menu.append(&tray_icon::menu::PredefinedMenuItem::separator());

        let quit_item = MenuItem::new("退出 Codex App Transfer", true, None);
        self.quit_id = quit_item.id().clone();
        let _ = menu.append(&quit_item);

        let _ = icon.set_menu(Some(Box::new(menu)));
    }

    /// 主帧调用。返回收到的事件(已映射到语义),不阻塞。
    pub fn poll_events(&self) -> Vec<TrayUiEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == self.show_id {
                out.push(TrayUiEvent::ToggleWindow);
            } else if ev.id == self.quit_id {
                out.push(TrayUiEvent::Quit);
            } else if ev.id == self.proxy_toggle_id {
                out.push(TrayUiEvent::ToggleProxy);
            } else if let Some((_, pid)) = self.provider_items.iter().find(|(mid, _)| mid == &ev.id)
            {
                out.push(TrayUiEvent::SelectProvider(pid.clone()));
            }
        }
        // 我们暂不处理左键点击(预期点击 = 弹出菜单,系统已处理)
        while TrayIconEvent::receiver().try_recv().is_ok() {}
        out
    }
}

#[derive(Debug, Clone)]
pub enum TrayUiEvent {
    ToggleWindow,
    ToggleProxy,
    SelectProvider(String),
    Quit,
}

fn build_tray_icon(menu: TrayMenu) -> Result<TrayIcon, tray_icon::Error> {
    // 占位单色 icon(W7 换打包 PNG;此处避免解码 PNG 引依赖膨胀)
    let icon = make_placeholder_icon();
    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_tooltip("Codex App Transfer")
        .build()
}

fn make_placeholder_icon() -> tray_icon::Icon {
    // 16x16 蓝色实心,#1476ff
    let size: u32 = 16;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for _ in 0..(size * size) {
        rgba.extend_from_slice(&[0x14, 0x76, 0xff, 0xff]);
    }
    tray_icon::Icon::from_rgba(rgba, size, size).expect("placeholder icon")
}

/// 设置自动启动(从 settings.auto_start 改动触发)。
pub fn set_auto_launch(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe
        .to_str()
        .ok_or_else(|| "exe path 含非 UTF-8".to_owned())?;
    let auto = auto_launch::AutoLaunchBuilder::new()
        .set_app_name("Codex App Transfer")
        .set_app_path(exe_str)
        .set_use_launch_agent(true)
        .build()
        .map_err(|e| e.to_string())?;
    if enabled {
        auto.enable().map_err(|e| e.to_string())
    } else {
        auto.disable().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_providers_add_minimal() {
        let r = parse_cas_url("cas://providers/add?baseUrl=https%3A%2F%2Fapi.example.com%2Fv1")
            .unwrap();
        match r {
            CasAction::AddProvider {
                base_url,
                name,
                api_key,
            } => {
                assert_eq!(base_url, "https://api.example.com/v1");
                assert!(name.is_none());
                assert!(api_key.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_providers_add_full() {
        let r = parse_cas_url(
            "cas://providers/add?name=Demo&baseUrl=https%3A%2F%2Fa%2Fv1&apiKey=sk-abc",
        )
        .unwrap();
        match r {
            CasAction::AddProvider {
                name,
                base_url,
                api_key,
            } => {
                assert_eq!(name.as_deref(), Some("Demo"));
                assert_eq!(base_url, "https://a/v1");
                assert_eq!(api_key.as_deref(), Some("sk-abc"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_proxy_actions() {
        assert!(matches!(
            parse_cas_url("cas://proxy/start"),
            Some(CasAction::ProxyStart)
        ));
        assert!(matches!(
            parse_cas_url("cas://proxy/stop"),
            Some(CasAction::ProxyStop)
        ));
    }

    #[test]
    fn parse_apply() {
        let r = parse_cas_url("cas://desktop/apply?provider=abc-123").unwrap();
        match r {
            CasAction::ApplyProvider { provider_id } => assert_eq!(provider_id, "abc-123"),
            _ => panic!("wrong"),
        }
    }

    #[test]
    fn parse_unknown_returns_none() {
        assert!(parse_cas_url("cas://nothing").is_none());
        assert!(parse_cas_url("https://something").is_none());
        assert!(parse_cas_url("").is_none());
    }
}
