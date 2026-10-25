use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Material {
    #[cfg(any(target_os = "macos", test))]
    LiquidGlass,
    #[cfg(any(target_os = "macos", test))]
    Vibrancy,
    Solid,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Appearance {
    pub platform: &'static str,
    pub material: Material,
}

#[cfg(any(target_os = "macos", test))]
fn select_material(
    glass_available: bool,
    reduced_transparency: bool,
    increased_contrast: bool,
) -> Material {
    if reduced_transparency || increased_contrast {
        Material::Solid
    } else if glass_available {
        Material::LiquidGlass
    } else {
        Material::Vibrancy
    }
}

pub fn setup(window: &tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let key = window.ns_window().map_err(|error| error.to_string())? as usize;
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                macos::release(key);
            }
        });
        window
            .with_webview(|platform| {
                if let Err(error) = macos::apply(platform, false, false, true) {
                    eprintln!("Native appearance initialization failed: {error}");
                }
            })
            .map_err(|error| error.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window;
    Ok(())
}

pub async fn configure(
    window: tauri::Webview,
    reduced: bool,
    contrast: bool,
    dark_mode: bool,
) -> Result<Appearance, String> {
    window
        .window()
        .set_theme(Some(if dark_mode {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }))
        .map_err(|error| error.to_string())?;
    #[cfg(target_os = "macos")]
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        window
            .with_webview(move |platform| {
                let _ = sender.send(macos::apply(platform, reduced, contrast, dark_mode));
            })
            .map_err(|error| error.to_string())?;
        tokio::time::timeout(std::time::Duration::from_secs(5), receiver)
            .await
            .map_err(|_| "Native appearance update timed out.".to_string())?
            .map_err(|_| "Window closed before updating its appearance.".to_string())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, reduced, contrast);
        Ok(Appearance {
            platform: "other",
            material: Material::Solid,
        })
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use objc2::{rc::Retained, runtime::AnyClass, MainThreadOnly};
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSAutoresizingMaskOptions, NSColor, NSGlassEffectView, NSGlassEffectViewStyle, NSView,
        NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
        NSWindow, NSWorkspace,
    };
    use objc2_foundation::MainThreadMarker;
    use std::{cell::RefCell, collections::HashMap};

    enum Backing {
        Glass {
            glass: Retained<NSGlassEffectView>,
            background: Retained<NSVisualEffectView>,
        },
        Vibrancy(Retained<NSVisualEffectView>),
    }
    impl Backing {
        fn view(&self) -> &NSView {
            match self {
                Self::Glass { background, .. } => background,
                Self::Vibrancy(view) => view,
            }
        }
    }
    struct WindowMaterial {
        root: Retained<NSView>,
        backing: Option<Backing>,
        material: Material,
    }
    thread_local! {
        // AppKit values never leave the main thread. One retained material
        // hierarchy per live window; preference changes replace it as a unit.
        static WINDOWS: RefCell<HashMap<usize, WindowMaterial>> = RefCell::new(HashMap::new());
    }

    pub fn release(key: usize) {
        if MainThreadMarker::new().is_some() {
            WINDOWS.with(|windows| {
                windows.borrow_mut().remove(&key);
            });
        }
    }

    pub fn apply(
        platform: tauri::webview::PlatformWebview,
        reduced: bool,
        contrast: bool,
        dark_mode: bool,
    ) -> Result<Appearance, String> {
        let mtm =
            MainThreadMarker::new().ok_or("AppKit appearance must update on the main thread.")?;
        // Tauri documents inner() as WKWebView and ns_window() as NSWindow.
        // Retain these borrowed handles only while moving the existing view;
        // no webview, browser process, or rendering loop is created here.
        let webview = unsafe { Retained::retain(platform.inner().cast::<NSView>()) }
            .ok_or("Webview is unavailable.")?;
        let window = unsafe { Retained::retain(platform.ns_window().cast::<NSWindow>()) }
            .ok_or("Window is unavailable.")?;
        let key = platform.ns_window() as usize;
        let preferences = NSWorkspace::sharedWorkspace();
        let material = select_material(
            AnyClass::get(c"NSGlassEffectView").is_some(),
            reduced || preferences.accessibilityDisplayShouldReduceTransparency(),
            contrast || preferences.accessibilityDisplayShouldIncreaseContrast(),
        );
        // Apply before selecting background colors so both solid and native
        // translucent materials inherit the user's explicit theme.
        let name = unsafe {
            if dark_mode {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            }
        };
        if let Some(appearance) = NSAppearance::appearanceNamed(name) {
            window.setAppearance(Some(&appearance));
        }
        WINDOWS.with(|windows| -> Result<(), String> {
            let mut windows = windows.borrow_mut();
            if let std::collections::hash_map::Entry::Vacant(entry) = windows.entry(key) {
                // The retained NSWindow still owns Wry's parent content view;
                // this runs on the main thread before any hierarchy mutation.
                let root = unsafe { webview.superview() }.ok_or("Webview has no parent view.")?;
                entry.insert(WindowMaterial {
                    root,
                    backing: None,
                    material: Material::Solid,
                });
            }
            let state = windows
                .get_mut(&key)
                .ok_or("Window appearance state is unavailable.")?;
            if state.material != material {
                let responder = window.firstResponder();
                if let Some(Backing::Glass { glass, .. }) = &state.backing {
                    glass.setContentView(None);
                }
                webview.removeFromSuperview();
                if let Some(old) = state.backing.take() {
                    old.view().removeFromSuperview();
                }
                let frame = state.root.bounds();
                webview.setFrame(frame);
                let resize = NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable;
                webview.setAutoresizingMask(resize);
                state.backing = match material {
                    Material::LiquidGlass => {
                        // The documented behind-window material brings desktop
                        // colors into the window for the glass to sample.
                        let background = NSVisualEffectView::initWithFrame(
                            NSVisualEffectView::alloc(mtm),
                            frame,
                        );
                        background.setMaterial(NSVisualEffectMaterial::Sidebar);
                        background.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
                        background.setState(NSVisualEffectState::FollowsWindowActiveState);
                        background.setAutoresizingMask(resize);
                        // Public macOS26 API, runtime-gated before class lookup.
                        // Apple requires contentView membership for adaptive
                        // legibility: developer.apple.com/videos/play/wwdc2025/310/
                        let glass =
                            NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), frame);
                        glass.setStyle(NSGlassEffectViewStyle::Regular);
                        glass.setCornerRadius(12.0);
                        glass.setAutoresizingMask(resize);
                        glass.setContentView(Some(&webview));
                        background.addSubview(&glass);
                        state.root.addSubview(&background);
                        Some(Backing::Glass { glass, background })
                    }
                    Material::Vibrancy => {
                        let view = NSVisualEffectView::initWithFrame(
                            NSVisualEffectView::alloc(mtm),
                            frame,
                        );
                        view.setMaterial(NSVisualEffectMaterial::Sidebar);
                        view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
                        view.setState(NSVisualEffectState::FollowsWindowActiveState);
                        view.setAutoresizingMask(resize);
                        view.addSubview(&webview);
                        state.root.addSubview(&view);
                        Some(Backing::Vibrancy(view))
                    }
                    Material::Solid => {
                        state.root.addSubview(&webview);
                        None
                    }
                };
                if let Some(responder) = responder {
                    window.makeFirstResponder(Some(&responder));
                }
                state.material = material;
            }
            window.setOpaque(material == Material::Solid);
            let color = if material == Material::Solid {
                NSColor::windowBackgroundColor()
            } else {
                NSColor::clearColor()
            };
            window.setBackgroundColor(Some(&color));
            Ok(())
        })?;
        Ok(Appearance {
            platform: "macos",
            material,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_material_uses_glass_when_available_and_vibrancy_on_older_systems() {
        assert_eq!(select_material(true, false, false), Material::LiquidGlass);
        assert_eq!(select_material(false, false, false), Material::Vibrancy);
    }

    #[test]
    fn accessibility_overrides_each_translucent_material() {
        for available in [false, true] {
            for (reduced, contrast) in [(true, false), (false, true), (true, true)] {
                assert_eq!(
                    select_material(available, reduced, contrast),
                    Material::Solid
                );
            }
        }
    }

    #[test]
    fn appearance_reply_matches_frontend_contract() {
        assert_eq!(
            serde_json::to_value(Appearance {
                platform: "macos",
                material: Material::LiquidGlass
            })
            .unwrap(),
            serde_json::json!({"platform":"macos","material":"liquidGlass"})
        );
    }
}
