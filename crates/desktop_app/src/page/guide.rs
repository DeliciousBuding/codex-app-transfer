//! Guide page (W5 完整实装).
//! 用 `egui_commonmark` 渲染 markdown 静态文案,zh / en 双版本。

use eframe::egui;
use egui_commonmark::CommonMarkCache;
use std::sync::Mutex;

use crate::i18n::Locale;
use crate::state::AppState;

static MD_CACHE: Mutex<Option<CommonMarkCache>> = Mutex::new(None);

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let locale = state.settings.language;
    let md = match locale {
        Locale::Zh => GUIDE_MD_ZH,
        Locale::En => GUIDE_MD_EN,
    };

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.add_space(12.0);
            let mut guard = MD_CACHE.lock().unwrap();
            let cache = guard.get_or_insert_with(CommonMarkCache::default);
            egui_commonmark::CommonMarkViewer::new()
                .max_image_width(Some(640))
                .show(ui, cache, md);
        });
}

const GUIDE_MD_ZH: &str = r#"
# 使用引导

让 OpenAI Codex CLI 接入 Kimi、DeepSeek、智谱 GLM、阿里云百炼、Xiaomi MiMo 等供应商,无需改动 CLI 本身。

## 开始之前

需要已安装 **OpenAI Codex CLI 0.126+**。终端跑 `codex --version` 看版本号;没装的话先去 [github.com/openai/codex](https://github.com/openai/codex)。

## 快速开始

### 1. 添加提供商

在「提供商」页右上角「+」选预设(Kimi / Kimi Code / DeepSeek / 智谱 GLM / 阿里云百炼 / Xiaomi MiMo),粘贴 API Key。模型映射按官方文档已预填。

### 2. 设为默认

「提供商」列表点你要用的那个,确认带上「默认」标记。可以同时保存多个,以后随时切。

### 3. 应用配置(自动)

应用启动时自动写入 `~/.codex/config.toml` 与 `auth.json`,按需启动本地转发服务。**第一次会先快照备份你原来的 ~/.codex 配置**,退出时按 key 智能合并还原。

### 4. 在终端跑 codex

**打开新终端**,直接跑 `codex`。模型选单会显示当前 provider 的映射。

### 5. 切换 / 退出

右键系统托盘图标可以一键切换 provider。退出应用时,`~/.codex/` 自动回到你原来的配置 —— 不开应用 = 用你自己的原配置。

## 进阶用法

### DeepSeek 思维模式

编辑 DeepSeek provider 时打开「Max 思维」开关,自动按官方 chat/completions 思维协议发送 `reasoning_effort` + `thinking` 字段。

### 兼容性测试

Settings 页「检查兼容性」按钮一键测试所有 provider 的实际响应。

### 配置备份

Settings 页可立即备份当前配置,或导出/导入完整 JSON。备份包含 API Key,只在可信设备上保存。

## 遇到问题

### 提交反馈

Dashboard 顶栏「反馈」按钮匿名提交问题描述 + 截图 + 日志。

### 查看日志

Settings 页底部「查看日志」按钮直接打开 `~/.codex-app-transfer/logs/`。或在「转发」页内置实时日志面板。

### 模型菜单不刷新?

Codex CLI 只在启动时读 `~/.codex/` 配置。切换 provider 后必须**关闭并重新打开终端**。
"#;

const GUIDE_MD_EN: &str = r#"
# Quick Start Guide

Plug Kimi, DeepSeek, Zhipu GLM, Alibaba DashScope, Xiaomi MiMo and others into OpenAI Codex CLI without modifying the CLI itself.

## Prerequisites

Requires **OpenAI Codex CLI 0.126+**. Run `codex --version` to check; if not installed, see [github.com/openai/codex](https://github.com/openai/codex).

## Quick start

### 1. Add a provider

Click "+" in the Providers page, pick a preset (Kimi / Kimi Code / DeepSeek / Zhipu GLM / DashScope / Xiaomi MiMo), paste your API key. Model mappings are pre-filled.

### 2. Set as default

In the provider list, click the one you want, confirm the "Default" tag. Multiple providers can coexist; switch any time.

### 3. Apply config (automatic)

On startup, the app writes `~/.codex/config.toml` and `auth.json` and launches the local forwarder. **The first run snapshots your original ~/.codex config** and key-merge restores it on exit.

### 4. Run codex in a terminal

**Open a new terminal** and run `codex`. The model picker shows the current provider's mapping.

### 5. Switching / quitting

Right-click the tray icon to switch providers. When you quit the app, `~/.codex/` is restored — i.e. with the app closed = your original Codex config.

## Advanced

### DeepSeek thinking mode

When editing a DeepSeek provider, toggle "Max Thinking" to forward `reasoning_effort` + `thinking` fields per the official chat/completions protocol.

### Compatibility check

Settings page → "Check compatibility" tests every provider in one click.

### Config backup

Settings page can instant-backup current config, or export/import full JSON. Backups include API keys; keep them on trusted devices only.

## Troubleshooting

### Submit feedback

The "Feedback" button in the Dashboard top bar submits anonymous reports with text + screenshots + logs.

### View logs

Settings page → "View logs" opens `~/.codex-app-transfer/logs/`. Or use the live log panel in the Proxy page.

### Model menu not refreshing?

Codex CLI only reads `~/.codex/` config at startup. After switching providers you must **close and reopen the terminal**.
"#;
