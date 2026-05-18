//! Codex App Transfer — shared application library.
//!
//! This crate exposes the admin API handlers, proxy runner, telemetry bridge,
//! and static file serving that are shared between the Tauri desktop binary
//! and the standalone server binary.

pub mod admin;
#[cfg(feature = "desktop")]
pub mod codex_plugin_unlocker;
pub mod proxy_runner;
pub mod telemetry_bridge;
#[cfg(all(feature = "desktop", target_os = "windows"))]
pub mod windows_msix;
