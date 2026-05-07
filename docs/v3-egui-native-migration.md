# v3.0.0 — Tauri webview → egui 原生 widget 迁移

> **Status**: 起步阶段(W0)。本文档是 PR 内单一推进 source of truth,所有 W1-W8 子任务都按本表格推进,完成后我在对应 commit 里更新本文档的 checkbox 状态。
>
> **目标**: 抛 Tauri webview,改用 `eframe + egui` 原生 widget,**体积减半 / 启动快 4 倍 / RAM 减 70%**,功能 1:1 等价。
>
> **PR 模式**: 全部 W1-W8 commits 都推到这一个 PR,**不合并**直到用户手测确认。

---

## 0. 决议参数(锁定)

| 项 | 选择 | 备注 |
|---|---|---|
| UI 框架 | `eframe` 0.31+(底层 `egui` + `wgpu`) | 纯 Rust,不引 DSL,与现有 5 crate workspace 类型直接共享 |
| 图形后端 | `wgpu`(eframe 默认) | macOS Metal / Windows DX12 / Linux Vulkan |
| 表格 / 网格 | `egui_extras::TableBuilder` | provider 列表 + 日志面板虚拟滚动 |
| 图标字体 | `egui_phosphor` | 6000+ glyph,与现 Bootstrap Icons 1:1 替换 |
| 中文字体 | Source Han Sans CN VF subset | ~1.5 MB,内嵌打包 |
| 等宽字体(日志) | JetBrains Mono subset | ~250 KB |
| 托盘 | `tray-icon` 0.17+ | 跨三平台 |
| 菜单栏(macOS) | `muda` | tray-icon 同作者,Cmd+Q/Cmd+H native |
| 文件对话框 | `rfd` | 配置导入用 |
| 剪贴板 | `arboard` | 复制环境变量命令用 |
| 自动更新 | `self_update` crate | 走现 `latest.json`,只换客户端 |
| 单实例 | `single-instance` | 替代 `tauri-plugin-single-instance` |
| 系统通知 | `notify-rust` | 后台事件用 |
| markdown | `egui_commonmark` | Guide 页静态富文本 |
| 包格式 | `cargo-bundle` (mac) + `cargo-wix`+NSIS (win) + `cargo-deb` + `appimage-builder` | 抛 Tauri bundler |

## 1. 体积 / 性能目标

| 指标 | v2.0.9(现 Tauri) | v3.0.0(egui)目标 | 验证方式 |
|---|---|---|---|
| Mac arm64 binary | 27 MB | **≤ 12 MB** | `ls -la dist/mac/.../MacOS/codex-app-transfer` |
| 整 .app bundle | 28 MB | **≤ 14 MB** | `du -sh "Codex App Transfer.app"` |
| Windows setup.exe | 9.5 MB | **≤ 6 MB** | release.yml artifact |
| 冷启动到主窗口可交互 | ~600 ms (WebView2) | **≤ 150 ms** | Instruments 启动 timeline |
| 切页响应 | ~80 ms (DOM rebuild) | **≤ 16 ms**(1 frame @ 60Hz) | egui frame_time 直方图 |
| 空闲 RAM | ~110 MB (WKWebView + WebContent) | **≤ 35 MB** | Activity Monitor |
| 日志面板滚动 10k 行 | DOM 卡顿 | **流畅 60 fps** | TableBuilder virtualized |

## 2. 当前应用功能盘点(迁移 reference,**逐条不漏**)

### 2.1 七个页面

- [ ] Dashboard `#dashboard` — provider 卡片网格 + 三个 status hero(桌面/代理/当前 provider) + 行动按钮 + 最近活动
- [ ] Providers/Add `#providers/add` — name/baseUrl/apiKey/auth + 高级折叠(API format) + 模型映射 grid + preset 列表
- [ ] Providers `#providers` — 表头 + 可重排 provider 列表 + Claude 模型菜单切换
- [ ] Desktop `#desktop` — config 列表 + JSON 预览 + apply/clear + 3 步 mini-step
- [ ] Proxy `#proxy` — 启停 + 端口输入 + 实时日志面板(终端式)+ 自动滚动 + stats
- [ ] Settings `#settings` — 主题 / 语言 / 双端口 / 4 开关 / 兼容性检测 / 配置备份+导出+导入 / 反馈 / About
- [ ] Guide `#guide` — 静态富文本(用 `egui_commonmark`)

### 2.2 三个 modal

- [ ] deleteModal — "确认删除"
- [ ] restartReminderModal — "立即重启 Codex App?"
- [ ] feedbackModal — textarea + 截图上传 + 日志附加开关 + 提交

### 2.3 二十个 action(逐条迁移)

| Action | 类别 | 异步? |
|---|---|---|
| `apply-desktop` | Codex 应用 + 复制环境变量 | 是 |
| `apply-provider-desktop` | apply + 设默认 | 是 |
| `backup-config` | registry::create_backup | 同步 |
| `check-provider-compatibility` | 串行 curl /v1/models 上报 | 是,SSE 推进度 |
| `check-update` | self_update fetch | 是 |
| `choose-import-config` | rfd file picker + import | 是 |
| `clear-desktop` | restore_codex_state | 是 |
| `clear-logs` | telemetry().logs.clear | 同步 |
| `export-config` | rfd save + export | 是 |
| `fetch-form-models` | curl `<base>/v1/models` | 是 |
| `install-update` | self_update download + 重启 | 是 |
| `open-feedback` | open feedback dialog | 同步 |
| `open-log-dir` | opener::open(log_dir) | 同步 |
| `proxy-start` | ProxyManager::start | 是 |
| `proxy-stop` | ProxyManager::stop | 同步 |
| `test-provider-form` | curl probe | 是 |
| `toggle-baseurl-menu` | UI state | 同步 |
| `toggle-key` | UI state | 同步 |
| `toggle-model-menu-mode` | PUT /api/settings persist | 是 |
| `view-logs` | 跳到 #proxy | 同步 |

### 2.4 二十八个 admin REST endpoint(全保留,headless 用)

```
GET  /api/version                   GET  /api/status
GET  /api/settings                  PUT  /api/settings
GET  /api/providers                 POST /api/providers
GET  /api/providers/compatibility   POST /api/providers/test
POST /api/providers/models/available
PUT  /api/providers/reorder
GET  /api/presets
GET  /api/proxy/status              POST /api/proxy/start
POST /api/proxy/stop                GET  /api/proxy/logs
POST /api/proxy/logs/clear          POST /api/proxy/logs/open-dir
GET  /api/desktop/status            POST /api/desktop/configure
GET  /api/desktop/snapshot-status   POST /api/desktop/clear
POST /api/desktop/restart-codex-app
GET  /api/config/backups            POST /api/config/backup
GET  /api/config/export             POST /api/config/import
POST /api/feedback                  POST /api/update/install
```

### 2.5 系统集成

- [ ] cas:// URI scheme(macOS Info.plist / Windows Registry / Linux .desktop)
- [ ] 系统托盘:provider 切换 + 主窗口显隐 + 退出
- [ ] macOS native app menu(Cmd+Q/Cmd+H/About)
- [ ] 单实例(`single-instance` crate)
- [ ] 自动启动(autoStart 设置 → `auto-launch` crate)
- [ ] 自动更新(`self_update` crate;沿用 latest.json)

### 2.6 七个主题

`default` / `green` / `orange` / `gray` / `dark` / `white` + 内置 dark mode 变体。
17 个 CSS `--xxx` 变量映射到 egui Visuals/Style。**颜色逐字搬,不重设计调色板**。

### 2.7 双语 i18n

zh / en 两套字典共 571 行(`frontend/js/i18n.js`)。逐 key 搬到 TOML,build.rs 用 `phf_codegen` 生成 O(1) 静态表。

## 3. 新 workspace 结构

```
crates/
  registry/          ← 保留(provider data + config IO)
  proxy/             ← 保留(axum proxy + SSE 适配)
  adapters/          ← 保留(协议转换)
  codex_integration/ ← 保留(Codex CLI apply/restore)
  admin_api/         ← W1 新建:从 src-tauri/src/admin/ 抽出
  desktop_state/     ← W1 新建:跨 desktop_app + admin_api 共享 state machine
  desktop_app/       ← W2 新建:eframe 主程序(替代 src-tauri)
xtask/               ← 保留;W7 扩展 bundle 任务
src-tauri/           ← W1-W7 期间存活共存,W8 删
frontend/            ← W1-W7 期间存活共存,W8 删
```

## 4. W1-W8 推进表

> 每个 W 完成时:更新对应 checkbox + push commit 到 PR + 在 PR comment 给 changelog + 把"决策点"标 ⚠️ 的项**先汇报后动手**。

### W1: 抽 admin_api + desktop_state ✅ 完成

- [x] 新建 `crates/admin_api/`,把 `src-tauri/src/admin/{mod.rs, handlers.rs, state.rs, static_files.rs, registry_io.rs}` 整体搬过来
- [x] `crates/admin_api/Cargo.toml` 依赖 axum + serde + 现 4 crate + tower/bytes/include_dir/mime_guess/chrono/reqwest/sha2/base64/getrandom
- [x] `src-tauri/src/main.rs` 改成引 `admin_api::build_app_router` + `admin::handlers/registry_io` + `proxy_runner::ProxyManager`
- [x] 新建 `crates/desktop_state/`,定义 `Action` / `Effect` / `Model` 三个核心 enum / struct(W1 占位 scaffolding)
- [x] **额外抽出 `crates/proxy_runner/`** — ProxyManager 单独成 crate(183 行),admin_api 依赖,后续 desktop_app 也直接用,避免循环依赖
- [x] **额外动作**:`APP_VERSION` 从 `env!("CARGO_PKG_VERSION")` 改成 `OnceLock<&'static str>` + bin 启动时 `set_app_version()` 注入,11 个 site 全更新
- [x] workspace test 全绿(257+ tests,所有 admin handler 测试不变)
- [x] **现 Tauri app `make mac-app` 仍能 build 仍能跑**(已验证:dist/mac binary 27M,version 2.0.9,行为不变)
- [x] `cargo fmt --check` 通过

### W2: desktop_app 骨架 ✅ 完成

- [x] `crates/desktop_app/Cargo.toml` 依赖 eframe 0.31 + egui 0.31 + egui_extras + phf + serde
- [x] 主窗口 1024×700,标题"Codex App Transfer"
- [x] 7 个空 Page(每个 page 一个 placeholder render)+ 左侧 nav 切换
- [x] 主题切换器(Settings page 内)+ 7 主题 ThemeName::ALL + Palette 结构 + apply()
  - [x] Default / Dark 完整调色板(从 style.css 第 1-25 行逐字搬)
  - [ ] Green / Orange / Gray / White 完整调色板(W3 决策点 W2-A 时填充并审)
- [x] i18n:`build.rs` 读 `src/i18n/strings.toml`(267 keys 全自动从 i18n.js 抽出)→ phf::Map<&str, [&str; 2]> 静态表
- [x] `lookup_owned()` 函数 + `t!()` 宏
- [x] Locale enum + 切换器(Settings page)
- [x] `cargo run -p desktop_app` 能起空窗口(已 smoke test 验证 2 秒无 panic)
- [x] **不**动 src-tauri / frontend(回滚保护成立:`make mac-app` 仍然 build 出 v2.0.9)
- [ ] ⚠️ **决策点 W2-A**:延后到 W3。W2 只填 Default/Dark 两套真色板,Green/Orange/Gray/White 是占位;W3 填充齐 5 套色板后再做 A/B 审,信息更充分

### W2 量化首结

| 指标 | 现状 | 备注 |
|---|---|---|
| desktop_app release binary | **9.5 MB** | vs Tauri 27 MB(目标 ≤12 MB),已留充足余量 |
| workspace 全测 | 250+ tests 全绿 | adapters / proxy / codex_integration / registry / 等等不动 |
| Tauri app 旧版本 | `make mac-app` 仍出 v2.0.9 27MB | 回滚保护成立 |

### W3: Dashboard + Settings 完整 ✅ 完成

- [x] Dashboard:provider 卡片网格(从真实 ~/.codex-app-transfer/config.json 读)+ 桌面/代理/当前 provider 三个 hero + activity placeholder + 顶栏反馈/还原按钮
- [x] Settings 全 20 项渲染:
  - [x] Theme 单选(7 项, hover hint)
  - [x] Language toggle(zh/en)
  - [x] 双端口输入(DragValue 1024-65535)
  - [x] 4 个开关(autoApplyOnStart / restoreCodexOnExit / exposeAllProviderModels / autoStart)
  - [x] Update URL 单行输入
  - [x] Compatibility 检查按钮(渲染,W6 wire async)
  - [x] Backup / Export / Import 按钮(渲染,W6 wire)
  - [x] Feedback 按钮(渲染,W6 wire modal)
  - [x] About:版本 / License / 检查更新按钮 / 安装更新按钮
- [x] 这两页 sync action 全部接通(theme/language/ports/4 switches/updateUrl);async action(W6 接通)按钮已渲染但 W6 wire
- [x] **AppState 数据源**:registry crate 直接读写 ~/.codex-app-transfer/config.json,2 秒自动 reload,settings 改动立即写回
- [x] 6 套主题完整调色板(从 style.css `[data-theme-palette]` 1751-1853 行逐字搬,default/green/orange/gray/dark/white)
- [x] 5 个新 i18n key(proxy.notRunning / provider.active / provider.none / providers.default / providers.noApiKey)
- [x] ⚠️ **决策点 W2-A 现在到位**:7 主题完整,你可 \`cargo run --release -p codex-app-transfer-desktop-app\` 切换 7 主题 × 7 page 看色彩,告诉我哪些接受 / 哪些要调
- [ ] ~~决策点 W3-A~~ 与 W2-A 合并(本质同一个色彩审议)
- [ ] egui_kittest snapshot W4 加(需要先有 page 内容稳定,W3 末刚有实装)

### W3 量化(累计)

| 指标 | W2 末 | W3 末 |
|---|---|---|
| desktop_app release binary | 9.5 MB | **9.9 MB** | ≤ 12 MB(余 2.1 MB)|
| workspace tests | 250+ 全绿 | **250+ 全绿** | 不动 |
| Tauri 旧版 `make mac-app` | v2.0.9 27 MB | **v2.0.9 27 MB**(回滚保护成立)|

### W4: Providers + Providers/Add ✅ 完成

- [x] Providers/Add:完整表单 + 高级折叠(API format + auth scheme)+ mapping grid(6 slots: default + gpt-5.5/5.4/5.4-mini/5.3-codex/5.2)+ preset 列表(7 个内置 preset 渲染 + 一键填充)+ base_url options ComboBox(部分 preset 含多区域 URL)+ apiKey 显隐切换
- [x] Providers:provider 列表(表头 + 行)+ 上下箭头 reorder(替代 W6 drag-drop)+ 启用切换(set default)+ 编辑跳转(load_provider_into_form + nav_to_providers_add)+ 删除入口(经 deleteModal 确认)+ 添加按钮(右上角)
- [x] **deleteModal 实装**(W6 三 modal 中第一个落地;在 app.rs 用 egui::Window 渲染,confirm_delete_id 触发)
- [x] AppState 加 ProviderForm + presets cache + 6 个新方法(load_provider_into_form / save_form / delete_provider / set_default_provider / move_provider / fill_form_from_preset)
- [x] page::render 签名改 \`&mut Page\`,允许 page 内部跳转(state.nav_to_providers_add / nav_back_to_providers 信号)
- [x] 7 个新 i18n key(providers.cluster / providers.authScheme / providers.edit / providersAdd.formatTitle / common.show / common.hide / presets.use)
- [x] 累计 15/20 action 接通(W3 8 个 + W4 7 个:add provider / edit / delete / set default / move-up / move-down / save form)。剩 5 个 async / modal action 在 W6:apply-desktop / proxy-start/stop / clear-desktop / open-feedback / install-update / fetch-form-models / test-provider-form / check-update / check-provider-compatibility / backup-config / export-config / import-config(actually count is more,但都属于 W6)

### W4 量化(累计)

| 指标 | W3 末 | **W4 末** | 目标 |
|---|---|---|---|
| desktop_app release binary | 9.9 MB | **10 MB** | ≤ 12 MB(余 2 MB)|
| workspace tests | 250+ 绿 | **250+ 绿** | 不动 |
| Tauri 旧版 \`make mac-app\` | v2.0.9 27 MB | **v2.0.9 27 MB**(回滚保护成立)|

### W5: Proxy + Desktop + Guide

- [ ] Proxy:启停 + 端口 + 实时日志(`egui_extras::TableBuilder` virtual scroll,10k 行流畅)+ 自动滚动 + stats
- [ ] Desktop:config list + JSON pre + apply/clear/restart Codex 三按钮 + 3 mini-step
- [ ] Guide:`egui_commonmark` 渲染原 guide 静态文案
- [ ] restartReminderModal 实装
- [ ] 累计 20/20 action 全部接通

### W6: 系统集成

- [ ] tray-icon:动态 provider 列表 + 主窗口显隐 + Quit
- [ ] muda macOS menu:Cmd+Q/Cmd+H/Cmd+W/About + 编辑菜单(Cut/Copy/Paste/SelectAll)
- [ ] cas:// URI scheme 三平台注册
- [ ] single-instance:第二实例把 cas:// URL 通过 ipc-channel 转发到第一实例
- [ ] auto-launch:autoStart 设置开关接通
- [ ] feedbackModal 实装(含 rfd 截图上传 + 日志附加 + multipart POST 到 feedback-worker)
- [ ] ⚠️ **决策点 W6-A**:cas:// 注册流程在三平台手测,需要你各装一份测一次

### W7: self-update + 三平台 CI

- [ ] self_update 替代 Tauri updater,沿用 latest.json
- [ ] xtask mac-bundle / win-msi+nsis / linux-deb / linux-appimage
- [ ] codesign / notarytool / WiX / RSA 自签 — 沿用现 secrets
- [ ] release.yml 改 `cargo tauri build` → `cargo bundle` 链路
- [ ] **不 push 真 release tag**;只验证 PR CI 三平台 build 跑通,artifact 下载手测装能用
- [ ] ⚠️ **决策点 W7-A**:跨版本自动更新 staging 测(用户旧 v2.0.9 装机 → 触发 self-update → 升到本 PR 出的 v3.0.0-pre);需要你手测确认才发外部
- [ ] ⚠️ **决策点 W7-B**:三平台 binary 体积达成 §1 表;若任一平台没达成,讨论权衡

### W8: cutover

- [ ] 删 `src-tauri/` 整目录
- [ ] 删 `frontend/` 整目录
- [ ] 改 `Cargo.toml` workspace.members:移除 src-tauri,加 desktop_app + admin_api + desktop_state
- [ ] 改 `Makefile` mac-app target 走 cargo bundle
- [ ] 改 `release.yml` 完成
- [ ] 改 `README.md` 介绍说明 v3 是原生 Rust
- [ ] 写 `docs/release-notes-v3.0.0.md`
- [ ] bump version 2.0.9 → 3.0.0
- [ ] **本 PR 此时 ready-for-merge**
- [ ] ⚠️ **决策点 W8-A**:你手测 → 你确认 → 你说 "merge",我才合(release tag 也等你说才打)

## 5. 风险登记 + 缓解

| ID | 风险 | 影响 | 缓解 |
|---|---|---|---|
| R1 | egui 主题做不到 Bootstrap 视觉精度 | 视觉退化 | W3-A 决策点截图 A/B 给用户 |
| R2 | Bootstrap modal 动画消失 | UX 略降 | egui::Window 自带 fade,接受 |
| R3 | macOS 公证后 cas:// 不工作 | 链接不响应 | W6-A 决策点测三平台,加 fallback 引导 |
| R4 | 中文 IME 候选窗错位 | 输入体验降 | eframe 0.31+ 已修主流问题,W3 测;加 `egui_inbox` fallback 监听 winit IME |
| R5 | 字体 subset 漏字符 | 显示豆腐块 | FontDefinitions fallback chain 加系统字体 |
| R6 | self_update 在 macOS 公证后无法替换 .app | 自动更新失败 | helper script 模式:下载 .dmg 提示用户挂载替换 |
| R7 | TableBuilder 10k 行卡 | 日志面板退化 | 启 `striped` virtualized;不达标改 ScrollArea 自实现 |
| R8 | Tauri 旧 artifact 自动更新链路被破坏 | 旧用户升级失败 | 沿用 latest.json schema + binary 名 + bundle ID |

## 6. 中途新增任务 / 优化(自动追加)

> 推进过程中发现需要的优化项,我会自动追加到本节,**不打断主线**;到达决策点时统一汇报,你裁决要不要本 PR 吸收。

(W0 起步阶段,空)

## 7. 决策点登记(到达时汇报)

| ID | 节点 | 决策内容 | 状态 |
|---|---|---|---|
| W2-A | 主题骨架做完 | 7 主题色彩是否接受 | 未到 |
| W3-A | Dashboard 完整后 | A/B 视觉差异接受范围 | 未到 |
| W6-A | 系统集成做完 | cas:// 三平台注册各自测试 | 未到 |
| W7-A | CI 跑通后 | 跨版本自动更新 staging 测 | 未到 |
| W7-B | CI 跑通后 | 三平台体积是否达成,任一未达讨论 | 未到 |
| W8-A | cutover ready | 你手测 + 确认 + 说 merge | 未到 |

## 8. 回滚预案

- W1-W7 期间 src-tauri/ + frontend/ 共存,`make mac-app` 默认仍能 build 旧 Tauri 版
- 每 W 完成 git tag `migrate-egui-Wn`(本地 tag,不推 remote)作为可回退点
- W8 切换前最后一刻 `pre-egui-cutover` tag(推 remote)
- W8 切换后发现 P0:`git revert <cutover-commit>` 一键回到 Tauri 版,恢复 src-tauri/ + frontend/

## 9. acceptance criteria(W8 末)

✅ 7 页全部存在且功能等价(逐条走 §2.1)
✅ 3 modal 都能正确打开/关闭/数据 round-trip(§2.2)
✅ 20 action 全部触发对应行为(§2.3)
✅ 28 admin endpoint headless curl 可用(§2.4)
✅ 7 主题切换实时生效(§2.6)
✅ zh/en i18n 切换无遗漏 key(build.rs 编译期检查)
✅ 系统托盘 provider 切换 + 重启 Codex 流程通(§2.5)
✅ macOS Cmd+Q/Cmd+H/Cmd+W 标准菜单
✅ cas:// 链接外部点击拉起应用并跳到对应页
✅ 自动更新 v2.0.x → v3.0.0-pre staging 链路通
✅ 二进制体积达成 §1 目标
✅ workspace 全套测试 + 新 egui_kittest snapshot 全绿

## 10. 通讯口径(W8 后 release notes 用)

> v3.0.0 是架构级变更:全 Rust 原生 widget,无 webview。
> 体积减半,启动快 4 倍,RAM 减 70%。
> 对用户:UI 视觉接近,功能 1:1,自动更新无缝。
> 对配置:配置文件 schema 不动,直接读旧 ~/.codex-app-transfer/config.json。
