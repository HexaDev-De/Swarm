//! VS Code color themes from `.vsix` packages, imported from a file or Open VSX.
//! Only the workbench `colors` map is kept (validated hex values under dotted keys);
//! nothing in a package is executed. Each theme is stored as one JSON file.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
    time::Duration,
};
use tauri::{AppHandle, Manager};

const MAX_PACKAGE: u64 = 64 * 1024 * 1024;
const MAX_JSON: u64 = 4 * 1024 * 1024;
const MAX_COLORS: usize = 3000;
const MAX_INCLUDES: usize = 4;
const OPEN_VSX: &str = "https://open-vsx.org/api";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub extension: String,
    /// `light` or `dark`; high-contrast variants map to their base.
    pub kind: String,
    pub colors: BTreeMap<String, String>,
}

/// JSON with comments and trailing commas, as VS Code reads theme files.
pub fn strip_jsonc(text: &str) -> String {
    let (mut out, mut chars) = (String::with_capacity(text.len()), text.chars().peekable());
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for skipped in chars.by_ref() {
                    if last == '*' && skipped == '/' {
                        break;
                    }
                    last = skipped;
                }
            }
            _ => out.push(c),
        }
    }
    // Trailing commas: a comma followed only by whitespace before `}` or `]`.
    let bytes: Vec<char> = out.chars().collect();
    let (mut result, mut in_string, mut i) = (String::with_capacity(out.len()), false, 0);
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            result.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                result.push(bytes[i + 1]);
                i += 1;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            result.push(c);
        } else if c == ',' {
            let next = bytes[i + 1..].iter().find(|c| !c.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                result.push(c);
            }
        } else {
            result.push(c);
        }
        i += 1;
    }
    result
}

fn valid_color(value: &str) -> bool {
    let hex = value.strip_prefix('#').unwrap_or("");
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}
fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 80 && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
}
fn slug(text: &str) -> String {
    let mut slug = String::new();
    for c in text.to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').chars().take(60).collect()
}

/// A package path relative to `base`, refusing anything that climbs out.
fn join(base: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<String> = base.split('/').filter(|p| !p.is_empty()).map(str::to_string).collect();
    for component in Path::new(relative.trim_start_matches("./")).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
                if parts.is_empty() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

struct Package<R: Read + std::io::Seek> {
    archive: zip::ZipArchive<R>,
}
impl<R: Read + std::io::Seek> Package<R> {
    fn text(&mut self, name: &str) -> Result<String, String> {
        let mut file = self.archive.by_name(name).map_err(|_| format!("The package is missing {name}."))?;
        if file.size() > MAX_JSON {
            return Err(format!("{name} is too large."));
        }
        let mut text = String::new();
        file.by_ref().take(MAX_JSON).read_to_string(&mut text).map_err(|_| format!("{name} is not text."))?;
        Ok(text.trim_start_matches('\u{feff}').to_string())
    }
    fn json(&mut self, name: &str) -> Result<Value, String> {
        serde_json::from_str(&strip_jsonc(&self.text(name)?)).map_err(|_| format!("{name} is not valid JSON."))
    }
    /// A theme's colors, following `include` chains; the including file wins.
    fn colors(&mut self, path: &str, depth: usize) -> Result<(BTreeMap<String, String>, Option<String>), String> {
        if depth > MAX_INCLUDES {
            return Err("The theme includes too many files.".into());
        }
        let value = self.json(path)?;
        let mut colors = BTreeMap::new();
        let mut kind = value["type"].as_str().map(str::to_string);
        if let Some(include) = value["include"].as_str() {
            let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
            let included = join(dir, include).ok_or("A theme include points outside the package.")?;
            let (base, base_kind) = self.colors(&included, depth + 1)?;
            colors = base;
            kind = kind.or(base_kind);
        }
        for (key, color) in value["colors"].as_object().into_iter().flatten() {
            if let Some(color) = color.as_str().filter(|c| valid_color(c)) {
                if valid_key(key) && colors.len() < MAX_COLORS {
                    colors.insert(key.clone(), color.to_ascii_lowercase());
                }
            }
        }
        Ok((colors, kind))
    }
}

/// Every color theme a `.vsix` contributes.
pub fn read_vsix(bytes: &[u8]) -> Result<Vec<Theme>, String> {
    let archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "That file is not a VS Code extension (.vsix).".to_string())?;
    if archive.len() > 20_000 {
        return Err("The package has too many files.".into());
    }
    let mut package = Package { archive };
    let manifest = package.json("extension/package.json")?;
    let nls = package.json("extension/package.nls.json").ok();
    let localized = |text: &str| -> String {
        let key = text.strip_prefix('%').and_then(|t| t.strip_suffix('%'));
        key.and_then(|key| nls.as_ref()?.get(key)?.as_str().map(str::to_string)).unwrap_or_else(|| text.to_string())
    };
    let publisher = manifest["publisher"].as_str().unwrap_or("unknown");
    let name = manifest["name"].as_str().unwrap_or("theme");
    let extension = localized(manifest["displayName"].as_str().unwrap_or(name));
    let mut themes = Vec::new();
    for entry in manifest["contributes"]["themes"].as_array().into_iter().flatten().take(64) {
        let Some(path) = entry["path"].as_str().and_then(|p| join("extension", p)) else { continue };
        let label = localized(entry["label"].as_str().or(entry["id"].as_str()).unwrap_or(name));
        let (colors, kind) = package.colors(&path, 0)?;
        if colors.is_empty() {
            continue;
        }
        let ui = entry["uiTheme"].as_str().map(str::to_string).or(kind).unwrap_or_default();
        themes.push(Theme {
            id: format!("{}.{}.{}", slug(publisher), slug(name), slug(&label)),
            name: label,
            extension: extension.clone(),
            kind: if matches!(ui.as_str(), "vs" | "hc-light" | "light") { "light" } else { "dark" }.into(),
            colors,
        });
    }
    if themes.is_empty() {
        return Err("This extension contributes no color themes.".into());
    }
    Ok(themes)
}

fn store(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("themes");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 200 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') && !id.starts_with('.')
}
fn save(app: &AppHandle, themes: &[Theme]) -> Result<(), String> {
    let dir = store(app)?;
    for theme in themes {
        let json = serde_json::to_vec(theme).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(format!("{}.json", theme.id)), json).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn themes_list(app: AppHandle) -> crate::Result<Vec<Theme>> {
    crate::blocking(move || {
        let mut themes: Vec<Theme> = std::fs::read_dir(store(&app)?)
            .map_err(|e| e.to_string())?
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
            .filter_map(|entry| serde_json::from_slice(&std::fs::read(entry.path()).ok()?).ok())
            .collect();
        themes.sort_by(|a, b| a.extension.cmp(&b.extension).then(a.name.cmp(&b.name)));
        Ok(themes)
    })
    .await
}

#[tauri::command]
pub async fn theme_import(app: AppHandle, path: String) -> crate::Result<Vec<Theme>> {
    crate::blocking(move || {
        let path = PathBuf::from(path);
        if path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() != Some("vsix") {
            return Err("Choose a .vsix file.".into());
        }
        let size = std::fs::metadata(&path).map_err(|_| "That file is unavailable.".to_string())?.len();
        if size > MAX_PACKAGE {
            return Err("That package is larger than 64 MB.".into());
        }
        let themes = read_vsix(&std::fs::read(&path).map_err(|e| e.to_string())?)?;
        save(&app, &themes)?;
        Ok(themes)
    })
    .await
}

/// `publisher.extension`, as Open VSX and the VS Code marketplace name it.
pub fn parse_extension_id(id: &str) -> Result<(String, String), String> {
    let id = id.trim();
    let (publisher, name) = id.split_once('.').ok_or("Use the extension ID, such as dracula-theme.theme-dracula.")?;
    let ok = |part: &str| !part.is_empty() && part.len() <= 100 && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.');
    if !ok(publisher) || !ok(name) {
        return Err("Use the extension ID, such as dracula-theme.theme-dracula.".into());
    }
    Ok((publisher.to_string(), name.to_string()))
}

/// Download an extension package from Open VSX.
pub async fn fetch_vsix(publisher: &str, name: &str) -> Result<Vec<u8>, String> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::limited(4))
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("PandaCode/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "Could not start the download.".to_string())?;
    let unreachable = |_| "Could not reach Open VSX. Try again later.".to_string();
    let meta = client.get(format!("{OPEN_VSX}/{publisher}/{name}")).send().await.map_err(unreachable)?;
    if meta.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!("Open VSX has no extension {publisher}.{name}."));
    }
    let meta: Value = meta.error_for_status().map_err(unreachable)?.json().await.map_err(unreachable)?;
    let url = meta["files"]["download"].as_str().ok_or("Open VSX listed no download for that extension.")?.to_string();
    let response = client.get(url).send().await.map_err(unreachable)?.error_for_status().map_err(unreachable)?;
    if response.content_length().is_some_and(|len| len > MAX_PACKAGE) {
        return Err("That package is larger than 64 MB.".into());
    }
    let bytes = response.bytes().await.map_err(unreachable)?;
    if bytes.len() as u64 > MAX_PACKAGE {
        return Err("That package is larger than 64 MB.".into());
    }
    Ok(bytes.to_vec())
}

#[tauri::command]
pub async fn theme_install(app: AppHandle, extension_id: String) -> crate::Result<Vec<Theme>> {
    let (publisher, name) = parse_extension_id(&extension_id)?;
    let bytes = fetch_vsix(&publisher, &name).await?;
    crate::blocking(move || {
        let themes = read_vsix(&bytes)?;
        save(&app, &themes)?;
        Ok(themes)
    })
    .await
}

#[tauri::command]
pub async fn theme_remove(app: AppHandle, id: String) -> crate::Result<()> {
    if !valid_id(&id) {
        return Err("Unknown theme.".into());
    }
    crate::blocking(move || match std::fs::remove_file(store(&app)?.join(format!("{id}.json"))) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn vsix(files: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut out);
            for (name, body) in files {
                zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn jsonc_comments_and_trailing_commas_are_removed_outside_strings() {
        let text = "{\n // note\n \"a\": \"http://x/*y*/\", /* block */ \"b\": [1, 2,],\n}";
        let value: Value = serde_json::from_str(&strip_jsonc(text)).unwrap();
        assert_eq!(value["a"], "http://x/*y*/");
        assert_eq!(value["b"], serde_json::json!([1, 2]));
    }

    #[test]
    fn packages_yield_validated_colors_with_includes_and_localized_labels() {
        let package = vsix(&[
            ("extension/package.json", r#"{"publisher":"Dracula-Theme","name":"theme-dracula","displayName":"%display%","contributes":{"themes":[
                {"label":"Dracula","uiTheme":"vs-dark","path":"./themes/dracula.json"},
                {"label":"Dracula Light","uiTheme":"vs","path":"./themes/light.json"},
                {"label":"Escape","uiTheme":"vs-dark","path":"../../etc/passwd"}]}}"#),
            ("extension/package.nls.json", r#"{"display":"Dracula Official"}"#),
            ("extension/themes/base.json", r##"{"colors":{"editor.foreground":"#F8F8F2","focusBorder":"#6272a4"}}"##),
            ("extension/themes/dracula.json", "{\n // dark\n \"include\": \"./base.json\",\n \"colors\": {\"editor.background\": \"#282A36\", \"focusBorder\": \"#bd93f9\", \"bad\": \"red\", \"evil key\": \"#fff\",},\n}"),
            ("extension/themes/light.json", r##"{"colors":{"editor.background":"#ffffff"}}"##),
        ]);
        let themes = read_vsix(&package).unwrap();
        assert_eq!(themes.len(), 2);
        let dark = &themes[0];
        assert_eq!((dark.id.as_str(), dark.name.as_str(), dark.extension.as_str(), dark.kind.as_str()), ("dracula-theme.theme-dracula.dracula", "Dracula", "Dracula Official", "dark"));
        assert_eq!(dark.colors.get("editor.background").unwrap(), "#282a36");
        assert_eq!(dark.colors.get("editor.foreground").unwrap(), "#f8f8f2", "included base");
        assert_eq!(dark.colors.get("focusBorder").unwrap(), "#bd93f9", "the including file wins");
        assert!(!dark.colors.contains_key("bad") && !dark.colors.contains_key("evil key"));
        assert_eq!(themes[1].kind, "light");
    }

    #[test]
    fn non_theme_packages_and_bad_ids_are_refused() {
        assert!(read_vsix(b"not a zip").is_err());
        assert!(read_vsix(&vsix(&[("extension/package.json", r#"{"name":"x","contributes":{}}"#)])).unwrap_err().contains("no color themes"));
        assert_eq!(parse_extension_id(" dracula-theme.theme-dracula ").unwrap(), ("dracula-theme".into(), "theme-dracula".into()));
        assert!(parse_extension_id("no-dot").is_err());
        assert!(parse_extension_id("a/b.c").is_err());
        assert!(!valid_id("../x") && valid_id("dracula-theme.theme-dracula.dracula"));
        assert_eq!(join("extension/themes", "../x.json").as_deref(), Some("extension/x.json"));
        assert_eq!(join("extension", "../../x.json"), None);
    }

    /// Parses real packages from a folder: `PANDACODE_VSIX_DIR=... cargo test -- --ignored vsix_themes::tests::live`.
    #[test]
    #[ignore]
    fn live_packages_parse() {
        let dir = std::env::var("PANDACODE_VSIX_DIR").expect("set PANDACODE_VSIX_DIR");
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let themes = read_vsix(&std::fs::read(entry.path()).unwrap()).unwrap_or_else(|e| panic!("{:?}: {e}", entry.path()));
            for theme in themes {
                println!("{} | {} | {} | {} colors | bg {:?} button {:?}", theme.id, theme.name, theme.kind, theme.colors.len(), theme.colors.get("editor.background"), theme.colors.get("button.background"));
            }
        }
    }


    /// Installs from the real Open VSX: `cargo test -- --ignored vsix_themes::tests::live_open_vsx`.
    #[test]
    #[ignore]
    fn live_open_vsx_install_parses() {
        let bytes = tauri::async_runtime::block_on(fetch_vsix("dracula-theme", "theme-dracula")).unwrap();
        let themes = read_vsix(&bytes).unwrap();
        assert!(themes.iter().any(|t| t.colors.get("editor.background").map(String::as_str) == Some("#282a36")), "{:?}", themes.iter().map(|t| &t.name).collect::<Vec<_>>());
        let missing = tauri::async_runtime::block_on(fetch_vsix("pandacode-nobody", "no-such-theme")).unwrap_err();
        assert!(missing.contains("no extension"), "{missing}");
    }

}
