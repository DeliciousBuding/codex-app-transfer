//! Page 路由 + 各 page 实装入口。
//!
//! W4 起 page::render 接收 `&mut Page`,允许 page 内部跳转(typical:
//! Providers 页里点 "edit" → providers::render 设 state.nav_to_providers_add,
//! mod.rs 检测后切到 Page::ProvidersAdd)。

use eframe::egui;

use crate::background::Bg;
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

pub fn render(ui: &mut egui::Ui, page: &mut Page, state: &mut AppState, bg: &Bg) {
    match *page {
        Page::Dashboard => dashboard::render(ui, state, bg),
        Page::Providers => providers::render(ui, state),
        Page::ProvidersAdd => providers_add::render(ui, state, bg),
        Page::Desktop => desktop::render(ui, state, bg),
        Page::Proxy => proxy::render(ui, state, bg),
        Page::Settings => settings::render(ui, state, bg),
        Page::Guide => guide::render(ui, state),
    }
    if state.nav_to_providers_add {
        state.nav_to_providers_add = false;
        *page = Page::ProvidersAdd;
    }
    if state.nav_back_to_providers {
        state.nav_back_to_providers = false;
        *page = Page::Providers;
    }
}
