//! Tart's command line: list, create, configure, start, stop, suspend, delete and exec.
//! Every call goes through the bounded process runner except `run`, which stays alive.
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use crate::workspace::{checked, run_process};

const QUICK: Duration = Duration::from_secs(60);
/// Pulling an image can take a long time on a slow connection.
const PULL: Duration = Duration::from_secs(3 * 60 * 60);

/// Tart from PATH, or the app bundle PandaCode's setup installs in ~/Applications.
pub fn binary() -> Option<PathBuf> {
    let bundles = ["Applications/tart.app/Contents/MacOS/tart"];
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .into_iter()
        .flat_map(|home| bundles.map(|bundle| home.join(bundle)))
        .chain([PathBuf::from("/Applications/tart.app/Contents/MacOS/tart")])
        .find(|path| path.is_file())
        .or_else(|| crate::token_saver::find_tool("tart"))
}
fn tart() -> Result<Command, String> {
    let path = binary().ok_or("Tart is not installed. Set up VMs from the VM panel first.")?;
    Ok(Command::new(path))
}
fn output(mut command: Command, timeout: Duration) -> Result<String, String> {
    command.env("TART_NO_AUTO_PRUNE", "1");
    let output = checked(run_process(command, "Tart", 1 << 20, timeout)?).map_err(|error| if error == "Git operation failed" { "Tart failed.".into() } else { error })?;
    Ok(String::from_utf8_lossy(&output.bytes).into_owned())
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Vm {
    pub name: String,
    pub running: bool,
    pub size_gb: u64,
    pub disk_gb: u64,
}
#[derive(Deserialize)]
struct Listed {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Running")]
    running: bool,
    #[serde(rename = "Size", default)]
    size: u64,
    #[serde(rename = "Disk", default)]
    disk: u64,
    #[serde(rename = "Source")]
    source: String,
}
/// Local VMs only; pulled images (OCI entries) are a cache, not machines.
pub fn parse_list(json: &str) -> Result<Vec<Vm>, String> {
    let listed: Vec<Listed> = serde_json::from_str(json).map_err(|e| format!("Unexpected Tart list output: {e}"))?;
    Ok(listed.into_iter().filter(|vm| vm.source == "local").map(|vm| Vm { name: vm.name, running: vm.running, size_gb: vm.size, disk_gb: vm.disk }).collect())
}
pub fn list() -> Result<Vec<Vm>, String> {
    let mut command = tart()?;
    command.args(["list", "--format", "json"]);
    parse_list(&output(command, QUICK)?)
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    pub os: String,
    pub memory_mb: u64,
    pub cpus: u64,
    pub width: u32,
    pub height: u32,
}
pub fn parse_details(json: &str) -> Result<Details, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("Unexpected Tart output: {e}"))?;
    let (width, height) = value["Display"].as_str().and_then(|d| d.split_once('x')).map_or((1024, 768), |(w, h)| (w.parse().unwrap_or(1024), h.parse().unwrap_or(768)));
    Ok(Details {
        os: value["OS"].as_str().unwrap_or("linux").to_string(),
        memory_mb: value["Memory"].as_u64().unwrap_or(0),
        cpus: value["CPU"].as_u64().unwrap_or(0),
        width,
        height,
    })
}
pub fn details(name: &str) -> Result<Details, String> {
    let mut command = tart()?;
    command.args(["get", name, "--format", "json"]);
    parse_details(&output(command, QUICK)?)
}

pub fn clone(image: &str, name: &str) -> Result<(), String> {
    let mut command = tart()?;
    command.args(["clone", image, name]);
    output(command, PULL).map(|_| ())
}
pub fn configure(name: &str, memory_mb: u64, cpus: u64) -> Result<(), String> {
    let mut command = tart()?;
    command.args(["set", name, "--memory", &memory_mb.to_string(), "--cpu", &cpus.to_string()]);
    output(command, QUICK).map(|_| ())
}
pub fn stop(name: &str) -> Result<(), String> {
    let mut command = tart()?;
    command.args(["stop", name, "--timeout", "20"]);
    output(command, QUICK).map(|_| ())
}
/// macOS guests only: frees the VM's memory and resumes in seconds.
pub fn suspend(name: &str) -> Result<(), String> {
    let mut command = tart()?;
    command.args(["suspend", name]);
    output(command, QUICK).map(|_| ())
}
pub fn delete(name: &str) -> Result<(), String> {
    let mut command = tart()?;
    command.args(["delete", name]);
    output(command, QUICK).map(|_| ())
}

/// How a VM is started: headless, with the framework's own VNC server for its screen.
pub fn run_arguments(name: &str, macos: bool, share: Option<&std::path::Path>) -> Vec<String> {
    let mut args = vec!["run".to_string(), name.to_string(), "--no-graphics".into(), "--vnc-experimental".into()];
    if macos {
        args.push("--suspendable".into());
    }
    if let Some(path) = share {
        args.push(format!("--dir=project:{}:ro", path.display()));
    }
    args
}
pub fn spawn_run(args: &[String]) -> Result<Child, String> {
    let mut command = tart()?;
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).env("TART_NO_AUTO_PRUNE", "1");
    command.spawn().map_err(|error| format!("Could not start the VM: {error}"))
}

pub struct Exec {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
}
/// Run a shell command in the guest through the Tart Guest Agent (no SSH needed).
pub fn exec(name: &str, macos: bool, script: &str, timeout: Duration) -> Result<Exec, String> {
    let mut command = tart()?;
    command.args(["exec", name, if macos { "/bin/zsh" } else { "/bin/bash" }, "-lc", script]);
    let out = run_process(command, "Tart", 256 * 1024, timeout)?;
    let stderr = String::from_utf8_lossy(&out.stderr.bytes).into_owned();
    if stderr.contains("guest agent") && !out.status.success() {
        return Err("This VM has no Tart Guest Agent, so commands cannot run in it. Use a Cirrus Labs base image.".into());
    }
    Ok(Exec { code: out.status.code(), stdout: String::from_utf8_lossy(&out.stdout.bytes).into_owned(), stderr, truncated: out.stdout.truncated || out.stderr.truncated })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_local_vms_and_skips_the_image_cache() {
        let json = r#"[{"State":"running","Name":"pandacode-ubuntu","Size":5,"Disk":20,"Running":true,"Source":"local"},{"State":"stopped","Name":"ghcr.io/cirruslabs/ubuntu:latest","Size":5,"Disk":20,"Running":false,"Source":"OCI"}]"#;
        assert_eq!(parse_list(json).unwrap(), vec![Vm { name: "pandacode-ubuntu".into(), running: true, size_gb: 5, disk_gb: 20 }]);
        assert!(parse_list("nope").is_err());
    }
    #[test]
    fn reads_os_memory_and_display() {
        let json = r#"{"Memory":2048,"CPU":2,"Display":"1024x768","Running":true,"OS":"linux"}"#;
        assert_eq!(parse_details(json).unwrap(), Details { os: "linux".into(), memory_mb: 2048, cpus: 2, width: 1024, height: 768 });
    }
    #[test]
    fn runs_headless_and_shares_the_project_read_only() {
        let args = run_arguments("vm", true, Some(std::path::Path::new("/p/app")));
        assert_eq!(&args[..4], ["run", "vm", "--no-graphics", "--vnc-experimental"]);
        assert!(args.contains(&"--suspendable".into()));
        assert!(args.contains(&"--dir=project:/p/app:ro".into()));
        let linux = run_arguments("vm", false, None);
        assert!(!linux.iter().any(|a| a.contains("suspend") || a.contains("--dir")));
    }
}
