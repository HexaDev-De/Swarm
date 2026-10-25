#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    #[cfg(target_os = "linux")]
    {
        // Fix WebKitGTK black screen / blank window on Linux WebKit renderers
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
