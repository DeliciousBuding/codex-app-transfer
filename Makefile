# Phase 2 起 release pipeline 全部走 GitHub Actions (.github/workflows/release.yml)。
# 本地 Makefile 只保留两个 target:
#   mac-app  - 本地自测出 .app
#   clean    - 清理 build/dist/release/.tmp
# 三平台 release 触发: gh workflow run release.yml -f version=2.0.1
# 详见 docs/build.md。

.PHONY: help mac-app mac-app-egui clean

help:
	@echo "Targets:"
	@echo "  mac-app       Build Tauri v2.0.x unsigned .app into dist/mac/ (W8 删 Tauri 后将退役)"
	@echo "  mac-app-egui  Build v3.0.0-pre eframe/egui .app + .dmg via cargo-packager (W7.2+)"
	@echo "  clean         Remove build/, dist/, release/, .release-signing/, .tmp/"
	@echo ""
	@echo "Release: 三平台 release 由 GitHub Actions 出, 不再走本地 Makefile."
	@echo "         手动触发: gh workflow run release.yml -f version=<x.y.z>"
	@echo "         tag 触发: git tag v<x.y.z> && git push --tags"

mac-app:
	cargo tauri build --bundles app
	mkdir -p dist/mac
	rm -rf "dist/mac/Codex App Transfer.app"
	cp -R "target/release/bundle/macos/Codex App Transfer.app" "dist/mac/Codex App Transfer.app"
	@echo ""
	@echo "✓ Built: dist/mac/Codex App Transfer.app"

# W7.2+:cargo-packager 替代 cargo tauri,出 v3.0.0-pre .app + .dmg
# 需要先 `cargo install cargo-packager --locked --version 0.11.8`
# 签名留 W7.5 走 release.yml(本地 build 仅 adhoc 签)
mac-app-egui:
	cd crates/desktop_app && cargo packager --release -f app -f dmg
	mkdir -p dist/mac-egui
	rm -rf "dist/mac-egui/Codex App Transfer.app"
	cp -R "target/release/Codex App Transfer.app" "dist/mac-egui/Codex App Transfer.app"
	cp "target/release/Codex App Transfer_3.0.0-pre_aarch64.dmg" dist/mac-egui/ 2>/dev/null || true
	@echo ""
	@echo "✓ Built: dist/mac-egui/Codex App Transfer.app + .dmg"

clean:
	rm -rf build dist release .release-signing .tmp
