//! Settings page (W3 完整实装).
//!
//! 完整实装 20 项设置 UI 渲染。其中:
//! - Theme / Language / 双 Port / 4 个开关 / Update URL → **W3 sync wire**
//!   (改一下立即写回 ~/.codex-app-transfer/config.json)
//! - Compatibility check / Backup / Export / Import / Feedback / Check Update
//!   → **W6 async wire**(需要 tokio runtime + reqwest;W3 只渲染按钮,
//!   点击 push 一个 toast 占位)
//!
//! 以上对应跟踪文档 §2 「20 个 action」表中 8/20 在本 page。

use eframe::egui;

use crate::background::{Bg, UiAction};
use crate::i18n::{lookup_owned, Locale};
use crate::state::AppState;
use crate::theme::ThemeName;

pub fn render(ui: &mut egui::Ui, state: &mut AppState, bg: &Bg) {
    let locale = state.settings.language;
    let mut dirty = false;

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.add_space(12.0);
            ui.heading(lookup_owned(locale, "nav.settings"));
            ui.add_space(20.0);

            // ── 主题 ──
            section(ui, locale, "settings.theme", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for &t in ThemeName::ALL {
                        let selected = state.settings.theme == t;
                        if ui
                            .selectable_label(selected, t.label())
                            .on_hover_text(theme_hover(t))
                            .clicked()
                            && !selected
                        {
                            state.settings.theme = t;
                            dirty = true;
                        }
                    }
                });
            });

            // ── 语言 ──
            section(ui, locale, "settings.language", |ui| {
                if ui
                    .selectable_label(locale == Locale::Zh, Locale::Zh.label())
                    .clicked()
                    && locale != Locale::Zh
                {
                    state.settings.language = Locale::Zh;
                    dirty = true;
                }
                if ui
                    .selectable_label(locale == Locale::En, Locale::En.label())
                    .clicked()
                    && locale != Locale::En
                {
                    state.settings.language = Locale::En;
                    dirty = true;
                }
            });

            // ── 转发端口 / 管理端口 ──
            section(ui, locale, "settings.proxyPort", |ui| {
                let mut p = state.settings.proxy_port;
                if ui
                    .add(egui::DragValue::new(&mut p).range(1024..=65535).speed(1))
                    .changed()
                {
                    state.settings.proxy_port = p;
                    dirty = true;
                }
            });
            section(ui, locale, "settings.adminPort", |ui| {
                let mut p = state.settings.admin_port;
                if ui
                    .add(egui::DragValue::new(&mut p).range(1024..=65535).speed(1))
                    .changed()
                {
                    state.settings.admin_port = p;
                    dirty = true;
                }
            });

            // ── 4 个开关 ──
            section_with_hint(
                ui,
                locale,
                "settings.autoApplyOnStart",
                "settings.autoApplyOnStartHint",
                |ui| {
                    if ui
                        .checkbox(&mut state.settings.auto_apply_on_start, "")
                        .changed()
                    {
                        dirty = true;
                    }
                },
            );
            section_with_hint(
                ui,
                locale,
                "settings.restoreCodexOnExit",
                "settings.restoreCodexOnExitHint",
                |ui| {
                    if ui
                        .checkbox(&mut state.settings.restore_codex_on_exit, "")
                        .changed()
                    {
                        dirty = true;
                    }
                },
            );
            section(ui, locale, "settings.exposeAllModels", |ui| {
                if ui
                    .checkbox(&mut state.settings.expose_all_provider_models, "")
                    .changed()
                {
                    dirty = true;
                }
            });
            section(ui, locale, "settings.autoStart", |ui| {
                if ui.checkbox(&mut state.settings.auto_start, "").changed() {
                    dirty = true;
                }
            });

            // ── Update URL ──
            section(ui, locale, "settings.updateUrl", |ui| {
                let mut url = state.settings.update_url.clone();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut url)
                            .desired_width(380.0)
                            .hint_text("https://..."),
                    )
                    .lost_focus()
                {
                    if url != state.settings.update_url {
                        state.settings.update_url = url;
                        dirty = true;
                    }
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);

            // ── 第三方兼容性(W6 wire)──
            section_with_hint(
                ui,
                locale,
                "settings.thirdPartyCompat",
                "settings.thirdPartyCompatHint",
                |ui| {
                    if ui
                        .button(format!(
                            "✓ {}",
                            lookup_owned(locale, "settings.checkCompatibility")
                        ))
                        .clicked()
                    {
                        // W6: 串行 test_provider over each;暂以 toast 提示
                        // (compat 矩阵 UI 留给后续 commit 增强)
                        for p in &state.providers {
                            bg.dispatch(UiAction::TestProvider {
                                base_url: p.base_url.clone(),
                                api_key: String::new(), // 空 key 让 401 也能反映 endpoint 可达
                                model: p.default_model.clone(),
                            });
                        }
                    }
                },
            );

            // ── 配置备份(W6 wire)──
            section_with_hint(
                ui,
                locale,
                "settings.configBackup",
                "settings.configBackupHint",
                |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .button(lookup_owned(locale, "settings.backupNow"))
                            .clicked()
                        {
                            bg.dispatch(UiAction::BackupConfig);
                        }
                        if ui
                            .button(lookup_owned(locale, "settings.exportConfig"))
                            .clicked()
                        {
                            bg.dispatch(UiAction::ExportConfig);
                        }
                        if ui
                            .button(lookup_owned(locale, "settings.importConfig"))
                            .clicked()
                        {
                            bg.dispatch(UiAction::ImportConfig);
                        }
                    });
                },
            );

            // ── 反馈入口(W6 modal)──
            section_with_hint(
                ui,
                locale,
                "settings.feedback",
                "settings.feedbackHint",
                |ui| {
                    if ui
                        .button(lookup_owned(locale, "settings.feedbackOpen"))
                        .clicked()
                    {
                        state.show_feedback_modal = true;
                    }
                },
            );

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);

            // ── About ──
            ui.heading(lookup_owned(locale, "settings.about"));
            ui.add_space(6.0);
            row(
                ui,
                lookup_owned(locale, "settings.version"),
                env!("CARGO_PKG_VERSION"),
            );
            row(ui, lookup_owned(locale, "settings.license"), "MIT License");
            ui.horizontal(|ui| {
                ui.label(lookup_owned(locale, "settings.checkUpdate"));
                if ui
                    .button(lookup_owned(locale, "settings.checkUpdate"))
                    .clicked()
                {
                    bg.dispatch(UiAction::CheckUpdate);
                }
                if let Some((ver, url)) = state.update_available.clone() {
                    if ui
                        .button(format!(
                            "{} → {ver}",
                            lookup_owned(locale, "settings.installUpdate")
                        ))
                        .clicked()
                    {
                        bg.dispatch(UiAction::InstallUpdate {
                            url: url.unwrap_or_default(),
                        });
                    }
                }
            });

            ui.add_space(20.0);

            // 调试 footer
            ui.weak(format!(
                "i18n keys: {} · locale={} · theme={} · cfg_present={}",
                crate::i18n::KEY_COUNT,
                locale.code(),
                state.settings.theme.label(),
                state.config_present
            ));

            // 错误显示
            if let Some(err) = &state.config_save_error {
                ui.add_space(6.0);
                ui.colored_label(egui::Color32::RED, format!("⚠ save error: {err}"));
            }
        });

    if dirty {
        state.save_settings();
    }
}

fn section(ui: &mut egui::Ui, locale: Locale, title_key: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(220.0, 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.strong(lookup_owned(locale, title_key));
            },
        );
        ui.add_space(8.0);
        body(ui);
    });
    ui.add_space(8.0);
}

fn section_with_hint(
    ui: &mut egui::Ui,
    locale: Locale,
    title_key: &str,
    hint_key: &str,
    body: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(220.0, 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.strong(lookup_owned(locale, title_key));
            },
        );
        ui.add_space(8.0);
        body(ui);
    });
    let hint = lookup_owned(locale, hint_key);
    if hint != hint_key {
        ui.add_space(2.0);
        ui.indent("hint_indent", |ui| {
            ui.weak(hint);
        });
    }
    ui.add_space(8.0);
}

fn row(ui: &mut egui::Ui, label: impl Into<String>, value: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(220.0, 22.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(label.into());
            },
        );
        ui.strong(value.into());
    });
    ui.add_space(4.0);
}

fn theme_hover(t: ThemeName) -> &'static str {
    match t {
        ThemeName::Default => "蓝主基调,style.css 默认",
        ThemeName::Green => "绿主操作色",
        ThemeName::Orange => "橙主操作色",
        ThemeName::Gray => "中性灰",
        ThemeName::Dark => "深色模式",
        ThemeName::White => "极简灰主操作色",
    }
}
