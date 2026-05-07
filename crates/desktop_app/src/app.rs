//! eframe::App 主体。W3:接入 AppState(读 ~/.codex-app-transfer/config.json),
//! 主题 / locale 都从 settings 读;Dashboard 与 Settings page 接通真实数据。

use eframe::egui;

use crate::i18n::lookup_owned;
use crate::page::{self, Page};
use crate::state::AppState;
use crate::theme::{self, ThemeName};

pub struct App {
    pub active_page: Page,
    pub state: AppState,
    last_applied_theme: Option<ThemeName>,
    last_applied_locale: Option<crate::i18n::Locale>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        try_install_system_cjk_font(&cc.egui_ctx);
        Self {
            active_page: Page::Dashboard,
            state: AppState::load(),
            last_applied_theme: None,
            last_applied_locale: None,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 周期性 reload config.json(检测外部修改)
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
                        // W6 wire:打开还原确认 modal
                        let _ = ui.button(format!(
                            "↺ {}",
                            lookup_owned(cur_locale, "dashboard.clearDesktopConfig")
                        ));
                        // W6 wire:打开反馈 modal
                        let _ = ui.button(format!(
                            "💬 {}",
                            lookup_owned(cur_locale, "dashboard.feedback")
                        ));
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
            page::render(ui, self.active_page, &mut self.state);
        });
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
