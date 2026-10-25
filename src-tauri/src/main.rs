#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    #[cfg(target_os = "linux")]
    {
        // Enforce X11/XWayland fallback and bypass broken EGL/DMABuf drivers
        if std::env::var("GDK_BACKEND").is_err() {
            std::env::set_var("GDK_BACKEND", "x11,wayland");
        }
        if std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }
    // First, while single-threaded: agents and tools inherit a PATH that finds `gh`, `node`…
    pandacode_lib::prepare_environment();
    if pandacode_lib::handle_usage_keychain_helper() {
        return;
    }
    pandacode_lib::run()
}
