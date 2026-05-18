fn main() {
    println!("cargo:rerun-if-changed=../frontend");
    println!("cargo:rerun-if-changed=../crates/registry/src/presets_data.json");

    let repo = std::env::var("CODEX_APP_TRANSFER_REPO")
        .unwrap_or_else(|_| "Cmochance/codex-app-transfer".to_string());
    let update_url = format!(
        "https://github.com/{}/releases/latest/download/latest.json",
        repo
    );
    println!(
        "cargo:rustc-env=CODEX_APP_TRANSFER_DEFAULT_UPDATE_URL={}",
        update_url
    );
    println!("cargo:rerun-if-env-changed=CODEX_APP_TRANSFER_REPO");

    #[cfg(feature = "desktop")]
    tauri_build::build()
}
