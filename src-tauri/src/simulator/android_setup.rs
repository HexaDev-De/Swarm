//! One-click Android emulator setup through Homebrew, for Macs without Android Studio:
//! a JDK, Google's command-line tools, adb, the emulator, one system image and a Pixel
//! AVD. Every step is a documented CLI; nothing needs an administrator password.
use serde::Serialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tauri::{AppHandle, Emitter};

use crate::token_saver::find_tool;
use crate::workspace::{checked, run_process};

static RUNNING: AtomicBool = AtomicBool::new(false);
const LONG: Duration = Duration::from_secs(60 * 60);
pub const API: u32 = 35;
pub const AVD: &str = "Pixel_9_API_35";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress { step: usize, total: usize, label: String, done: bool, error: Option<String> }

fn emit(app: &AppHandle, step: usize, total: usize, label: &str) {
    let _ = app.emit("android-setup", Progress { step, total, label: label.into(), done: false, error: None });
}
fn brew(args: &[&str]) -> Result<(), String> {
    let brew = find_tool("brew").ok_or("Homebrew is needed to set up the Android emulator. Install it from brew.sh, or install Android Studio.")?;
    let mut command = Command::new(brew);
    command.args(args).env("HOMEBREW_NO_AUTO_UPDATE", "1").env("HOMEBREW_NO_INSTALL_CLEANUP", "1").env("HOMEBREW_NO_ENV_HINTS", "1").env("NONINTERACTIVE", "1");
    checked(run_process(command, "Homebrew", 2 * 1024 * 1024, LONG)?).map(|_| ()).map_err(|error| format!("Homebrew: {}", error.lines().last().unwrap_or(&error)))
}
fn brew_prefix(formula: &str) -> Option<PathBuf> {
    let mut command = Command::new(find_tool("brew")?);
    command.args(["--prefix", formula]);
    let output = checked(run_process(command, "Homebrew", 4096, Duration::from_secs(30)).ok()?).ok()?;
    Some(PathBuf::from(String::from_utf8_lossy(&output.bytes).trim()))
}
fn homebrew_prefix() -> PathBuf {
    find_tool("brew").and_then(|brew| brew.parent()?.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("/opt/homebrew"))
}
/// A JDK for sdkmanager: the system's if it answers, else Homebrew's keg-only openjdk@21.
fn java_home() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("JAVA_HOME").map(PathBuf::from).filter(|home| home.join("bin/java").is_file()) {
        return Some(home);
    }
    let homebrew = brew_prefix("openjdk@21")?.join("libexec/openjdk.jdk/Contents/Home");
    homebrew.join("bin/java").is_file().then_some(homebrew)
}
/// sdkmanager and avdmanager ask for licenses and profiles on stdin; this answers them.
fn tool(name: &str, root: &Path, java: &Path, args: &[&str], answers: &str) -> Result<String, String> {
    let path = find_tool(name).or_else(|| Some(root.join("cmdline-tools/latest/bin").join(name)).filter(|p| p.is_file())).ok_or_else(|| format!("{name} was not found after installing the command-line tools."))?;
    let mut child = Command::new(path)
        .args(args)
        .env("JAVA_HOME", java)
        .env("ANDROID_HOME", root)
        .env("ANDROID_SDK_ROOT", root)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start {name}: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(answers.as_bytes());
    }
    let output = child.wait_with_output().map_err(|error| error.to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if output.status.success() { Ok(text) } else { Err(format!("{name} failed: {}", text.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("unknown error"))) }
}

/// The system image for this Mac's architecture.
pub fn system_image() -> String {
    let arch = if cfg!(target_arch = "aarch64") { "arm64-v8a" } else { "x86_64" };
    format!("system-images;android-{API};google_apis;{arch}")
}

fn run(app: &AppHandle) -> Result<String, String> {
    if !cfg!(target_os = "macos") {
        return Err("One-click setup is for macOS. Install Android Studio to get the emulator.".into());
    }
    let total = 5;
    emit(app, 1, total, "Installing Java (openjdk@21)…");
    let java = match java_home() { Some(java) => java, None => { brew(&["install", "openjdk@21"])?; java_home().ok_or("Java was installed but could not be found.")? } };
    emit(app, 2, total, "Installing Android command-line tools…");
    if find_tool("sdkmanager").is_none() {
        brew(&["install", "--cask", "android-commandlinetools"])?;
    }
    // The cask's SDK lives under Homebrew's prefix (/opt/homebrew or /usr/local).
    let root = super::android::sdk_root().unwrap_or_else(|| homebrew_prefix().join("share/android-commandlinetools"));
    let sdk = format!("--sdk_root={}", root.display());
    emit(app, 3, total, "Accepting the Android SDK licenses…");
    tool("sdkmanager", &root, &java, &[&sdk, "--licenses"], &"y\n".repeat(40))?;
    emit(app, 4, total, &format!("Downloading the emulator and Android {} (about 2 GB)…", super::android::android_version(API)));
    tool("sdkmanager", &root, &java, &[&sdk, "platform-tools", "emulator", &system_image()], &"y\n".repeat(10))?;
    emit(app, 5, total, "Creating a Pixel 9 emulator…");
    let image = system_image();
    let created = tool("avdmanager", &root, &java, &["create", "avd", "-n", AVD, "-k", &image, "-d", "pixel_9", "--force"], "no\n")
        .or_else(|_| tool("avdmanager", &root, &java, &["create", "avd", "-n", AVD, "-k", &image, "-d", "pixel_7", "--force"], "no\n"));
    created?;
    Ok(AVD.into())
}

/// Set up the Android emulator, reporting each step as an `android-setup` event.
#[tauri::command]
pub async fn android_setup(app: AppHandle) -> crate::Result<String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("Android setup is already running.".into());
    }
    let events = app.clone();
    let result = crate::blocking(move || run(&app)).await;
    RUNNING.store(false, Ordering::SeqCst);
    let _ = events.emit("android-setup", Progress { step: 5, total: 5, label: match &result { Ok(_) => "Android emulator ready".into(), Err(_) => "Android setup stopped".into() }, done: true, error: result.as_ref().err().cloned() });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_image_matches_this_mac() {
        let image = system_image();
        assert!(image.starts_with("system-images;android-35;google_apis;"));
        assert!(image.ends_with(if cfg!(target_arch = "aarch64") { "arm64-v8a" } else { "x86_64" }));
        assert!(super::super::android::valid_name(AVD));
    }
}
