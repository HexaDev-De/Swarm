//! Desktop webview integration, compiled only into its corresponding OS build.
use super::{current_view, update, Result};
use std::time::Duration;
use tauri::{Manager, Webview};

#[cfg(target_os = "linux")]
pub(super) fn position_linux(view: &Webview, bounds: super::BrowserBounds) -> Result<()> {
    use gtk::prelude::*;
    let _ = view.with_webview(move |platform| {
        let native = platform.inner();
        let native_widget = native.upcast_ref::<gtk::Widget>();
        if let Some(parent) = native_widget.parent() {
            if let Ok(_overlay) = parent.clone().dynamic_cast::<gtk::Overlay>() {
                native_widget.set_halign(gtk::Align::Start);
                native_widget.set_valign(gtk::Align::Start);
                native_widget.set_margin_start(bounds.x.round() as i32);
                native_widget.set_margin_top(bounds.y.round() as i32);
                native_widget.set_size_request(bounds.width.round() as i32, bounds.height.round() as i32);
                native_widget.queue_resize();
            } else if let Ok(vbox) = parent.clone().dynamic_cast::<gtk::Box>() {
                let children = vbox.children();
                if let Some(main_view) = children.iter().find(|c| *c != native_widget) {
                    let overlay = gtk::Overlay::new();
                    vbox.remove(main_view);
                    vbox.remove(native_widget);

                    overlay.add(main_view);
                    overlay.add_overlay(native_widget);

                    native_widget.set_halign(gtk::Align::Start);
                    native_widget.set_valign(gtk::Align::Start);
                    native_widget.set_margin_start(bounds.x.round() as i32);
                    native_widget.set_margin_top(bounds.y.round() as i32);
                    native_widget.set_size_request(bounds.width.round() as i32, bounds.height.round() as i32);

                    vbox.pack_start(&overlay, true, true, 0);
                    overlay.show_all();
                }
            }
        }
    });
    Ok(())
}

#[cfg(target_os = "linux")]
pub(super) fn sync_status(view: &Webview, generation: u64) {
    use webkit2gtk::WebViewExt;
    let app = view.app_handle().clone();
    let _ = view.with_webview(move |platform| {
        if !current_view(&app, generation) {
            return;
        }
        let native = platform.inner();
        update(&app, |status| {
            if let Some(url) = native.uri() {
                status.url = url.to_string();
            }
            if let Some(title) = native.title() {
                status.title = super::clip(&title, 300);
            }
            status.loading = native.is_loading();
            status.can_go_back = native.can_go_back();
            status.can_go_forward = native.can_go_forward();
        });
    });
}

#[cfg(target_os = "windows")]
pub(super) fn sync_status(view: &Webview, generation: u64) {
    use windows::core::BOOL;
    let app = view.app_handle().clone();
    let url = view.url().ok();
    let _ = view.with_webview(move |platform| unsafe {
        if !current_view(&app, generation) {
            return;
        }
        let Ok(native) = platform.controller().CoreWebView2() else {
            return;
        };
        let (mut back, mut forward) = (BOOL(0), BOOL(0));
        let _ = native.CanGoBack(&mut back);
        let _ = native.CanGoForward(&mut forward);
        update(&app, |status| {
            if let Some(url) = url {
                status.url = url.to_string();
            }
            status.can_go_back = back.as_bool();
            status.can_go_forward = forward.as_bool();
        });
    });
    // Navigation/title events handle the normal lifecycle. This bounded fallback
    // also settles failed loads; it gives guest pages no application IPC access.
    let app = view.app_handle().clone();
    let navigation = app
        .state::<super::BrowserState>()
        .load_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    let _ = view.eval_with_callback("document.readyState === 'complete'", move |value| {
        if current_view(&app, generation)
            && app
                .state::<super::BrowserState>()
                .load_generation
                .load(std::sync::atomic::Ordering::SeqCst)
                == navigation
            && value == "true"
        {
            update(&app, |status| status.loading = false);
        }
    });
}

pub(super) fn action(view: &Webview, action: &str) -> Result<()> {
    let action = action.to_owned();
    view.with_webview(move |platform| {
        #[cfg(target_os = "linux")]
        {
            use webkit2gtk::WebViewExt;
            let native = platform.inner();
            match action.as_str() {
                "back" => native.go_back(),
                "forward" => native.go_forward(),
                "reload" => native.reload(),
                _ => native.stop_loading(),
            }
        }
        #[cfg(target_os = "windows")]
        unsafe {
            if let Ok(native) = platform.controller().CoreWebView2() {
                let _ = match action.as_str() {
                    "back" => native.GoBack(),
                    "forward" => native.GoForward(),
                    "reload" => native.Reload(),
                    _ => native.Stop(),
                };
            }
        }
    })
    .map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
async fn snapshot(view: &Webview) -> Result<Vec<u8>> {
    use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |platform| {
        platform.inner().snapshot(
            SnapshotRegion::Visible,
            SnapshotOptions::NONE,
            None::<&gio::Cancellable>,
            move |surface| {
                let result = (|| {
                    let surface = surface.map_err(|error| error.to_string())?;
                    let original = cairo::ImageSurface::try_from(surface)
                        .map_err(|_| "Snapshot image is unavailable.")?;
                    let scale = (1280.0 / original.width().max(1) as f64)
                        .min(2048.0 / original.height().max(1) as f64)
                        .min(1.0);
                    let image = cairo::ImageSurface::create(
                        cairo::Format::ARgb32,
                        (original.width() as f64 * scale).max(1.0) as i32,
                        (original.height() as f64 * scale).max(1.0) as i32,
                    )
                    .map_err(|error| error.to_string())?;
                    let context = cairo::Context::new(&image).map_err(|error| error.to_string())?;
                    context.scale(scale, scale);
                    context
                        .set_source_surface(&original, 0.0, 0.0)
                        .map_err(|error| error.to_string())?;
                    context.paint().map_err(|error| error.to_string())?;
                    let mut bytes = Vec::new();
                    image
                        .write_to_png(&mut bytes)
                        .map_err(|error| error.to_string())?;
                    Ok(bytes)
                })();
                let _ = tx.send(result);
            },
        );
    })
    .map_err(|error| error.to_string())?;
    tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .map_err(|_| "Browser screenshot timed out.")?
        .map_err(|_| "Browser closed before taking a screenshot.")?
}

#[cfg(target_os = "windows")]
async fn snapshot(view: &Webview) -> Result<Vec<u8>> {
    use std::{cell::RefCell, rc::Rc};
    use webview2_com::{
        CapturePreviewCompletedHandler,
        Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
    };
    use windows::Win32::{
        Foundation::HGLOBAL,
        System::Com::{
            StructuredStorage::CreateStreamOnHGlobal, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET,
        },
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |platform| unsafe {
        let tx = Rc::new(RefCell::new(Some(tx)));
        let start = (|| -> windows::core::Result<()> {
            let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
            let reader = stream.clone();
            let reply = tx.clone();
            let callback = CapturePreviewCompletedHandler::create(Box::new(move |result| {
                let result = (|| -> Result<Vec<u8>> {
                    result.map_err(|error| error.to_string())?;
                    let mut metadata = STATSTG::default();
                    reader
                        .Stat(&mut metadata, STATFLAG_NONAME)
                        .map_err(|error| error.to_string())?;
                    if metadata.cbSize == 0 || metadata.cbSize > 32 * 1024 * 1024 {
                        return Err(
                            "Screenshot is too large. Reduce the browser panel size.".into()
                        );
                    }
                    reader
                        .Seek(0, STREAM_SEEK_SET, None)
                        .map_err(|error| error.to_string())?;
                    let mut bytes = vec![0; metadata.cbSize as usize];
                    let mut read = 0;
                    reader
                        .Read(
                            bytes.as_mut_ptr().cast(),
                            bytes.len() as u32,
                            Some(&mut read),
                        )
                        .ok()
                        .map_err(|error| error.to_string())?;
                    if read as usize != bytes.len() {
                        return Err("Snapshot could not be read completely.".into());
                    }
                    Ok(bytes)
                })();
                if let Some(tx) = reply.borrow_mut().take() {
                    let _ = tx.send(result);
                }
                Ok(())
            }));
            platform.controller().CoreWebView2()?.CapturePreview(
                COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
                &stream,
                &callback,
            )
        })();
        if let Err(error) = start {
            if let Some(tx) = tx.borrow_mut().take() {
                let _ = tx.send(Err(error.to_string()));
            }
        }
    })
    .map_err(|error| error.to_string())?;
    tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .map_err(|_| "Browser screenshot timed out.")?
        .map_err(|_| "Browser closed before taking a screenshot.")?
}

pub(super) async fn capture(view: &Webview) -> Result<(Vec<u8>, u32, u32)> {
    let size = view.size().map_err(|error| error.to_string())?;
    if u64::from(size.width) * u64::from(size.height) > 16_000_000 {
        return Err("Reduce the browser panel size before taking a screenshot.".into());
    }
    let bytes = snapshot(view).await?;
    // Decoding/resizing runs away from the webview thread and has an allocation cap.
    tokio::task::spawn_blocking(move || {
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|_| "Snapshot could not be decoded.")?
            .thumbnail(1280, 2048);
        let (width, height) = (image.width(), image.height());
        let mut png = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|_| "Snapshot could not be encoded.")?;
        Ok((png.into_inner(), width, height))
    })
    .await
    .map_err(|_| "Browser screenshot could not be processed.".to_string())?
}
