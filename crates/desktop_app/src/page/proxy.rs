//! Proxy page (W5 完整实装).
//!
//! 启停按钮(渲染,W6 wire ProxyManager)+ 端口输入 + stats 卡片 +
//! 实时日志面板(`egui_extras::TableBuilder` virtualize,可承载 10k+ 行)+
//! 自动滚动开关 + 清空 / 打开日志目录按钮。

use eframe::egui;

use crate::i18n::lookup_owned;
use crate::state::AppState;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let locale = state.settings.language;

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.add_space(12.0);
            ui.heading(lookup_owned(locale, "proxy.title"));
            ui.add_space(4.0);
            ui.weak(lookup_owned(locale, "proxy.subtitle"));
            ui.add_space(20.0);

            ui.horizontal(|ui| {
                let _ = ui
                    .button(format!("▶ {}", lookup_owned(locale, "proxy.start")))
                    .on_hover_text("W6 wire ProxyManager.start(port).await");
                let _ = ui
                    .button(format!("⏹ {}", lookup_owned(locale, "proxy.stop")))
                    .on_hover_text("W6 wire ProxyManager.stop()");
                ui.add_space(20.0);
                ui.label(lookup_owned(locale, "settings.proxyPort"));
                let mut p = state.settings.proxy_port;
                if ui
                    .add(egui::DragValue::new(&mut p).range(1024..=65535).speed(1))
                    .changed()
                {
                    state.settings.proxy_port = p;
                    state.save_settings();
                }
                ui.weak(format!("· 127.0.0.1:{}", state.settings.proxy_port));
            });
            ui.add_space(16.0);

            let snap = codex_app_transfer_proxy::proxy_telemetry().stats.snapshot();
            ui.horizontal_wrapped(|ui| {
                stat_card(
                    ui,
                    "Total",
                    snap.total,
                    egui::Color32::from_rgb(0x14, 0x76, 0xff),
                );
                stat_card(
                    ui,
                    "Success",
                    snap.success,
                    egui::Color32::from_rgb(0x16, 0xa3, 0x4a),
                );
                stat_card(
                    ui,
                    "Failed",
                    snap.failed,
                    egui::Color32::from_rgb(0xff, 0x4d, 0x4f),
                );
            });
            ui.add_space(16.0);

            ui.horizontal(|ui| {
                ui.strong(lookup_owned(locale, "proxy.log"));
                ui.add_space(8.0);
                ui.checkbox(
                    &mut state.proxy_log_auto_scroll,
                    lookup_owned(locale, "proxy.autoScroll"),
                );
                ui.add_space(8.0);
                if ui.button(lookup_owned(locale, "proxy.clearLog")).clicked() {
                    codex_app_transfer_proxy::proxy_telemetry().logs.clear();
                }
                let _ = ui
                    .button(lookup_owned(locale, "proxy.viewLog"))
                    .on_hover_text("W6 wire opener::open(proxy_log_dir)");
            });
            ui.add_space(6.0);

            render_log_panel(ui, state);

            if let Some(err) = &state.config_save_error {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::RED, format!("⚠ {err}"));
            }
        });
}

fn stat_card(ui: &mut egui::Ui, label: &str, value: u64, color: egui::Color32) {
    let frame = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(16, 10))
        .corner_radius(egui::CornerRadius::same(10));
    frame.show(ui, |ui| {
        ui.set_min_width(140.0);
        ui.vertical(|ui| {
            ui.weak(label);
            ui.add_space(4.0);
            ui.colored_label(
                color,
                egui::RichText::new(value.to_string()).size(20.0).strong(),
            );
        });
    });
}

fn render_log_panel(ui: &mut egui::Ui, state: &AppState) {
    use egui_extras::{Column, TableBuilder};

    let logs = codex_app_transfer_proxy::proxy_telemetry().logs.get_all();
    let row_count = logs.len();

    let frame = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(8))
        .corner_radius(egui::CornerRadius::same(8));
    frame.show(ui, |ui| {
        ui.set_min_height(360.0);
        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto().at_least(80.0))
            .column(Column::auto().at_least(70.0))
            .column(Column::remainder())
            .min_scrolled_height(360.0);

        if state.proxy_log_auto_scroll && row_count > 0 {
            builder = builder.scroll_to_row(row_count.saturating_sub(1), Some(egui::Align::BOTTOM));
        }

        builder
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("time");
                });
                header.col(|ui| {
                    ui.strong("level");
                });
                header.col(|ui| {
                    ui.strong("message");
                });
            })
            .body(|body| {
                body.rows(18.0, row_count, |mut row| {
                    let entry = &logs[row.index()];
                    row.col(|ui| {
                        ui.monospace(&entry.time);
                    });
                    row.col(|ui| {
                        let color = match entry.level.as_str() {
                            "ERROR" => egui::Color32::from_rgb(0xff, 0x4d, 0x4f),
                            "WARN" => egui::Color32::from_rgb(0xf5, 0x9e, 0x0b),
                            "SUCCESS" => egui::Color32::from_rgb(0x16, 0xa3, 0x4a),
                            _ => egui::Color32::from_rgb(0x66, 0x70, 0x85),
                        };
                        ui.colored_label(color, &entry.level);
                    });
                    row.col(|ui| {
                        ui.label(&entry.message);
                    });
                });
            });
    });
}
