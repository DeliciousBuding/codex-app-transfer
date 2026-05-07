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

/// 表单编辑器(W4)。 Providers/Add 页用,字段对齐 schema::Provider 的可编辑部分。
#[derive(Debug, Clone, Default)]
pub struct ProviderForm {
    /// 编辑现有 provider 时填 Some(id);新增时为 None
    pub editing_id: Option<String>,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub auth_scheme: String, // "bearer" / "x-api-key" / "none"
    pub api_format: String,  // "openai_chat" / "responses"
    /// 6 模型 slot 的映射(对齐 schema::ModelSlotKey 顺序)
    pub mappings: [(String, String); 6],
    /// 当前 base_url 选项菜单(从 preset 的 baseUrlOptions 来,可空)
    pub base_url_options: Vec<(String, String)>,
}

impl ProviderForm {
    pub fn empty() -> Self {
        Self {
            editing_id: None,
            name: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            auth_scheme: "bearer".into(),
            api_format: "openai_chat".into(),
            mappings: default_mapping_slots(),
            base_url_options: Vec::new(),
        }
    }
}

fn default_mapping_slots() -> [(String, String); 6] {
    [
        ("default".into(), String::new()),
        ("gpt_5_5".into(), String::new()),
        ("gpt_5_4".into(), String::new()),
        ("gpt_5_4_mini".into(), String::new()),
        ("gpt_5_3_codex".into(), String::new()),
        ("gpt_5_2".into(), String::new()),
    ]
}

pub struct AppState {
    pub settings: Settings,
    pub providers: Vec<ProviderItem>,
    pub active_provider_id: Option<String>,
    pub config_load_error: Option<String>,
    pub config_save_error: Option<String>,
    last_load: Instant,
    pub config_present: bool,

    // ── W4 新增:表单与 UI 临时状态 ──
    pub form: ProviderForm,
    pub api_key_visible: bool,
    /// Some(id) → 渲染 deleteModal 等用户确认
    pub confirm_delete_id: Option<String>,
    /// W4: builtin presets cache(启动时 clone 一份,数据量小)
    pub presets: Vec<Value>,
    /// 通过 page::providers 设置 → app.rs 检测后切到 Page::ProvidersAdd
    pub nav_to_providers_add: bool,
    /// 完成保存或取消后,通过 page::providers_add 设置 → 切回 Page::Providers
    pub nav_back_to_providers: bool,
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
            form: ProviderForm::empty(),
            api_key_visible: false,
            confirm_delete_id: None,
            presets: codex_app_transfer_registry::builtin_presets().to_vec(),
            nav_to_providers_add: false,
            nav_back_to_providers: false,
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

    // ── W4 编辑/新增/删除/重排 provider 操作 ──

    /// 加载现有 provider 数据填进 form,准备进入编辑界面。
    pub fn load_provider_into_form(&mut self, id: &str) {
        let v = match self.read_config() {
            Some(v) => v,
            None => return,
        };
        let arr = match v.get("providers").and_then(|x| x.as_array()) {
            Some(a) => a,
            None => return,
        };
        let p = match arr
            .iter()
            .find(|p| p.get("id").and_then(|i| i.as_str()) == Some(id))
        {
            Some(p) => p,
            None => return,
        };
        let mut f = ProviderForm::empty();
        f.editing_id = Some(id.to_owned());
        f.name = p
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_owned();
        f.base_url = p
            .get("baseUrl")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_owned();
        f.api_key = p
            .get("apiKey")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_owned();
        f.auth_scheme = p
            .get("authScheme")
            .and_then(|x| x.as_str())
            .unwrap_or("bearer")
            .to_owned();
        f.api_format = p
            .get("apiFormat")
            .and_then(|x| x.as_str())
            .unwrap_or("openai_chat")
            .to_owned();
        if let Some(models) = p.get("models").and_then(|x| x.as_object()) {
            for (slot, target) in f.mappings.iter_mut() {
                *target = models
                    .get(slot.as_str())
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_owned();
            }
        }
        self.form = f;
        self.api_key_visible = false;
    }

    /// 把当前 form 写回 config.json。新增 provider 时生成短 8 字 hex id。
    pub fn save_form(&mut self) {
        let mut v = match self.read_config() {
            Some(v) => v,
            None => return,
        };
        let providers = v
            .as_object_mut()
            .and_then(|m| m.get_mut("providers"))
            .and_then(|p| p.as_array_mut());
        let providers = match providers {
            Some(a) => a,
            None => {
                v["providers"] = json!([]);
                v["providers"].as_array_mut().unwrap()
            }
        };

        let id = self
            .form
            .editing_id
            .clone()
            .unwrap_or_else(generate_short_id);

        // 构造 provider object
        let mut models = serde_json::Map::new();
        for (slot, target) in &self.form.mappings {
            models.insert(slot.clone(), Value::String(target.clone()));
        }

        let mut obj = serde_json::Map::new();
        obj.insert("id".into(), Value::String(id.clone()));
        obj.insert("name".into(), Value::String(self.form.name.clone()));
        obj.insert("baseUrl".into(), Value::String(self.form.base_url.clone()));
        obj.insert("apiKey".into(), Value::String(self.form.api_key.clone()));
        obj.insert(
            "authScheme".into(),
            Value::String(self.form.auth_scheme.clone()),
        );
        obj.insert(
            "apiFormat".into(),
            Value::String(self.form.api_format.clone()),
        );
        obj.insert("models".into(), Value::Object(models));
        obj.insert("isBuiltin".into(), Value::Bool(false));

        // upsert
        if let Some(idx) = providers
            .iter()
            .position(|p| p.get("id").and_then(|i| i.as_str()) == Some(&id))
        {
            // 编辑:保留原顺序 + 原 extraHeaders/modelCapabilities/requestOptions 等(merge)
            let merged = if let Value::Object(orig) = providers[idx].clone() {
                let mut merged = orig.clone();
                for (k, val) in &obj {
                    merged.insert(k.clone(), val.clone());
                }
                Value::Object(merged)
            } else {
                Value::Object(obj)
            };
            providers[idx] = merged;
        } else {
            providers.push(Value::Object(obj));
        }

        match save_raw_config(&self.config_path_or_default(), &v) {
            Ok(_) => {
                self.config_save_error = None;
                self.reload_now();
            }
            Err(e) => self.config_save_error = Some(format!("save provider 失败: {e}")),
        }
    }

    pub fn delete_provider(&mut self, id: &str) {
        let mut v = match self.read_config() {
            Some(v) => v,
            None => return,
        };
        if let Some(arr) = v
            .as_object_mut()
            .and_then(|m| m.get_mut("providers"))
            .and_then(|p| p.as_array_mut())
        {
            arr.retain(|p| p.get("id").and_then(|i| i.as_str()) != Some(id));
        }
        // 如果删除的是默认 provider,清空 activeProvider
        if v.get("activeProvider").and_then(|x| x.as_str()) == Some(id) {
            v["activeProvider"] = Value::Null;
        }
        match save_raw_config(&self.config_path_or_default(), &v) {
            Ok(_) => {
                self.config_save_error = None;
                self.reload_now();
            }
            Err(e) => self.config_save_error = Some(format!("delete 失败: {e}")),
        }
    }

    pub fn set_default_provider(&mut self, id: &str) {
        let mut v = match self.read_config() {
            Some(v) => v,
            None => return,
        };
        v["activeProvider"] = Value::String(id.to_owned());
        match save_raw_config(&self.config_path_or_default(), &v) {
            Ok(_) => {
                self.config_save_error = None;
                self.reload_now();
            }
            Err(e) => self.config_save_error = Some(format!("set default 失败: {e}")),
        }
    }

    /// 上下移动 provider 一格(W4 简化版,W6 可换 drag-drop)。
    pub fn move_provider(&mut self, id: &str, delta: i32) {
        let mut v = match self.read_config() {
            Some(v) => v,
            None => return,
        };
        if let Some(arr) = v
            .as_object_mut()
            .and_then(|m| m.get_mut("providers"))
            .and_then(|p| p.as_array_mut())
        {
            if let Some(idx) = arr
                .iter()
                .position(|p| p.get("id").and_then(|i| i.as_str()) == Some(id))
            {
                let new_idx =
                    ((idx as i32) + delta).clamp(0, (arr.len() as i32).saturating_sub(1)) as usize;
                if new_idx != idx {
                    let item = arr.remove(idx);
                    arr.insert(new_idx, item);
                }
            }
        }
        match save_raw_config(&self.config_path_or_default(), &v) {
            Ok(_) => {
                self.config_save_error = None;
                self.reload_now();
            }
            Err(e) => self.config_save_error = Some(format!("reorder 失败: {e}")),
        }
    }

    /// 用 preset 填充当前 form(用户在 providers/add 页右侧选 preset 后)。
    pub fn fill_form_from_preset(&mut self, preset: &Value) {
        let f = &mut self.form;
        f.editing_id = None;
        f.name = preset
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_owned();
        f.base_url = preset
            .get("baseUrl")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_owned();
        f.auth_scheme = preset
            .get("authScheme")
            .and_then(|x| x.as_str())
            .unwrap_or("bearer")
            .to_owned();
        f.api_format = preset
            .get("apiFormat")
            .and_then(|x| x.as_str())
            .unwrap_or("openai_chat")
            .to_owned();
        if let Some(models) = preset.get("models").and_then(|x| x.as_object()) {
            for (slot, target) in f.mappings.iter_mut() {
                *target = models
                    .get(slot.as_str())
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_owned();
            }
        }
        // baseUrlOptions(部分 preset 多区域 URL)
        f.base_url_options = preset
            .get("baseUrlOptions")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|opt| {
                        let label = opt.get("label").and_then(|x| x.as_str())?.to_owned();
                        let value = opt.get("value").and_then(|x| x.as_str())?.to_owned();
                        Some((label, value))
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    // ── helpers ──

    fn read_config(&mut self) -> Option<Value> {
        let path = self.config_path_or_default();
        if !path.exists() {
            return Some(json!({
                "version": "1.0.4",
                "activeProvider": null,
                "providers": [],
                "settings": {}
            }));
        }
        match load_raw_config(&path) {
            Ok(v) => Some(v),
            Err(e) => {
                self.config_save_error = Some(format!("读 config 失败: {e}"));
                None
            }
        }
    }

    fn config_path_or_default(&self) -> std::path::PathBuf {
        config_file().unwrap_or_else(|| std::path::PathBuf::from("config.json"))
    }
}

fn generate_short_id() -> String {
    use std::time::SystemTime;
    let ns = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:08x}", (ns as u32))
}
