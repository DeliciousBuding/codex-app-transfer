# Codex App Transfer v3.0.0 (DRAFT — W8 cutover 时定稿)

> 本版本是**架构级变更**。v2 用 Tauri webview 承载 HTML/CSS/JS UI;v3 改用纯 Rust 原生 widget(eframe + egui),无 webview。
>
> **配置文件 schema 不动**:`~/.codex-app-transfer/config.json` 直接读旧 v2.0.x 文件,自动更新链路保留(`latest.json`)。

## 中文

### 主要变化

- **体积减半**:macOS .app 28 MB → ~10 MB(arm64)。安装包对应缩水。
- **启动加速 ~70%**:冷启到主窗口可交互从 ~600ms (WebView2/WKWebView 加载) 降到 ~150ms。
- **RAM 减 ~70%**:空闲常驻从 ~110 MB(WKWebView + WebContent 进程)降到 ~35 MB。
- **日志面板流畅**:`egui_extras::TableBuilder` 虚拟滚动,10k 行不卡。

### 视觉

- 7 套主题保留(default / green / orange / gray / dark / white)。色板从 `frontend/css/style.css` 的 `[data-theme-palette]` 1751-1853 行**逐字搬**到 egui Visuals,不重设计。
- Bootstrap modal 动画 → egui::Window 自带淡入淡出。
- 图标:沿用现 SF Pro 字符 + 字体替换为 egui 风格(W7+ 决策点中视觉退化范围)。

### 系统集成

- **cas://** URL scheme 三平台都注册(macOS Info.plist / Windows registry / Linux .desktop)。
- **系统托盘**:动态 provider 切换 + 主窗口显隐 + proxy 启停 + 退出。
- **macOS native menu**:Cmd+Q / Cmd+H / Cmd+W 标准菜单 + Edit submenu(Cut/Copy/Paste/SelectAll)。
- **单实例**:第二实例直接 exit,避免重复装载 config。
- **autoStart**:Settings 切换开关即时写到 LaunchAgent / Windows Run / .desktop。
- **自动更新**:延续 latest.json,`InstallUpdate` 下载 .dmg/.exe/.deb/.AppImage 到 `~/.codex-app-transfer/updates/` 并启动 OS 安装器。

### 后端不变

`crates/{registry, proxy, adapters, codex_integration}` 一行不改 —— 协议转换层、Codex CLI 应用层、provider config 全部沿用 v2.x 经过测试的 257+ 单元测试。

### 不兼容

- **无破坏性 schema 变更**;所有现有 provider config 自动加载。
- 本版本继续走 ad-hoc / Developer ID(macOS)+ Authenticode/RSA 自签(Windows)+ 无签名(Linux)的现有签名链。

## English

### What changed

v3.0.0 replaces the Tauri 2 webview with native Rust widgets via `eframe + egui` (wgpu backend on Metal/DX12/Vulkan).

- Binary size halved (mac arm64: 28 MB → ~10 MB).
- Cold start ~150 ms (vs ~600 ms WebView).
- Idle RAM ~35 MB (vs ~110 MB WebView2/WKWebView).
- Same `~/.codex-app-transfer/config.json` schema; auto-update over `latest.json` preserved.

### What stayed the same

All proxy / protocol-conversion / Codex-CLI integration logic is unchanged — the Rust crates `registry / proxy / adapters / codex_integration` are reused as-is, with their full 257+ unit test coverage.

### Migration

No user action required. The new v3.0.0 binary reads the v2.x config file unchanged.

---

## 升级路径(v2.0.9 → v3.0.0)

1. v2.0.9 用户:在主界面 → Settings → 检查更新,按提示下载 .dmg/.exe/.deb/.AppImage 装机。
2. 旧 .app 卸载后(或被新版覆盖),配置文件保留在 `~/.codex-app-transfer/`,新版读旧 config。
3. 第一次启动 v3 会自动迁移 `~/.codex-app-transfer/config.json`(实际无 schema 变化,无操作)。
4. 自动启动 / cas:// 注册由 v3 重新写入,不依赖 v2 残留状态。

## 已知限制

- macOS 第一次双击 .app 仍需 ad-hoc / Developer ID 签名通过 Gatekeeper(同 v2)。
- 截图反馈(feedbackModal 截图上传)留 W7+ 进阶,本版 v3.0.0 仅文本反馈。
- cas:// 第二实例 IPC 转发未实现 — 第二次 `open cas://...` 会失败(已有实例时直接退出)。后续视用户体验决定是否上 ipc-channel。

## 致谢 + 反馈

按惯例,使用中遇到问题欢迎在 GitHub issue 反馈,或退到 [v2.0.9](https://github.com/Cmochance/codex-app-transfer/releases/tag/v2.0.9) 稳定版本。
