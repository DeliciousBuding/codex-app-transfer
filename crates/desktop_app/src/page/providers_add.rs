//! Providers/Add page (W4 完整实装).
//!
//! 表单 + 模型映射 grid + preset 列表 + base_url 多选(部分 preset 提供
//! 多区域 URL,如 Mimo Token Plan)。同步 action(save / cancel)直接写
//! config.json;async action(test provider / fetch models / apply-desktop)
//! W6 接 tokio runtime。

use eframe::egui;

use crate::background::{Bg, UiAction};
use crate::i18n::lookup_owned;
use crate::state::AppState;

pub fn render(ui: &mut egui::Ui, state: &mut AppState, bg: &Bg) {
    let locale = state.settings.language;
    let editing = state.form.editing_id.is_some();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.add_space(12.0);
            ui.heading(if editing {
                lookup_owned(locale, "providers.edit")
            } else {
                lookup_owned(locale, "providersAdd.title")
            });
            ui.add_space(4.0);
            ui.weak(lookup_owned(locale, "providersAdd.subtitle"));
            ui.add_space(20.0);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(560.0);
                    render_form(ui, state, locale, bg);
                });
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.set_min_width(280.0);
                    render_preset_list(ui, state, locale);
                });
            });
        });
}

fn render_form(ui: &mut egui::Ui, state: &mut AppState, locale: crate::i18n::Locale, bg: &Bg) {
    ui.label(lookup_owned(locale, "providers.name"));
    ui.add(
        egui::TextEdit::singleline(&mut state.form.name)
            .desired_width(560.0)
            .hint_text("My DeepSeek"),
    );
    ui.add_space(10.0);

    ui.label(lookup_owned(locale, "providers.baseUrl"));
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.form.base_url)
                .desired_width(420.0)
                .hint_text("https://api.example.com/v1"),
        );
        if !state.form.base_url_options.is_empty() {
            egui::ComboBox::from_id_salt("base_url_menu")
                .width(120.0)
                .selected_text(lookup_owned(locale, "providers.cluster"))
                .show_ui(ui, |ui| {
                    let mut chosen: Option<String> = None;
                    for (label, value) in &state.form.base_url_options {
                        if ui
                            .selectable_label(state.form.base_url == *value, label)
                            .clicked()
                        {
                            chosen = Some(value.clone());
                        }
                    }
                    if let Some(v) = chosen {
                        state.form.base_url = v;
                    }
                });
        }
    });
    ui.add_space(10.0);

    ui.label("API Key");
    ui.horizontal(|ui| {
        let edit = egui::TextEdit::singleline(&mut state.form.api_key)
            .desired_width(420.0)
            .hint_text("sk-...")
            .password(!state.api_key_visible);
        ui.add(edit);
        let label = if state.api_key_visible {
            lookup_owned(locale, "common.hide")
        } else {
            lookup_owned(locale, "common.show")
        };
        if ui.button(label).clicked() {
            state.api_key_visible = !state.api_key_visible;
        }
    });
    ui.add_space(16.0);

    egui::CollapsingHeader::new(lookup_owned(locale, "providersAdd.compatTitle"))
        .default_open(false)
        .show(ui, |ui| {
            ui.weak(lookup_owned(locale, "providersAdd.compatHint"));
            ui.add_space(6.0);

            ui.label(lookup_owned(locale, "providersAdd.formatTitle"));
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(
                        state.form.api_format == "openai_chat",
                        lookup_owned(locale, "providersAdd.formatResponses"),
                    )
                    .clicked()
                {
                    state.form.api_format = "openai_chat".into();
                }
                if ui
                    .selectable_label(
                        state.form.api_format == "responses",
                        lookup_owned(locale, "providersAdd.formatOpenAI"),
                    )
                    .clicked()
                {
                    state.form.api_format = "responses".into();
                }
            });
            ui.add_space(6.0);

            ui.label(lookup_owned(locale, "providers.authScheme"));
            ui.horizontal(|ui| {
                for opt in ["bearer", "x-api-key", "none"] {
                    if ui
                        .selectable_label(state.form.auth_scheme == opt, opt)
                        .clicked()
                    {
                        state.form.auth_scheme = opt.into();
                    }
                }
            });
        });
    ui.add_space(16.0);

    ui.label(lookup_owned(locale, "providersAdd.mappingTitle"));
    ui.weak(lookup_owned(locale, "providersAdd.mappingSubtitle"));
    ui.add_space(6.0);

    egui::Grid::new("model_mapping")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            for (slot, target) in state.form.mappings.iter_mut() {
                ui.label(slot_label(slot));
                ui.add(
                    egui::TextEdit::singleline(target)
                        .desired_width(360.0)
                        .hint_text("upstream model id"),
                );
                ui.end_row();
            }
        });
    ui.add_space(8.0);

    if ui
        .button(format!("☁ {}", lookup_owned(locale, "models.fetch")))
        .clicked()
    {
        bg.dispatch(UiAction::FetchModels {
            base_url: state.form.base_url.clone(),
            api_key: state.form.api_key.clone(),
        });
    }
    if !state.available_models.is_empty() {
        ui.add_space(4.0);
        ui.weak(format!(
            "{}: {}",
            lookup_owned(locale, "models.fetchedHint"),
            state.available_models.join(", ")
        ));
    }

    ui.add_space(20.0);
    ui.separator();
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        if ui
            .button(format!("▶ {}", lookup_owned(locale, "providers.enable")))
            .clicked()
        {
            state.save_form();
            // 启用 = save + apply Codex(异步)
            bg.dispatch(UiAction::ApplyDesktop);
            state.nav_back_to_providers = true;
        }
        if ui.button(lookup_owned(locale, "common.saveOnly")).clicked() {
            state.save_form();
            state.nav_back_to_providers = true;
        }
        if ui.button(lookup_owned(locale, "common.cancel")).clicked() {
            state.nav_back_to_providers = true;
        }
    });

    if let Some(err) = &state.config_save_error {
        ui.add_space(8.0);
        ui.colored_label(egui::Color32::RED, format!("⚠ {err}"));
    }
}

fn render_preset_list(ui: &mut egui::Ui, state: &mut AppState, locale: crate::i18n::Locale) {
    ui.heading(lookup_owned(locale, "providersAdd.presets"));
    ui.add_space(4.0);
    ui.weak(lookup_owned(locale, "providersAdd.presetsHint"));
    ui.add_space(8.0);

    let mut chosen: Option<usize> = None;
    for (idx, preset) in state.presets.iter().enumerate() {
        let name = preset
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("Unknown");
        let base = preset.get("baseUrl").and_then(|x| x.as_str()).unwrap_or("");

        let frame = egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::symmetric(10, 8))
            .corner_radius(egui::CornerRadius::same(8));
        frame.show(ui, |ui| {
            ui.set_min_width(260.0);
            ui.vertical(|ui| {
                ui.strong(name);
                ui.weak(base);
                if ui
                    .small_button(lookup_owned(locale, "presets.use"))
                    .clicked()
                {
                    chosen = Some(idx);
                }
            });
        });
        ui.add_space(6.0);
    }

    if let Some(idx) = chosen {
        let preset = state.presets[idx].clone();
        state.fill_form_from_preset(&preset);
    }
}

fn slot_label(slot: &str) -> String {
    match slot {
        "default" => "Default *".into(),
        "gpt_5_5" => "gpt-5.5".into(),
        "gpt_5_4" => "gpt-5.4".into(),
        "gpt_5_4_mini" => "gpt-5.4-mini".into(),
        "gpt_5_3_codex" => "gpt-5.3-codex".into(),
        "gpt_5_2" => "gpt-5.2".into(),
        s => s.to_owned(),
    }
}
