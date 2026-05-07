use crate::state::AppState;
use eframe::egui;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let title = match module_path!() {
        s if s.ends_with("::providers") => "providers.title",
        s if s.ends_with("::providers_add") => "providersAdd.title",
        s if s.ends_with("::desktop") => "desktop.title",
        s if s.ends_with("::proxy") => "proxy.title",
        s if s.ends_with("::guide") => "guide.title",
        _ => "common.cancel",
    };
    let week = match module_path!() {
        s if s.ends_with("::providers") || s.ends_with("::providers_add") => "W4",
        _ => "W5",
    };
    super::placeholder(ui, state.settings.language, title, week);
}
