//! The app's own icon for a project, for its sidebar rows: the first favicon or app icon
//! found in the usual places (web `public/` and `static/` folders, Next.js `app/`, Tauri
//! and Electron icon folders, the project root), returned as a `data:` URL the webview
//! can show without file access. Only small image files are read; anything else falls
//! back to the sidebar's default icon.
use std::path::Path;

use base64::Engine;

const LIMIT: u64 = 256 * 1024;

/// Checked in order; the first readable image wins. Vector and larger icons come first.
const CANDIDATES: &[&str] = &[
    "public/favicon.svg", "public/icon.svg", "public/logo.svg", "public/apple-touch-icon.png",
    "public/favicon.png", "public/icon.png", "public/logo.png", "public/favicon.ico",
    "static/favicon.svg", "static/favicon.png", "static/favicon.ico",
    "app/icon.svg", "app/icon.png", "app/apple-icon.png", "app/favicon.ico",
    "src/app/icon.svg", "src/app/icon.png", "src/app/apple-icon.png", "src/app/favicon.ico",
    "src/assets/icon.svg", "src/assets/logo.svg", "src/assets/icon.png", "src/assets/logo.png",
    "assets/icon.svg", "assets/icon.png", "assets/logo.svg", "assets/logo.png",
    "src-tauri/icons/128x128.png", "src-tauri/icons/icon.png",
    "build/icon.png", "resources/icon.png",
    "favicon.svg", "favicon.png", "favicon.ico", "icon.svg", "icon.png", "logo.svg", "logo.png",
];

fn media_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "image/png",
    }
}

/// The project's icon as a `data:` URL, or None when it has none we can show.
pub fn find(root: &Path) -> Option<String> {
    for candidate in CANDIDATES {
        let path = root.join(candidate);
        let Ok(metadata) = std::fs::metadata(&path) else { continue };
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > LIMIT {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        if !looks_like(candidate, &bytes) {
            continue;
        }
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        return Some(format!("data:{};base64,{encoded}", media_type(candidate)));
    }
    None
}

/// A cheap check that the file is the image its name says, so a stray text file or an
/// unrelated binary named `icon.png` never reaches the sidebar.
fn looks_like(path: &str, bytes: &[u8]) -> bool {
    match media_type(path) {
        "image/svg+xml" => {
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
            head.contains("<svg")
        }
        "image/x-icon" => bytes.starts_with(&[0, 0, 1, 0]) || bytes.starts_with(b"\x89PNG"),
        _ => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("pandacode-icon-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn finds_the_first_real_icon_in_the_usual_places() {
        let root = project();
        assert_eq!(find(&root), None);
        std::fs::create_dir_all(root.join("public")).unwrap();
        std::fs::write(root.join("public/favicon.ico"), [0u8, 0, 1, 0, 1, 0]).unwrap();
        assert!(find(&root).unwrap().starts_with("data:image/x-icon;base64,"));
        std::fs::write(root.join("public/favicon.svg"), "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
        assert!(find(&root).unwrap().starts_with("data:image/svg+xml;base64,"), "an SVG is preferred");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn skips_files_that_are_not_what_they_claim_or_are_too_large() {
        let root = project();
        std::fs::write(root.join("icon.png"), "not a png").unwrap();
        std::fs::write(root.join("logo.svg"), vec![b'x'; (LIMIT + 1) as usize]).unwrap();
        assert_eq!(find(&root), None);
        std::fs::write(root.join("favicon.png"), b"\x89PNG\r\n\x1a\nrest").unwrap();
        assert!(find(&root).unwrap().starts_with("data:image/png;base64,"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
