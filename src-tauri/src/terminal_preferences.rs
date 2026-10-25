//! Best-effort import of local terminal appearance. No terminal, shell, Lua,
//! includes, or theme scripts are executed. Unsupported/dynamic values stay unset.
//! Font size stays controlled by the app's UI scale, never by imported settings.
//!
//! Formats: ghostty.org/docs/config; sw.kovidgoyal.net/kitty/conf;
//! alacritty.org/config-alacritty.html; wezterm.org/config/files.html;
//! learn.microsoft.com/windows/terminal/customize-settings/profile-appearance.

use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, fs, io::Read, path::PathBuf, time::SystemTime};

const MAX_CONFIG_BYTES: u64 = 128 * 1024;
const ANSI: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "brightBlack",
    "brightRed",
    "brightGreen",
    "brightYellow",
    "brightBlue",
    "brightMagenta",
    "brightCyan",
    "brightWhite",
];

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPreferences {
    pub source: Option<String>,
    pub font_family: Option<String>,
    pub line_height: Option<f64>,
    pub cursor_style: Option<String>,
    pub cursor_blink: Option<bool>,
    pub theme: Option<BTreeMap<String, String>>,
}

impl TerminalPreferences {
    fn useful(&self) -> bool {
        self.font_family.is_some()
            || self.line_height.is_some()
            || self.cursor_style.is_some()
            || self.cursor_blink.is_some()
            || self.theme.as_ref().is_some_and(|theme| !theme.is_empty())
    }
    fn color(&mut self, key: &str, value: &str) {
        if let Some(value) = color(value) {
            self.theme
                .get_or_insert_with(BTreeMap::new)
                .insert(key.into(), value);
        }
    }
}

#[derive(Clone, Copy)]
enum Format {
    Ghostty,
    Kitty,
    Alacritty,
    WezTerm,
    WindowsTerminal,
}
impl Format {
    fn name(self) -> &'static str {
        match self {
            Self::Ghostty => "Ghostty",
            Self::Kitty => "Kitty",
            Self::Alacritty => "Alacritty",
            Self::WezTerm => "WezTerm",
            Self::WindowsTerminal => "Windows Terminal",
        }
    }
    fn parse(self, source: &str) -> TerminalPreferences {
        match self {
            Self::Ghostty | Self::Kitty => parse_lines(source, self),
            Self::Alacritty => parse_alacritty(source),
            Self::WezTerm => parse_wezterm(source),
            Self::WindowsTerminal => parse_windows(source),
        }
    }
}

/// The most recently modified usable appearance configuration wins. This avoids
/// assuming that the presence of an older terminal config means it is preferred.
/// Only the terminal's name leaves native code; local paths and contents do not.
pub fn discover() -> TerminalPreferences {
    discover_paths(candidates(|name| std::env::var_os(name).map(PathBuf::from)))
}

fn candidates(env: impl Fn(&str) -> Option<PathBuf>) -> Vec<(Format, PathBuf)> {
    let home = if cfg!(windows) {
        env("USERPROFILE").or_else(|| env("HOME"))
    } else {
        env("HOME").or_else(|| env("USERPROFILE"))
    }
    .filter(|path| path.is_absolute());
    let xdg = env("XDG_CONFIG_HOME")
        .filter(|path| path.is_absolute())
        .or_else(|| home.as_ref().map(|home| home.join(".config")));
    let mut paths = Vec::new();
    if let Some(xdg) = xdg {
        paths.extend([
            (Format::Ghostty, xdg.join("ghostty/config.ghostty")),
            (Format::Ghostty, xdg.join("ghostty/config")),
            (Format::Kitty, xdg.join("kitty/kitty.conf")),
            (Format::Alacritty, xdg.join("alacritty/alacritty.toml")),
            (Format::Alacritty, xdg.join("alacritty.toml")),
            (Format::WezTerm, xdg.join("wezterm/wezterm.lua")),
        ]);
    }
    if let Some(home) = home {
        paths.push((Format::Alacritty, home.join(".alacritty.toml")));
        paths.push((Format::WezTerm, home.join(".wezterm.lua")));
        paths.push((Format::WezTerm, home.join(".config/wezterm/wezterm.lua")));
        if cfg!(target_os = "macos") {
            for file in ["config.ghostty", "config"] {
                paths.push((
                    Format::Ghostty,
                    home.join("Library/Application Support/com.mitchellh.ghostty")
                        .join(file),
                ));
            }
        }
    }
    if let Some(appdata) = env("APPDATA").filter(|path| path.is_absolute()) {
        paths.push((Format::Alacritty, appdata.join("alacritty/alacritty.toml")));
    }
    if let Some(local) = env("LOCALAPPDATA").filter(|path| path.is_absolute()) {
        for path in [
            "Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json",
            "Packages/Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe/LocalState/settings.json",
            "Microsoft/Windows Terminal/settings.json",
        ] {
            paths.push((Format::WindowsTerminal, local.join(path)));
        }
    }
    paths
}

fn discover_paths(paths: Vec<(Format, PathBuf)>) -> TerminalPreferences {
    let mut best: Option<(SystemTime, TerminalPreferences)> = None;
    for (format, path) in paths {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
            continue;
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        // A changed symlink must not turn a tiny configuration read into a FIFO wait.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let Ok(file) = options.open(path) else {
            continue;
        };
        let Ok(metadata) = file.metadata() else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
            continue;
        }
        let mut source = String::new();
        if file
            .take(MAX_CONFIG_BYTES + 1)
            .read_to_string(&mut source)
            .is_err()
            || source.len() as u64 > MAX_CONFIG_BYTES
        {
            continue;
        }
        let mut prefs = format.parse(&source);
        if !prefs.useful() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(current, _)| modified > *current) {
            prefs.source = Some(format.name().into());
            best = Some((modified, prefs));
        }
    }
    best.map(|(_, prefs)| prefs).unwrap_or_default()
}

fn unquote(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}
fn font(value: &str) -> Option<String> {
    let value = unquote(value).trim();
    (!value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_alphanumeric() || " -_.+()".contains(ch)))
    .then(|| value.to_string())
}
fn number(value: &str, min: f64, max: f64) -> Option<f64> {
    let number = value.trim().parse::<f64>().ok()?;
    (number.is_finite() && (min..=max).contains(&number)).then_some(number)
}
fn ratio(value: &str) -> Option<f64> {
    number(value, 1.0, 3.0)
}
fn boolean(value: &str) -> Option<bool> {
    match value {
        "true" | "yes" => Some(true),
        "false" | "no" => Some(false),
        _ => None,
    }
}
fn cursor(value: &str) -> Option<String> {
    match value {
        "block" | "Block" | "block_hollow" | "filledBox" | "emptyBox" => Some("block"),
        "bar" | "Bar" | "beam" | "Beam" => Some("bar"),
        "underline" | "Underline" | "underscore" | "doubleUnderscore" => Some("underline"),
        _ => None,
    }
    .map(str::to_owned)
}
fn color(value: &str) -> Option<String> {
    let value = unquote(value);
    let value = value
        .strip_prefix('#')
        .or_else(|| value.strip_prefix("0x"))
        .unwrap_or(value);
    if value.len() == 6 && value.bytes().all(|ch| ch.is_ascii_hexdigit()) {
        Some(format!("#{}", value.to_ascii_lowercase()))
    } else if value.len() == 3 && value.bytes().all(|ch| ch.is_ascii_hexdigit()) {
        Some(format!(
            "#{}",
            value
                .chars()
                .flat_map(|ch| [ch, ch])
                .collect::<String>()
                .to_ascii_lowercase()
        ))
    } else {
        None
    }
}

fn parse_lines(source: &str, format: Format) -> TerminalPreferences {
    let mut prefs = TerminalPreferences::default();
    for line in source
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
    {
        let pair = if matches!(format, Format::Ghostty) {
            line.split_once('=')
        } else {
            line.find(char::is_whitespace)
                .map(|at| (&line[..at], &line[at..]))
        };
        let Some((key, value)) = pair else { continue };
        let key = key.trim();
        let value = unquote(value.trim());
        match key {
            "font-family" | "font_family" => {
                let value = value.strip_prefix("family=").unwrap_or(value);
                prefs.font_family = font(value);
            }
            "adjust-cell-height" => {
                prefs.line_height = value
                    .strip_suffix('%')
                    .and_then(|v| number(v, 0.0, 200.0))
                    .map(|v| 1.0 + v / 100.0)
            }
            "modify_font" => {
                if let Some(value) = value.strip_prefix("cell_height ") {
                    prefs.line_height = value
                        .trim()
                        .strip_suffix('%')
                        .and_then(|v| number(v, 100.0, 300.0))
                        .map(|v| v / 100.0);
                }
            }
            "cursor-style" | "cursor_shape" => prefs.cursor_style = cursor(value),
            "cursor-style-blink" => prefs.cursor_blink = boolean(value),
            "cursor_blink_interval" => {
                prefs.cursor_blink = value
                    .split_whitespace()
                    .next()
                    .and_then(|v| number(v, 0.0, 60.0))
                    .map(|v| v > 0.0)
            }
            "foreground" | "background" => prefs.color(key, value),
            "cursor-color" | "cursor" => prefs.color("cursor", value),
            "cursor-text" | "cursor_text_color" => prefs.color("cursorAccent", value),
            "selection-background" | "selection_background" => {
                prefs.color("selectionBackground", value)
            }
            "selection-foreground" | "selection_foreground" => {
                prefs.color("selectionForeground", value)
            }
            "palette" => {
                if let Some((index, value)) = value.split_once('=') {
                    if let Some(key) = index.trim().parse::<usize>().ok().and_then(|i| ANSI.get(i))
                    {
                        prefs.color(key, value);
                    }
                }
            }
            _ => {
                if let Some(key) = key
                    .strip_prefix("color")
                    .and_then(|i| i.parse::<usize>().ok())
                    .and_then(|i| ANSI.get(i))
                {
                    prefs.color(key, value);
                }
            }
        }
    }
    prefs
}

fn parse_alacritty(source: &str) -> TerminalPreferences {
    let Ok(value) = toml::from_str::<Value>(source) else {
        return TerminalPreferences::default();
    };
    let mut prefs = TerminalPreferences {
        font_family: value
            .pointer("/font/normal/family")
            .and_then(Value::as_str)
            .and_then(font),
        ..Default::default()
    };
    let style = value.pointer("/cursor/style");
    prefs.cursor_style = style
        .and_then(|v| {
            v.as_str()
                .or_else(|| v.get("shape").and_then(Value::as_str))
        })
        .and_then(cursor);
    prefs.cursor_blink = style
        .and_then(|v| v.get("blinking"))
        .and_then(Value::as_str)
        .and_then(|v| match v {
            "On" | "Always" => Some(true),
            "Off" | "Never" => Some(false),
            _ => None,
        });
    for (path, key) in [
        ("/colors/primary/foreground", "foreground"),
        ("/colors/primary/background", "background"),
        ("/colors/cursor/cursor", "cursor"),
        ("/colors/cursor/text", "cursorAccent"),
        ("/colors/selection/background", "selectionBackground"),
        ("/colors/selection/text", "selectionForeground"),
    ] {
        if let Some(value) = value.pointer(path).and_then(Value::as_str) {
            prefs.color(key, value);
        }
    }
    for (index, key) in ANSI.iter().enumerate() {
        let path = format!(
            "/colors/{}/{}",
            if index < 8 { "normal" } else { "bright" },
            ANSI[index % 8]
        );
        if let Some(value) = value.pointer(&path).and_then(Value::as_str) {
            prefs.color(key, value);
        }
    }
    prefs
}

/// Strip JSONC/Lua comments without treating comment markers inside strings as
/// comments. Lua long strings/comments are deliberately unsupported.
fn strip_comments(source: &str, lua: bool) -> Option<String> {
    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    let mut quote = 0;
    while i < bytes.len() {
        if quote != 0 {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == quote {
                quote = 0;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' || lua && bytes[i] == b'\'' {
            quote = bytes[i];
            i += 1;
            continue;
        }
        if lua && bytes[i..].starts_with(b"[[") {
            return None;
        }
        if bytes[i..].starts_with(if lua { b"--" } else { b"//" }) {
            if lua && bytes.get(i + 2) == Some(&b'[') {
                return None;
            }
            while i < bytes.len() && bytes[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
        } else if !lua && bytes[i..].starts_with(b"/*") {
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            while i + 1 < bytes.len() && !bytes[i..].starts_with(b"*/") {
                if bytes[i] != b'\n' {
                    out[i] = b' ';
                }
                i += 1;
            }
            if i + 1 >= bytes.len() {
                return None;
            }
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
        } else {
            i += 1;
        }
    }
    if quote != 0 {
        return None;
    }
    String::from_utf8(out).ok()
}

fn parse_windows(source: &str) -> TerminalPreferences {
    let Some(mut source) = strip_comments(source, false) else {
        return TerminalPreferences::default();
    };
    // JSONC permits a trailing comma before a closing array/object.
    let mut bytes = source.into_bytes();
    let mut quote = false;
    let mut i = 0;
    while i < bytes.len() {
        if quote && bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            quote = !quote;
        } else if !quote
            && bytes[i] == b','
            && bytes[i + 1..]
                .iter()
                .find(|b| !b.is_ascii_whitespace())
                .is_some_and(|b| *b == b'}' || *b == b']')
        {
            bytes[i] = b' ';
        }
        i += 1;
    }
    source = String::from_utf8(bytes).unwrap_or_default();
    let Ok(value) = serde_json::from_str::<Value>(&source) else {
        return TerminalPreferences::default();
    };
    let defaults = value.pointer("/profiles/defaults").unwrap_or(&Value::Null);
    let profile = value
        .pointer("/profiles/list")
        .and_then(Value::as_array)
        .and_then(|profiles| {
            let selected = value.get("defaultProfile").and_then(Value::as_str)?;
            profiles.iter().find(|p| {
                p.get("guid").and_then(Value::as_str) == Some(selected)
                    || p.get("name").and_then(Value::as_str) == Some(selected)
            })
        })
        .unwrap_or(&Value::Null);
    let setting = |key: &str| profile.get(key).or_else(|| defaults.get(key));
    let font_setting = |key: &str| {
        profile
            .get("font")
            .and_then(|f| f.get(key))
            .or_else(|| defaults.get("font").and_then(|f| f.get(key)))
    };
    let mut prefs = TerminalPreferences {
        font_family: font_setting("face")
            .or_else(|| setting("fontFace"))
            .and_then(Value::as_str)
            .and_then(font),
        line_height: font_setting("cellHeight")
            .and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .and_then(|v| ratio(&v.to_string())),
        cursor_style: setting("cursorShape")
            .and_then(Value::as_str)
            .and_then(cursor),
        ..Default::default()
    };
    if let Some(name) = setting("colorScheme").and_then(Value::as_str) {
        if let Some(scheme) = value
            .get("schemes")
            .and_then(Value::as_array)
            .and_then(|schemes| {
                schemes
                    .iter()
                    .find(|s| s.get("name").and_then(Value::as_str) == Some(name))
            })
        {
            windows_colors(&mut prefs, |key| scheme.get(key));
        }
    }
    windows_colors(&mut prefs, setting);
    prefs
}
fn windows_colors<'a>(prefs: &mut TerminalPreferences, get: impl Fn(&str) -> Option<&'a Value>) {
    for key in ANSI {
        if let Some(value) = get(key).and_then(Value::as_str) {
            prefs.color(key, value);
        }
    }
    for (key, target) in [
        ("background", "background"),
        ("foreground", "foreground"),
        ("cursorColor", "cursor"),
        ("selectionBackground", "selectionBackground"),
    ] {
        if let Some(value) = get(key).and_then(Value::as_str) {
            prefs.color(target, value);
        }
    }
}

fn parse_wezterm(source: &str) -> TerminalPreferences {
    let Some(source) = strip_comments(source, true) else {
        return TerminalPreferences::default();
    };
    // Never infer the result of conditional/computed Lua. Common static config
    // builder assignments and literal return tables are enough for appearance.
    let mut outside = String::new();
    let mut quote = None;
    let mut escaped = false;
    for ch in source.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote.is_some() && ch == '\\' {
            escaped = true;
            continue;
        }
        if quote == Some(ch) {
            quote = None;
            continue;
        }
        if quote.is_none() && matches!(ch, '\'' | '"') {
            quote = Some(ch);
            continue;
        }
        if quote.is_none() {
            outside.push(ch);
        }
    }
    if outside
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .any(|word| {
            matches!(
                word,
                "if" | "then"
                    | "else"
                    | "elseif"
                    | "function"
                    | "for"
                    | "while"
                    | "repeat"
                    | "until"
                    | "do"
                    | "end"
            )
        })
    {
        return TerminalPreferences::default();
    }
    let mut prefs = TerminalPreferences::default();
    for line in source.lines() {
        let line = line.trim().trim_end_matches([',', ';']).trim();
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().strip_prefix("config.").unwrap_or(key.trim());
        let raw = raw.trim();
        match key {
            "font" => {
                if let Some(raw) = raw.strip_prefix("wezterm.font") {
                    let raw = raw.trim();
                    let raw = raw
                        .strip_prefix('(')
                        .and_then(|v| v.strip_suffix(')'))
                        .unwrap_or(raw)
                        .trim();
                    if raw.starts_with(['\'', '"']) {
                        prefs.font_family = font(raw);
                    }
                }
            }
            "line_height" => prefs.line_height = ratio(raw),
            "default_cursor_style" => {
                let value = unquote(raw);
                let style = value
                    .strip_prefix("Blinking")
                    .or_else(|| value.strip_prefix("Steady"));
                if let Some(style) = style {
                    prefs.cursor_style = cursor(style);
                    if prefs.cursor_style.is_some() {
                        prefs.cursor_blink = Some(value.starts_with("Blinking"));
                    }
                }
            }
            // Only explicit scalar color assignments, never color_scheme loading.
            "colors.foreground" => prefs.color("foreground", raw),
            "colors.background" => prefs.color("background", raw),
            "colors.cursor_bg" => prefs.color("cursor", raw),
            "colors.cursor_fg" => prefs.color("cursorAccent", raw),
            "colors.selection_bg" => prefs.color("selectionBackground", raw),
            "colors.selection_fg" => prefs.color("selectionForeground", raw),
            _ => {}
        }
    }
    prefs
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn font_sizes_are_not_imported_from_any_terminal() {
        for (format, source) in [
            (Format::Ghostty, "font-size = 48"),
            (Format::Kitty, "font_size 48"),
            (Format::Alacritty, "[font]\nsize = 48"),
            (Format::WezTerm, "config.font_size = 48"),
            (
                Format::WindowsTerminal,
                r#"{"profiles":{"defaults":{"font":{"size":48}}}}"#,
            ),
            (
                Format::WindowsTerminal,
                r#"{"profiles":{"defaults":{"fontSize":48}}}"#,
            ),
        ] {
            let prefs = format.parse(source);
            assert!(!prefs.useful(), "{} imported a font size", format.name());
            assert!(serde_json::to_value(prefs)
                .unwrap()
                .get("fontSize")
                .is_none());
        }
    }
    #[test]
    fn ghostty_imports_only_safe_appearance() {
        let prefs = Format::Ghostty.parse("font-family = \"JetBrains Mono\"\nfont-size = 12\nadjust-cell-height = 20%\ncursor-style=bar\ncursor-style-blink=false\nforeground=dddddd\nbackground=#123\npalette=9=#ff0000\ncommand=steal-secrets\nconfig-file=/secret\ntheme=unresolved");
        assert_eq!(prefs.font_family.as_deref(), Some("JetBrains Mono"));
        assert_eq!(prefs.line_height, Some(1.2));
        assert_eq!(prefs.cursor_style.as_deref(), Some("bar"));
        assert_eq!(prefs.cursor_blink, Some(false));
        let theme = prefs.theme.unwrap();
        assert_eq!(theme["background"], "#112233");
        assert_eq!(theme["brightRed"], "#ff0000");
        assert_eq!(theme.len(), 3);
    }
    #[test]
    fn kitty_maps_font_family_cell_ratio_and_cursor() {
        let prefs = Format::Kitty.parse("font_family family=\"Fira Code\"\nfont_size 13.5\nmodify_font cell_height 120%\ncursor_shape beam\ncursor_blink_interval 0\ncolor0 #112233\ninclude secret.conf");
        assert_eq!(prefs.font_family.as_deref(), Some("Fira Code"));
        assert_eq!(prefs.line_height, Some(1.2));
        assert_eq!(prefs.cursor_blink, Some(false));
        assert_eq!(prefs.cursor_style.as_deref(), Some("bar"));
        assert_eq!(prefs.theme.unwrap()["black"], "#112233");
    }
    #[test]
    fn alacritty_parses_toml_without_loading_imports() {
        let prefs = parse_alacritty("[general]\nimport=['/secret']\n[font]\nsize=12.0\nnormal={family='Iosevka'}\n[cursor]\nstyle={shape='Underline',blinking='Always'}\n[colors.primary]\nbackground='#102030'\n[colors.bright]\nred='0xff4444'");
        assert_eq!(prefs.font_family.as_deref(), Some("Iosevka"));
        assert_eq!(prefs.cursor_style.as_deref(), Some("underline"));
        assert_eq!(prefs.cursor_blink, Some(true));
        assert_eq!(prefs.theme.unwrap()["brightRed"], "#ff4444");
    }
    #[test]
    fn windows_jsonc_uses_default_profile_overrides_and_custom_palette() {
        let prefs = parse_windows(
            r##"{ // Profile defaults are merged.
          "defaultProfile":"chosen", "profiles":{"defaults":{"font":{"face":"Cascadia Code","size":12},"cursorShape":"bar"},"list":[
            {"guid":"other","font":{"face":"Wrong Font"}}, {"guid":"chosen","font":{"size":15,"cellHeight":"1.2"},"colorScheme":"Custom","foreground":"#eeeeee",},]},
          "schemes":[{"name":"Custom","background":"#101010","red":"#ff0000","cursorColor":"#ababab"}],
          "commandline":"https://example.com//not-a-comment", /* never imported */
        }"##,
        );
        assert_eq!(prefs.font_family.as_deref(), Some("Cascadia Code"));
        assert_eq!(prefs.line_height, Some(1.2));
        let theme = prefs.theme.unwrap();
        assert_eq!(theme["background"], "#101010");
        assert_eq!(theme["foreground"], "#eeeeee");
        assert_eq!(theme["red"], "#ff0000");
    }
    #[test]
    fn wezterm_reads_literals_and_never_evaluates_lua() {
        let prefs = parse_wezterm("local wezterm = require 'wezterm'\nlocal config = wezterm.config_builder()\nconfig.font = wezterm.font('JetBrains Mono')\nconfig.font_size = 12\nconfig.line_height = 1.25\nconfig.default_cursor_style = 'BlinkingBar'\nconfig.colors.foreground = '#abcdef'\nreturn config");
        assert_eq!(prefs.font_family.as_deref(), Some("JetBrains Mono"));
        assert_eq!(prefs.line_height, Some(1.25));
        assert_eq!(prefs.cursor_blink, Some(true));
        assert_eq!(prefs.cursor_style.as_deref(), Some("bar"));
        assert!(!parse_wezterm("config.font_size = os.execute('touch /tmp/no')\nconfig.font = wezterm.font(os.getenv('SECRET'))").useful());
        assert!(
            !parse_wezterm("if false then\nconfig.font = wezterm.font('Hidden Font')\nend")
                .useful()
        );
        assert!(!parse_wezterm("--[[\nconfig.font = wezterm.font('Hidden Font')\n]]").useful());
    }
    #[test]
    fn hostile_or_unsupported_values_are_ignored() {
        let prefs = parse_lines("font-family = bad; background: url(x)\nfont-size = inf\nadjust-cell-height=-100%\ncursor-style=evil\nforeground=rgb(0,0,0)\nbackground=javascript:bad\npalette=99=#ffffff", Format::Ghostty);
        assert!(!prefs.useful());
        assert!(!parse_alacritty("not valid toml!").useful());
        assert!(!parse_windows("{broken}").useful());
        assert!(font("Font\nInjected").is_none());
        assert!(ratio("9999999").is_none());
        assert!(ratio("NaN").is_none());
    }
    #[test]
    fn candidate_paths_are_fixed_and_never_recursively_scanned() {
        let root = std::env::temp_dir().join("pandacode-terminal-path-fixture");
        let paths = candidates(|key| match key {
            "HOME" => Some(root.join("home")),
            "XDG_CONFIG_HOME" => Some(root.join("config")),
            "LOCALAPPDATA" => Some(root.join("local")),
            _ => None,
        });
        assert!(paths
            .iter()
            .any(|(_, path)| path == &root.join("config/ghostty/config.ghostty")));
        assert!(paths
            .iter()
            .any(|(_, path)| path == &root.join("local/Microsoft/Windows Terminal/settings.json")));
        assert!(paths.len() < 20);
        assert!(candidates(|_| Some(PathBuf::from("relative"))).is_empty());
    }
    #[test]
    fn discovery_uses_newest_usable_file_and_skips_size_only_or_unusable_files() {
        let dir =
            std::env::temp_dir().join(format!("pandacode-terminal-prefs-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let (old, new, size_only, unrelated, oversized) = (
            dir.join("old"),
            dir.join("new"),
            dir.join("size-only"),
            dir.join("unrelated"),
            dir.join("oversized"),
        );
        fs::write(&old, "font-family=JetBrains Mono").unwrap();
        fs::write(&new, "font_family Fira Code").unwrap();
        fs::write(&size_only, "font-size=48").unwrap();
        fs::write(&unrelated, "command=never-run").unwrap();
        fs::write(&oversized, "x".repeat(MAX_CONFIG_BYTES as usize + 1)).unwrap();
        fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10))
            .unwrap();
        fs::File::options()
            .write(true)
            .open(&new)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(20))
            .unwrap();
        let result = discover_paths(vec![
            (Format::Ghostty, old),
            (Format::Kitty, new),
            (Format::Ghostty, size_only),
            (Format::Ghostty, unrelated),
            (Format::Ghostty, oversized),
            (Format::Ghostty, dir.clone()),
        ]);
        assert_eq!(result.source.as_deref(), Some("Kitty"));
        assert_eq!(result.font_family.as_deref(), Some("Fira Code"));
        let encoded = serde_json::to_string(&result).unwrap();
        assert!(!encoded.contains("pandacode-terminal-prefs"));
        assert!(!encoded.contains("command"));
        fs::remove_dir_all(dir).unwrap();
    }
}
