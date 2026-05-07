//! AppState — 把 ~/.codex-app-transfer/config.json 的内容映射成 Rust 结构,
//! 供页面渲染读、用户操作写。
//!
//! 设计原则:
//! - 启动时 load 一次;每帧 update 调 maybe_reload() 自动每 2 秒刷新一次磁盘
//! - 用户改设置 → 立即调 save() 写回(简单,先不做 debounce,W4 起观察是否需要)
//! - I/O 错误不阻塞 UI:load 失败用 default,save 失败 push 到 errors,UI 提示

use std::time::{Duration, Instant};

use codex_app_transfer_registry::{config_file, load_raw_config, save_raw_config};
use serde_json::{json, Value};

use crate::i18n::Locale;
use crate::theme::ThemeName;

/// 设置面板的 7 个原子字段 + updateUrl(对齐 ~/.codex-app-transfer/config.json
/// 里的 settings 子树)。读时把 i18n 文本 / theme name 解析成枚举;写时反序列化。
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub theme: ThemeName,
    pub language: Locale,
    pub proxy_port: u16,
    pub admin_port: u16,
    pub auto_start: bool,
    pub auto_apply_on_start: bool,
    pub expose_all_provider_models: bool,
    pub restore_codex_on_exit: bool,
    pub update_url: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeName::Default,
            language: Locale::Zh,
            proxy_port: 18080,
            admin_port: 18081,
            auto_start: false,
            auto_apply_on_start: true,
            expose_all_provider_models: false,
            restore_codex_on_exit: true,
            update_url:
                "https://github.com/Cmochance/codex-app-transfer/releases/latest/download/latest.json"
                    .to_owned(),
        }
    }
}

impl Settings {
    fn from_value(v: &Value) -> Self {
        let mut s = Self::default();
        if let Some(obj) = v.as_object() {
            if let Some(t) = obj.get("theme").and_then(|x| x.as_str()) {
                s.theme = match t {
                    "default" => ThemeName::Default,
                    "green" => ThemeName::Green,
                    "orange" => ThemeName::Orange,
                    "gray" => ThemeName::Gray,
                    "dark" => ThemeName::Dark,
                    "white" => ThemeName::White,
                    _ => ThemeName::Default,
                };
            }
            if let Some(l) = obj.get("language").and_then(|x| x.as_str()) {
                s.language = Locale::from_code(l);
            }
            if let Some(n) = obj.get("proxyPort").and_then(|x| x.as_u64()) {
                s.proxy_port = n.min(65535) as u16;
            }
            if let Some(n) = obj.get("adminPort").and_then(|x| x.as_u64()) {
                s.admin_port = n.min(65535) as u16;
            }
            if let Some(b) = obj.get("autoStart").and_then(|x| x.as_bool()) {
                s.auto_start = b;
            }
            if let Some(b) = obj.get("autoApplyOnStart").and_then(|x| x.as_bool()) {
                s.auto_apply_on_start = b;
            }
            if let Some(b) = obj.get("exposeAllProviderModels").and_then(|x| x.as_bool()) {
                s.expose_all_provider_models = b;
            }
            if let Some(b) = obj.get("restoreCodexOnExit").and_then(|x| x.as_bool()) {
                s.restore_codex_on_exit = b;
            }
            if let Some(u) = obj.get("updateUrl").and_then(|x| x.as_str()) {
                s.update_url = u.to_owned();
            }
        }
        s
    }

    fn theme_str(&self) -> &'static str {
        match self.theme {
            ThemeName::Default => "default",
            ThemeName::Green => "green",
            ThemeName::Orange => "orange",
            ThemeName::Gray => "gray",
            ThemeName::Dark => "dark",
            ThemeName::White => "white",
        }
    }

    fn write_to(&self, v: &mut Value) {
        let obj = v.as_object_mut().expect("settings 子树必须是 object");
        obj.insert("theme".into(), json!(self.theme_str()));
        obj.insert("language".into(), json!(self.language.code()));
        obj.insert("proxyPort".into(), json!(self.proxy_port));
        obj.insert("adminPort".into(), json!(self.admin_port));
        obj.insert("autoStart".into(), json!(self.auto_start));
        obj.insert("autoApplyOnStart".into(), json!(self.auto_apply_on_start));
        obj.insert(
            "exposeAllProviderModels".into(),
            json!(self.expose_all_provider_models),
        );
        obj.insert(
            "restoreCodexOnExit".into(),
            json!(self.restore_codex_on_exit),
        );
        obj.insert("updateUrl".into(), json!(self.update_url));
    }
}

/// Provider 列表项(只展示视图需要的字段,不复用 admin_api 的 Public 形态)。
#[derive(Debug, Clone)]
pub struct ProviderItem {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    pub is_default: bool,
    pub has_api_key: bool,
}

pub struct AppState {
    pub settings: Settings,
    pub providers: Vec<ProviderItem>,
    pub active_provider_id: Option<String>,
    pub config_load_error: Option<String>,
    pub config_save_error: Option<String>,
    last_load: Instant,
    pub config_present: bool,
}

impl AppState {
    pub fn load() -> Self {
        let mut s = Self {
            settings: Settings::default(),
            providers: Vec::new(),
            active_provider_id: None,
            config_load_error: None,
            config_save_error: None,
            last_load: Instant::now(),
            config_present: false,
        };
        s.reload_now();
        s
    }

    /// 周期性自动重载(2 秒一次)。
    pub fn maybe_reload(&mut self) {
        if self.last_load.elapsed() >= Duration::from_secs(2) {
            self.reload_now();
        }
    }

    pub fn reload_now(&mut self) {
        self.last_load = Instant::now();
        let path = match config_file() {
            Some(p) => p,
            None => {
                self.config_load_error = Some("无法定位 ~/.codex-app-transfer/config.json".into());
                return;
            }
        };
        if !path.exists() {
            self.config_present = false;
            self.config_load_error = None;
            return;
        }
        match load_raw_config(&path) {
            Ok(v) => {
                self.config_load_error = None;
                self.config_present = true;
                self.settings = v
                    .get("settings")
                    .map(Settings::from_value)
                    .unwrap_or_default();
                self.active_provider_id = v
                    .get("activeProvider")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_owned());
                self.providers = v
                    .get("providers")
                    .and_then(|x| x.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|p| {
                                let id = p.get("id").and_then(|x| x.as_str())?.to_owned();
                                let name = p
                                    .get("name")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("Unnamed")
                                    .to_owned();
                                let base_url = p
                                    .get("baseUrl")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("")
                                    .to_owned();
                                let default_model = p
                                    .get("models")
                                    .and_then(|m| m.get("default"))
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("")
                                    .to_owned();
                                let is_default = self.active_provider_id.as_deref() == Some(&id);
                                let has_api_key = p
                                    .get("apiKey")
                                    .and_then(|x| x.as_str())
                                    .map(|s| !s.is_empty())
                                    .unwrap_or(false);
                                Some(ProviderItem {
                                    id,
                                    name,
                                    base_url,
                                    default_model,
                                    is_default,
                                    has_api_key,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
            Err(e) => {
                self.config_load_error = Some(format!("读取 config.json 失败: {e}"));
            }
        }
    }

    /// 把当前 settings 写回磁盘,保留其它 top-level 字段(providers/version/etc 不动)。
    pub fn save_settings(&mut self) {
        let path = match config_file() {
            Some(p) => p,
            None => {
                self.config_save_error = Some("无法定位 ~/.codex-app-transfer/config.json".into());
                return;
            }
        };
        let mut v = if path.exists() {
            match load_raw_config(&path) {
                Ok(v) => v,
                Err(e) => {
                    self.config_save_error = Some(format!("save: 重载 config.json 失败: {e}"));
                    return;
                }
            }
        } else {
            json!({
                "version": "1.0.4",
                "activeProvider": null,
                "providers": [],
                "settings": {}
            })
        };

        if v.get("settings").is_none() {
            v["settings"] = json!({});
        }
        self.settings.write_to(&mut v["settings"]);

        match save_raw_config(&path, &v) {
            Ok(_) => {
                self.config_save_error = None;
                self.last_load = Instant::now();
            }
            Err(e) => {
                self.config_save_error = Some(format!("写 config.json 失败: {e}"));
            }
        }
    }
}
