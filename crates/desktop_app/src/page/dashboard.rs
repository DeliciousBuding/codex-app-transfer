//! Dashboard page (W3 完整实装).
//!
//! 三个 hero 卡片(桌面 / 代理 / 当前 provider)+ provider 卡片网格 +
//! activity 占位。所有 action(apply/clear/proxy-start)只渲染按钮,不接通
//! 后端调用 — W6 用 tokio runtime 通连后再 wire。

use eframe::egui;

use crate::background::{Bg, UiAction};
use crate::i18n::lookup_owned;
use crate::state::AppState;

pub fn render(ui: &mut egui::Ui, state: &mut AppState, bg: &Bg) {
    let locale = state.settings.language;
    // 把所有依赖 state 的判定先算掉,避免与下面 &state 借用冲突
    let codex_applied = cfg_applied_hint(state);
    let active_present = state.active_provider_id.is_some();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.add_space(12.0);
            ui.heading(lookup_owned(locale, "dashboard.title"));
            ui.add_space(4.0);
            ui.weak(lookup_owned(locale, "dashboard.subtitle"));
            ui.add_space(20.0);

            // 三个 hero 卡片 horizontal
            ui.horizontal_wrapped(|ui| {
                hero(
                    ui,
                    locale,
                    "dashboard.desktopStatus",
                    if codex_applied {
                        "status.configured"
                    } else {
                        "status.notConfigured"
                    },
                    codex_applied,
                );
                hero(
                    ui,
                    locale,
                    "dashboard.proxyStatus",
                    "proxy.notRunning",
                    false,
                );
                hero(
                    ui,
                    locale,
                    "dashboard.activeProvider",
                    if active_present {
                        "provider.active"
                    } else {
                        "provider.none"
                    },
                    active_present,
                );
            });

            ui.add_space(16.0);

            // 行动按钮(W6 wire)
            ui.horizontal(|ui| {
                if ui
                    .button(format!(
                        "✨ {}",
                        lookup_owned(locale, "dashboard.configureDesktop")
                    ))
                    .clicked()
                {
                    state.nav_to_providers_add = true;
                    state.form = crate::state::ProviderForm::empty();
                }
                ui.add_space(4.0);
                if !state.proxy_running {
                    if ui
                        .button(format!("▶ {}", lookup_owned(locale, "proxy.start")))
                        .clicked()
                    {
                        bg.dispatch(UiAction::StartProxy);
                    }
                } else if ui
                    .button(format!("⏹ {}", lookup_owned(locale, "proxy.stop")))
                    .clicked()
                {
                    bg.dispatch(UiAction::StopProxy);
                }
                ui.add_space(4.0);
                let _ = ui.button(format!(
                    "↔ {}",
                    lookup_owned(locale, "dashboard.switchProvider")
                ));
            });
            ui.add_space(20.0);

            // Provider 卡片网格
            ui.heading(format!("{}", lookup_owned(locale, "providers.title")));
            ui.add_space(8.0);
            if state.providers.is_empty() {
                ui.weak(lookup_owned(locale, "providers.empty"));
            } else {
                ui.horizontal_wrapped(|ui| {
                    for p in state.providers.iter() {
                        provider_card(ui, p, locale);
                    }
                });
            }

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(8.0);

            // Recent activity placeholder
            ui.strong(lookup_owned(locale, "dashboard.recentActivity"));
            ui.add_space(4.0);
            ui.weak(format!("(W3 placeholder · 实时 activity 列表 W6 接通)"));

            // 错误提示
            if let Some(err) = &state.config_load_error {
                ui.add_space(20.0);
                ui.colored_label(egui::Color32::RED, format!("⚠ load error: {err}"));
            }
            if let Some(err) = &state.config_save_error {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::RED, format!("⚠ save error: {err}"));
            }
        });
}

fn hero(
    ui: &mut egui::Ui,
    locale: crate::i18n::Locale,
    title_key: &str,
    state_key: &str,
    is_ok: bool,
) {
    let frame = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(20, 14))
        .corner_radius(egui::CornerRadius::same(12));
    frame.show(ui, |ui| {
        ui.set_min_width(180.0);
        ui.vertical(|ui| {
            ui.weak(lookup_owned(locale, title_key));
            ui.add_space(6.0);
            let label = lookup_owned(locale, state_key);
            if is_ok {
                ui.colored_label(
                    egui::Color32::from_rgb(0x16, 0xa3, 0x4a),
                    format!("✓ {label}"),
                );
            } else {
                ui.weak(format!("· {label}"));
            }
        });
    });
}

fn provider_card(ui: &mut egui::Ui, p: &crate::state::ProviderItem, locale: crate::i18n::Locale) {
    let frame = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(16, 12))
        .corner_radius(egui::CornerRadius::same(12));
    frame.show(ui, |ui| {
        ui.set_min_width(220.0);
        ui.set_max_width(280.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.strong(&p.name);
                if p.is_default {
                    ui.weak(format!("· {}", lookup_owned(locale, "providers.default")));
                }
            });
            ui.add_space(2.0);
            ui.weak(p.base_url.clone());
            ui.add_space(2.0);
            ui.weak(format!("→ {}", p.default_model));
            ui.add_space(6.0);
            if !p.has_api_key {
                ui.colored_label(
                    egui::Color32::from_rgb(0xf5, 0x9e, 0x0b),
                    format!("⚠ {}", lookup_owned(locale, "providers.noApiKey")),
                );
            }
        });
    });
}

/// W3 起步:粗略估计 — 看 ~/.codex/config.toml 是否存在 OPENAI_API_KEY 或
/// model_catalog_json。W6 接 codex_integration::is_codex_applied 真实判定。
fn cfg_applied_hint(_state: &AppState) -> bool {
    let home = match dirs_or_home() {
        Some(p) => p,
        None => return false,
    };
    let toml = home.join(".codex").join("config.toml");
    if let Ok(content) = std::fs::read_to_string(&toml) {
        return content.contains("model_catalog_json") || content.contains("openai_base_url");
    }
    false
}

fn dirs_or_home() -> Option<std::path::PathBuf> {
    if let Some(h) = std::env::var_os("HOME") {
        return Some(std::path::PathBuf::from(h));
    }
    if let Some(h) = std::env::var_os("USERPROFILE") {
        return Some(std::path::PathBuf::from(h));
    }
    None
}
