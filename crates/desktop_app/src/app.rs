//! eframe::App 主体。W3:AppState 接 ~/.codex-app-transfer/config.json。
//! W6:接 tokio runtime + 后台 async action,Toast 队列,顶栏反馈/还原 wire。

use std::time::{Duration, Instant};

use eframe::egui;

use crate::background::{Bg, BgEvent, ToastKind, UiAction};
use crate::i18n::lookup_owned;
use crate::page::{self, Page};
use crate::state::AppState;
use crate::theme::{self, ThemeName};

pub struct App {
    pub active_page: Page,
    pub state: AppState,
    pub bg: Bg,
    pub toasts: Vec<Toast>,
    last_applied_theme: Option<ThemeName>,
    last_applied_locale: Option<crate::i18n::Locale>,
}

pub struct Toast {
    pub kind: ToastKind,
    pub message: String,
    pub created_at: Instant,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        try_install_system_cjk_font(&cc.egui_ctx);
        let mut bg = Bg::new();
        bg.set_egui_ctx(cc.egui_ctx.clone());
        Self {
            active_page: Page::Dashboard,
            state: AppState::load(),
            bg,
            toasts: Vec::new(),
            last_applied_theme: None,
            last_applied_locale: None,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. 处理 BgEvent → 更新 state + 入 toast 队列
        let events = crate::background::drain_into(&mut self.state, &mut self.bg);
        for ev in events {
            if let BgEvent::Toast { kind, message } = ev {
                self.toasts.push(Toast {
                    kind,
                    message,
                    created_at: Instant::now(),
                });
            }
        }
        // 老 toast(>4s)淘汰
        self.toasts
            .retain(|t| t.created_at.elapsed() < Duration::from_secs(4));

        // 2. 周期性 reload config.json(检测外部修改)
        self.state.maybe_reload();

        // 主题切换检测 → set_style
        let cur_theme = self.state.settings.theme;
        if self.last_applied_theme != Some(cur_theme) {
            theme::apply(ctx, &cur_theme.palette());
            self.last_applied_theme = Some(cur_theme);
        }
        // Locale 变化时强制 repaint(否则 toml 里的 strong/weak 可能 lazy)
        let cur_locale = self.state.settings.language;
        if self.last_applied_locale != Some(cur_locale) {
            ctx.request_repaint();
            self.last_applied_locale = Some(cur_locale);
        }

        // 顶栏:标题 + 反馈按钮 + 还原 Codex 按钮(后两个 W6 才真正 wire)
        egui::TopBottomPanel::top("top_bar")
            .min_height(48.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.heading("Codex App Transfer");
                    ui.weak(format!("v{}", env!("CARGO_PKG_VERSION")));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        // W6:点了直接调 ClearDesktop async action
                        if ui
                            .button(format!(
                                "↺ {}",
                                lookup_owned(cur_locale, "dashboard.clearDesktopConfig")
                            ))
                            .clicked()
                        {
                            self.bg.dispatch(UiAction::ClearDesktop);
                        }
                        // W6:打开反馈 modal
                        if ui
                            .button(format!(
                                "💬 {}",
                                lookup_owned(cur_locale, "dashboard.feedback")
                            ))
                            .clicked()
                        {
                            self.state.show_feedback_modal = true;
                        }
                    });
                });
            });

        // 左 nav
        egui::SidePanel::left("nav")
            .resizable(false)
            .exact_width(180.0)
            .show(ctx, |ui| {
                ui.add_space(12.0);
                for &page in Page::ALL {
                    let label = lookup_owned(cur_locale, page.nav_key());
                    if ui
                        .selectable_label(self.active_page == page, label)
                        .clicked()
                    {
                        self.active_page = page;
                    }
                }
            });

        // 中心 page
        egui::CentralPanel::default().show(ctx, |ui| {
            page::render(ui, &mut self.active_page, &mut self.state, &self.bg);
        });

        // ── deleteModal(W4 实装第一个 modal,W6 加另两个)──
        if let Some(id) = self.state.confirm_delete_id.clone() {
            let provider_name = self
                .state
                .providers
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| id.clone());
            let mut close = false;
            let mut do_delete = false;
            egui::Window::new(lookup_owned(cur_locale, "providers.deleteTitle"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.set_min_width(360.0);
                    ui.add_space(4.0);
                    ui.label(format!(
                        "{} \"{}\"?",
                        lookup_owned(cur_locale, "providers.deleteMessage"),
                        provider_name
                    ));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(lookup_owned(cur_locale, "common.cancel"))
                            .clicked()
                        {
                            close = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(format!("🗑 {}", lookup_owned(cur_locale, "common.delete")))
                                .clicked()
                            {
                                do_delete = true;
                            }
                        });
                    });
                });
            if do_delete {
                self.state.delete_provider(&id);
                self.state.confirm_delete_id = None;
            } else if close {
                self.state.confirm_delete_id = None;
            }
        }

        // ── restartReminderModal(W5 渲染骨架;W6 已 wire RestartCodex action)──
        if self.state.show_restart_reminder {
            let mut do_now = false;
            let mut do_later = false;
            egui::Window::new(lookup_owned(cur_locale, "restartReminder.title"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.set_min_width(420.0);
                    ui.add_space(4.0);
                    ui.label(lookup_owned(cur_locale, "restartReminder.body"));
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(lookup_owned(cur_locale, "restartReminder.later"))
                            .clicked()
                        {
                            do_later = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(format!(
                                    "▶ {}",
                                    lookup_owned(cur_locale, "restartReminder.now")
                                ))
                                .clicked()
                            {
                                do_now = true;
                            }
                        });
                    });
                });
            if do_now {
                self.bg.dispatch(UiAction::RestartCodex);
                self.state.show_restart_reminder = false;
            } else if do_later {
                self.state.show_restart_reminder = false;
            }
        }

        // ── feedbackModal(W6 第三 modal)──
        if self.state.show_feedback_modal {
            let mut close = false;
            let mut submit = false;
            egui::Window::new(lookup_owned(cur_locale, "feedback.title"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.set_min_width(520.0);
                    ui.add_space(4.0);
                    ui.weak(lookup_owned(cur_locale, "feedback.intro"));
                    ui.add_space(8.0);

                    ui.label(lookup_owned(cur_locale, "feedback.titleLabel"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.state.feedback_title)
                            .desired_width(500.0)
                            .hint_text(lookup_owned(cur_locale, "feedback.titlePlaceholder")),
                    );
                    ui.add_space(6.0);

                    ui.label(lookup_owned(cur_locale, "feedback.bodyLabel"));
                    ui.add(
                        egui::TextEdit::multiline(&mut self.state.feedback_body)
                            .desired_width(500.0)
                            .desired_rows(8)
                            .hint_text(lookup_owned(cur_locale, "feedback.bodyPlaceholder")),
                    );
                    ui.add_space(6.0);

                    ui.checkbox(
                        &mut self.state.feedback_include_diagnostics,
                        lookup_owned(cur_locale, "feedback.includeDiagnostics"),
                    );
                    ui.weak(lookup_owned(cur_locale, "feedback.includeDiagnosticsHint"));
                    ui.add_space(4.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(0xf5, 0x9e, 0x0b),
                        lookup_owned(cur_locale, "feedback.privacyWarning"),
                    );
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        if ui
                            .button(lookup_owned(cur_locale, "common.cancel"))
                            .clicked()
                        {
                            close = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(lookup_owned(cur_locale, "feedback.submit"))
                                .clicked()
                            {
                                submit = true;
                            }
                        });
                    });
                });
            if submit {
                if self.state.feedback_body.trim().is_empty() {
                    self.toasts.push(Toast {
                        kind: ToastKind::Warn,
                        message: lookup_owned(cur_locale, "feedback.bodyRequired"),
                        created_at: Instant::now(),
                    });
                } else {
                    self.bg.dispatch(UiAction::SubmitFeedback {
                        title: self.state.feedback_title.clone(),
                        body: self.state.feedback_body.clone(),
                        include_diagnostics: self.state.feedback_include_diagnostics,
                    });
                }
            }
            if close {
                self.state.show_feedback_modal = false;
            }
        }

        // ── Toast 队列(右下角浮动)──
        if !self.toasts.is_empty() {
            egui::Area::new(egui::Id::new("toasts"))
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        for t in self.toasts.iter().rev().take(4) {
                            let color = match t.kind {
                                ToastKind::Info => egui::Color32::from_rgb(0x14, 0x76, 0xff),
                                ToastKind::Success => egui::Color32::from_rgb(0x16, 0xa3, 0x4a),
                                ToastKind::Warn => egui::Color32::from_rgb(0xf5, 0x9e, 0x0b),
                                ToastKind::Error => egui::Color32::from_rgb(0xff, 0x4d, 0x4f),
                            };
                            egui::Frame::group(ui.style())
                                .inner_margin(egui::Margin::symmetric(12, 8))
                                .corner_radius(egui::CornerRadius::same(10))
                                .stroke(egui::Stroke::new(1.0, color))
                                .show(ui, |ui| {
                                    ui.colored_label(color, &t.message);
                                });
                            ui.add_space(4.0);
                        }
                    });
                });
            // 有 toast 时强制重绘以触发淘汰
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}

/// 尝试在系统层面找一份 CJK 字体灌进 egui。失败就保持默认(显示豆腐块,
/// W7 用 bundled font 解决)。
fn try_install_system_cjk_font(ctx: &egui::Context) {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
        ]
    } else if cfg!(target_os = "windows") {
        &[
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyh.ttf",
            "C:\\Windows\\Fonts\\simhei.ttf",
        ]
    } else {
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/arphic/uming.ttc",
        ]
    };

    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut data = egui::FontData::from_owned(bytes);
            data.tweak.scale = 1.0;
            data.index = 0;
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert("system_cjk".into(), data.into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "system_cjk".into());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("system_cjk".into());
            ctx.set_fonts(fonts);
            return;
        }
    }
}
