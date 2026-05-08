//! TuneCraft v5.0 — Cross-platform music player built with Dioxus.
//!
//! This is the Dioxus-based UI that replaces the iced frontend.
//! Architecture: Component-based (React-like) with Dioxus signals and hooks.

mod app;
mod app_state;
mod components;
mod i18n;
mod media_keys;
mod styles;

#[cfg(target_os = "windows")]
fn setup_gstreamer_env() {
    if let Ok(mut exe_path) = std::env::current_exe() {
        exe_path.pop(); // Get directory containing Tunecraft.exe
        let plugins_dir = exe_path.join("plugins");
        if plugins_dir.exists() {
            std::env::set_var("GST_PLUGIN_PATH", &plugins_dir);
            std::env::set_var("GST_PLUGIN_SYSTEM_PATH", &plugins_dir);
        }
        std::env::set_var("GST_REGISTRY_FORK", "no");
    }
}

fn main() {
    #[cfg(target_os = "windows")]
    setup_gstreamer_env();

    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("TUNECRAFT_LOG_LEVEL").unwrap_or("info".into()))
        .init();

    crate::i18n::init_i18n();

    tracing::info!("TuneCraft v5.0 starting (Dioxus UI)");

    dioxus::launch(app::App);
}
