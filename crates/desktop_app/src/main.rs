//! Codex App Transfer 桌面 v3 — eframe + egui 原生主程序。
//!
//! W2 起步阶段:窗口 1024×700,7 页 placeholder,主题/语言切换。
//! W3+ 逐页填充功能。
//! W6.2 加 single-instance + cas:// argv 接入 + macOS native menu。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod background;
mod i18n;
mod page;
mod state;
mod system;
mod theme;

use eframe::egui;

fn main() -> eframe::Result<()> {
    // single-instance 锁:已有实例在跑就直接退出。
    // (W6.2 暂不做 IPC URL 转发到第一实例;W6-A 后视测试结果决定是否上 ipc-channel)
    if !system::acquire_single_instance() {
        eprintln!("Codex App Transfer 已在运行,本次启动退出");
        return Ok(());
    }

    let initial_cas = system::cas_url_from_argv();

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 700.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title("Codex App Transfer"),
        ..Default::default()
    };
    eframe::run_native(
        "Codex App Transfer",
        opts,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, initial_cas)))),
    )
}
