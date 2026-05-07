//! Desktop (Codex CLI) page (W5 完整实装).
//!
//! 显示当前 ~/.codex/config.toml 关键字段 + JSON 预览 + 应用/还原/重启
//! Codex App 三按钮(W6 wire codex_integration)+ 3 步 mini-step 引导。

use eframe::egui;

use crate::background::{Bg, UiAction};
use crate::i18n::lookup_owned;
use crate::state::AppState;

pub fn render(ui: &mut egui::Ui, state: &mut AppState, bg: &Bg) {
    let locale = state.settings.language;
    let codex_dir = home_dir().map(|h| h.join(".codex"));

    // 读 ~/.codex/config.toml(尽量;失败用空)
    let toml_content: String = codex_dir
        .as_ref()
        .map(|d| d.join("config.toml"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.add_space(12.0);
            ui.heading(lookup_owned(locale, "desktop.title"));
            ui.add_space(4.0);
            ui.weak(lookup_owned(locale, "desktop.subtitle"));
            ui.add_space(20.0);

            // 状态摘要
            let applied = toml_content.contains("model_catalog_json")
                || toml_content.contains("openai_base_url");
            ui.horizontal(|ui| {
                if applied {
                    ui.colored_label(
                        egui::Color32::from_rgb(0x16, 0xa3, 0x4a),
                        format!("✓ {}", lookup_owned(locale, "status.configured")),
                    );
                } else {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xf5, 0x9e, 0x0b),
                        format!("· {}", lookup_owned(locale, "status.notConfigured")),
                    );
                }
                ui.weak(format!(
                    " · {}",
                    codex_dir
                        .as_ref()
                        .map(|d| d.display().to_string())
                        .unwrap_or_default()
                ));
            });
            ui.add_space(16.0);

            // 关键 config 列表
            ui.strong(lookup_owned(locale, "desktop.configTitle"));
            ui.add_space(6.0);
            for key in [
                "openai_base_url",
                "model_catalog_json",
                "model_context_window",
                "model",
            ] {
                if let Some(line) = grab_line(&toml_content, key) {
                    ui.monospace(format!("  {line}"));
                }
            }
            ui.add_space(16.0);

            // 操作按钮(W6 wire)
            ui.horizontal(|ui| {
                if ui
                    .button(format!("⬇ {}", lookup_owned(locale, "desktop.apply")))
                    .clicked()
                {
                    bg.dispatch(UiAction::ApplyDesktop);
                }
                if ui
                    .button(format!("↺ {}", lookup_owned(locale, "desktop.clear")))
                    .clicked()
                {
                    bg.dispatch(UiAction::ClearDesktop);
                }
                if ui.button(lookup_owned(locale, "desktop.restart")).clicked() {
                    bg.dispatch(UiAction::RestartCodex);
                }
            });
            ui.add_space(20.0);

            // JSON 预览(显示 toml 全文)
            ui.collapsing(lookup_owned(locale, "desktop.details"), |ui| {
                if toml_content.is_empty() {
                    ui.weak("(empty / not found)");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .id_salt("toml_preview")
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut toml_content.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(20),
                            );
                        });
                }
            });
            ui.add_space(20.0);

            // 3 步 mini-step
            ui.heading(lookup_owned(locale, "desktop.quickGuide"));
            ui.add_space(6.0);
            mini_step(
                ui,
                "1",
                lookup_owned(locale, "desktop.step1Title"),
                lookup_owned(locale, "desktop.step1Text"),
            );
            mini_step(
                ui,
                "2",
                lookup_owned(locale, "desktop.step2Title"),
                lookup_owned(locale, "desktop.step2Text"),
            );
            mini_step(
                ui,
                "3",
                lookup_owned(locale, "desktop.step3Title"),
                lookup_owned(locale, "desktop.step3Text"),
            );

            // 测试 modal 触发(W5 验证 restartReminderModal 渲染;W6 由 apply
            // 流程自动触发)
            ui.add_space(20.0);
            ui.separator();
            ui.add_space(8.0);
            if ui
                .small_button("(debug) test restartReminderModal")
                .on_hover_text(
                    "W5 临时:验证 modal 渲染。W6 起改由 apply-codex 流程触发,删除本按钮。",
                )
                .clicked()
            {
                state.show_restart_reminder = true;
            }
        });
}

fn mini_step(ui: &mut egui::Ui, num: &str, title: String, text: String) {
    let frame = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(14, 10))
        .corner_radius(egui::CornerRadius::same(10));
    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(0x14, 0x76, 0xff),
                egui::RichText::new(num).strong().size(18.0),
            );
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.strong(title);
                ui.weak(text);
            });
        });
    });
    ui.add_space(6.0);
}

fn grab_line(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(key) {
            return Some(trimmed.to_owned());
        }
    }
    None
}

fn home_dir() -> Option<std::path::PathBuf> {
    if let Some(h) = std::env::var_os("HOME") {
        return Some(std::path::PathBuf::from(h));
    }
    if let Some(h) = std::env::var_os("USERPROFILE") {
        return Some(std::path::PathBuf::from(h));
    }
    None
}
