//! 主题系统:7 主题(default/green/orange/gray/dark/white)+ dark 内置变体。
//!
//! 颜色字面量从 `frontend/css/style.css` 17 个 CSS `--xxx` 变量逐字搬过来。
//! W2 起步只实现 **default + dark** 两套色板加一个切换通路,W3 把剩余 5 套
//! 填充并做 A/B 截图给用户审(决策点 W2-A)。

use eframe::egui::{self, Color32, CornerRadius, Shadow, Stroke};

#[derive(Copy, Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ThemeName {
    Default,
    Green,
    Orange,
    Gray,
    Dark,
    White,
}

impl ThemeName {
    pub const ALL: &'static [Self] = &[
        Self::Default,
        Self::Green,
        Self::Orange,
        Self::Gray,
        Self::Dark,
        Self::White,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Green => "Green",
            Self::Orange => "Orange",
            Self::Gray => "Gray",
            Self::Dark => "Dark",
            Self::White => "White",
        }
    }
    pub fn palette(self) -> Palette {
        match self {
            Self::Default => Palette::DEFAULT,
            Self::Green => Palette::GREEN,
            Self::Orange => Palette::ORANGE,
            Self::Gray => Palette::GRAY,
            Self::Dark => Palette::DARK,
            Self::White => Palette::WHITE,
        }
    }
}
impl Default for ThemeName {
    fn default() -> Self {
        Self::Default
    }
}

/// 17 字段映射 style.css 17 个 `--xxx` 变量。
///
/// `muted` / `success` / `success_soft` 字段当前 apply() 不直接消费(egui Visuals
/// 没有完全对应的位置),保留供 W7+ 局部组件(toast 边框 / 状态徽章 /
/// 弱化文字)按需读。
#[derive(Copy, Clone, Debug)]
#[allow(dead_code)]
pub struct Palette {
    pub app_bg: Color32,
    pub surface: Color32,
    pub soft_surface: Color32,
    pub line: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub primary: Color32,
    pub primary_soft: Color32,
    pub success: Color32,
    pub success_soft: Color32,
    pub danger: Color32,
    pub warning: Color32,
    pub shadow_alpha: u8, // 控制 shadow 强度,简化 CSS box-shadow 一个数值
    pub radius: f32,
    pub is_dark: bool,
}

impl Palette {
    // W3:6 主题色板逐字搬自 style.css `[data-theme-palette="*"]` 块
    // (第 1751-1853 行)。light 系列 5 个(default/green/orange/gray/white)
    // 共享白色背景,只变 primary / primary-soft;dark 单独整套深色。

    /// `[data-theme-palette="default"]` 默认蓝
    pub const DEFAULT: Self = Self {
        app_bg: rgb(0xff, 0xff, 0xff),
        surface: rgb(0xff, 0xff, 0xff),
        soft_surface: rgb(0xf7, 0xf8, 0xfb),
        line: rgb(0xe4, 0xe7, 0xec),
        text: rgb(0x11, 0x18, 0x27),
        muted: rgb(0x66, 0x70, 0x85),
        primary: rgb(0x14, 0x76, 0xff),
        primary_soft: rgb(0xee, 0xf6, 0xff),
        success: rgb(0x16, 0xa3, 0x4a),
        success_soft: rgb(0xec, 0xfd, 0xf3),
        danger: rgb(0xff, 0x4d, 0x4f),
        warning: rgb(0xf5, 0x9e, 0x0b),
        shadow_alpha: 0,
        radius: 14.0,
        is_dark: false,
    };

    /// `[data-theme-palette="green"]`
    pub const GREEN: Self = Self {
        primary: rgb(0x1f, 0x9d, 0x55),
        primary_soft: rgb(0xe4, 0xf6, 0xea),
        ..Self::DEFAULT
    };

    /// `[data-theme-palette="orange"]`
    pub const ORANGE: Self = Self {
        primary: rgb(0xf9, 0x73, 0x16),
        primary_soft: rgb(0xff, 0xf0, 0xe4),
        ..Self::DEFAULT
    };

    /// `[data-theme-palette="gray"]`
    pub const GRAY: Self = Self {
        primary: rgb(0x64, 0x74, 0x8b),
        primary_soft: rgb(0xe8, 0xed, 0xf3),
        ..Self::DEFAULT
    };

    /// `[data-theme-palette="white"]` 极简灰主操作色
    pub const WHITE: Self = Self {
        primary: rgb(0x94, 0xa3, 0xb8),
        primary_soft: rgb(0xf1, 0xf5, 0xf9),
        ..Self::DEFAULT
    };

    /// `[data-theme-palette="dark"]`
    pub const DARK: Self = Self {
        app_bg: rgb(0x11, 0x13, 0x18),
        surface: rgb(0x17, 0x1a, 0x21),
        soft_surface: rgb(0x20, 0x24, 0x2d),
        line: rgb(0x2d, 0x34, 0x40),
        text: rgb(0xf8, 0xfa, 0xfc),
        muted: rgb(0xaa, 0xb2, 0xc0),
        primary: rgb(0x60, 0xa5, 0xfa),
        primary_soft: Color32::from_rgba_premultiplied(96, 165, 250, 46),
        success: rgb(0x34, 0xd3, 0x99),
        success_soft: Color32::from_rgba_premultiplied(52, 211, 153, 41),
        danger: rgb(0xfb, 0x71, 0x85),
        warning: rgb(0xfb, 0xbf, 0x24),
        shadow_alpha: 0,
        radius: 14.0,
        is_dark: true,
    };
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// 把 Palette 应用到 egui::Style/Visuals。
pub fn apply(ctx: &egui::Context, p: &Palette) {
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;
    v.dark_mode = p.is_dark;
    v.window_fill = p.app_bg;
    v.panel_fill = p.app_bg;
    v.faint_bg_color = p.soft_surface;
    v.extreme_bg_color = p.surface;
    v.code_bg_color = p.soft_surface;
    v.override_text_color = Some(p.text);
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_rgba_premultiplied(0, 0, 0, p.shadow_alpha),
    };
    let r = p.radius.round().clamp(0.0, 255.0) as u8;
    v.window_corner_radius = CornerRadius::same(r);
    v.menu_corner_radius = CornerRadius::same((r as f32 * 0.66).round() as u8);

    // 主操作色 → primary
    v.selection.bg_fill = p.primary;
    v.selection.stroke = Stroke::new(1.0, p.primary);
    v.hyperlink_color = p.primary;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;

    // 按钮 / 控件背景层
    v.widgets.noninteractive.bg_fill = p.surface;
    v.widgets.noninteractive.weak_bg_fill = p.soft_surface;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    v.widgets.inactive.bg_fill = p.soft_surface;
    v.widgets.inactive.weak_bg_fill = p.soft_surface;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.line);
    v.widgets.hovered.bg_fill = p.primary_soft;
    v.widgets.hovered.weak_bg_fill = p.primary_soft;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.primary);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.primary);
    v.widgets.active.bg_fill = p.primary;
    v.widgets.active.weak_bg_fill = p.primary;
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.primary);

    ctx.set_style(style);
}
