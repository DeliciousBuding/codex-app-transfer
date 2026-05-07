//! Page 路由 + 各 page 实装入口。
//!
//! W3 起 page::render 接收 &mut AppState,各 page 直接读 state.settings /
//! state.providers 并修改 state.settings(然后 state.save_settings)。

use eframe::egui;

use crate::i18n::Locale;
use crate::state::AppState;

#[derive(Copy, Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Page {
    Dashboard,
    ProvidersAdd,
    Providers,
    Desktop,
    Proxy,
    Settings,
    Guide,
}

impl Default for Page {
    fn default() -> Self {
        Self::Dashboard
    }
}

impl Page {
    pub const ALL: &'static [Self] = &[
        Self::Dashboard,
        Self::Providers,
        Self::ProvidersAdd,
        Self::Desktop,
        Self::Proxy,
        Self::Settings,
        Self::Guide,
    ];

    pub fn nav_key(self) -> &'static str {
        match self {
            Self::Dashboard => "nav.dashboard",
            Self::Providers => "nav.providers",
            Self::ProvidersAdd => "providers.add",
            Self::Desktop => "nav.desktop",
            Self::Proxy => "nav.proxy",
            Self::Settings => "nav.settings",
            Self::Guide => "nav.guide",
        }
    }
}

pub mod dashboard;
pub mod desktop;
pub mod guide;
pub mod providers;
pub mod providers_add;
pub mod proxy;
pub mod settings;

/// 占位 render(W4-W5 各 page 替换):标题 + i18n + "TODO Wn"
pub fn placeholder(ui: &mut egui::Ui, locale: Locale, title_key: &str, todo_label: &str) {
    ui.add_space(8.0);
    ui.heading(crate::i18n::lookup_owned(locale, title_key));
    ui.add_space(4.0);
    ui.label(format!("(W3 placeholder · 完整实装在 {todo_label})"));
}

pub fn render(ui: &mut egui::Ui, page: Page, state: &mut AppState) {
    match page {
        Page::Dashboard => dashboard::render(ui, state),
        Page::Providers => providers::render(ui, state),
        Page::ProvidersAdd => providers_add::render(ui, state),
        Page::Desktop => desktop::render(ui, state),
        Page::Proxy => proxy::render(ui, state),
        Page::Settings => settings::render(ui, state),
        Page::Guide => guide::render(ui, state),
    }
}
