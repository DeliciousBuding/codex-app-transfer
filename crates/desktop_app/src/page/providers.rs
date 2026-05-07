//! Providers list page (W4 完整实装).
//!
//! 上下移动按钮 + 设默认 + 编辑(跳到 providers/add 编辑模式)+ 删除
//! (经 deleteModal 确认)+ 添加按钮(跳到 providers/add 新建模式)。
//! drag-drop reorder 留 W6 增强,W4 用上下箭头按钮够用。

use eframe::egui;

use crate::i18n::lookup_owned;
use crate::state::AppState;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let locale = state.settings.language;

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.heading(lookup_owned(locale, "providers.title"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(format!("+ {}", lookup_owned(locale, "providers.add")))
                        .clicked()
                    {
                        state.form = crate::state::ProviderForm::empty();
                        state.nav_to_providers_add = true;
                    }
                });
            });
            ui.add_space(4.0);
            ui.weak(lookup_owned(locale, "providers.subtitle"));
            ui.add_space(20.0);

            // 表头
            ui.horizontal(|ui| {
                col_header(ui, lookup_owned(locale, "providers.name"), 200.0);
                col_header(ui, lookup_owned(locale, "providers.baseUrl"), 280.0);
                col_header(ui, lookup_owned(locale, "providers.mapping"), 180.0);
                col_header(ui, lookup_owned(locale, "providers.status"), 80.0);
                col_header(ui, lookup_owned(locale, "providers.actions"), 240.0);
            });
            ui.separator();

            if state.providers.is_empty() {
                ui.add_space(12.0);
                ui.weak(lookup_owned(locale, "providers.empty"));
                return;
            }

            #[derive(Default)]
            struct Pending {
                edit: Option<String>,
                delete: Option<String>,
                set_default: Option<String>,
                move_up: Option<String>,
                move_down: Option<String>,
            }
            let mut pending = Pending::default();
            let row_count = state.providers.len();

            for (idx, p) in state.providers.iter().enumerate() {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(200.0, 26.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.strong(&p.name);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(280.0, 26.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.weak(p.base_url.clone());
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(180.0, 26.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.weak(format!("→ {}", p.default_model));
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(80.0, 26.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            if p.is_default {
                                ui.colored_label(
                                    egui::Color32::from_rgb(0x16, 0xa3, 0x4a),
                                    format!("✓ {}", lookup_owned(locale, "providers.default")),
                                );
                            } else if !p.has_api_key {
                                ui.colored_label(egui::Color32::from_rgb(0xf5, 0x9e, 0x0b), "⚠");
                            } else {
                                ui.weak("·");
                            }
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(240.0, 26.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            if !p.is_default
                                && ui
                                    .small_button(lookup_owned(locale, "providers.setDefault"))
                                    .clicked()
                            {
                                pending.set_default = Some(p.id.clone());
                            }
                            if ui
                                .small_button(lookup_owned(locale, "common.edit"))
                                .clicked()
                            {
                                pending.edit = Some(p.id.clone());
                            }
                            if idx > 0 && ui.small_button("↑").clicked() {
                                pending.move_up = Some(p.id.clone());
                            }
                            if idx + 1 < row_count && ui.small_button("↓").clicked() {
                                pending.move_down = Some(p.id.clone());
                            }
                            if ui
                                .small_button(format!(
                                    "🗑 {}",
                                    lookup_owned(locale, "common.delete")
                                ))
                                .clicked()
                            {
                                pending.delete = Some(p.id.clone());
                            }
                        },
                    );
                });
                ui.separator();
            }

            if let Some(id) = pending.set_default {
                state.set_default_provider(&id);
            }
            if let Some(id) = pending.edit {
                state.load_provider_into_form(&id);
                state.nav_to_providers_add = true;
            }
            if let Some(id) = pending.move_up {
                state.move_provider(&id, -1);
            }
            if let Some(id) = pending.move_down {
                state.move_provider(&id, 1);
            }
            if let Some(id) = pending.delete {
                state.confirm_delete_id = Some(id);
            }

            if let Some(err) = &state.config_save_error {
                ui.add_space(12.0);
                ui.colored_label(egui::Color32::RED, format!("⚠ {err}"));
            }
        });
}

fn col_header(ui: &mut egui::Ui, text: impl Into<String>, width: f32) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 24.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.weak(text.into());
        },
    );
}
