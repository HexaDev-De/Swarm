//! AWS desktop screens: an SSM port-forwarding session per instance carries the VNC
//! server (bound to the instance's localhost) to a free local port, and the shared RFB
//! client does the rest. Tunnels close when the instance stops or PandaCode quits.
use std::{
    collections::HashMap,
    net::{SocketAddr, TcpListener, TcpStream},
    process::{Child, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use super::aws::{flavor, Aws};
use crate::vm::{rfb::Rfb, screen};

struct Tunnel {
    child: Child,
    address: SocketAddr,
    rfb: Option<Rfb>,
}

fn tunnels() -> &'static Mutex<HashMap<String, Tunnel>> {
    static TUNNELS: OnceLock<Mutex<HashMap<String, Tunnel>>> = OnceLock::new();
    TUNNELS.get_or_init(Default::default)
}

fn free_port() -> Result<u16, String> {
    TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).map_err(|e| format!("No free local port: {e}"))
}

pub fn password_key(instance: &str) -> String {
    format!("aws_vnc:{instance}")
}

fn open(aws: &Aws, instance: &str, os: &str) -> Result<Tunnel, String> {
    if crate::token_saver::find_tool("session-manager-plugin").is_none() {
        return Err("Seeing AWS desktops needs the Session Manager plugin. Install it with `brew install --cask session-manager-plugin`, then try again.".into());
    }
    let port = free_port()?;
    let parameters = format!("portNumber={},localPortNumber={port}", flavor(os)?.vnc_port);
    let mut command = aws.command(&["ssm", "start-session", "--target", instance, "--document-name", "AWS-StartPortForwardingSession", "--parameters", &parameters])?;
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| format!("Could not open the AWS session: {e}"))?;
    let address: SocketAddr = ([127, 0, 0, 1], port).into();
    let deadline = Instant::now() + Duration::from_secs(40);
    while TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_err() {
        if child.try_wait().ok().flatten().is_some() || Instant::now() > deadline {
            let _ = child.kill();
            return Err("Could not reach the desktop through Systems Manager. If the instance just started, wait a minute; the desktop installs on first boot.".into());
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    Ok(Tunnel { child, address, rfb: None })
}

/// Use an AWS desktop's screen, opening the tunnel and VNC session on first use.
pub fn with_screen<T>(aws: &Aws, instance: &str, os: &str, mut action: impl FnMut(&mut Rfb) -> Result<T, String>) -> Result<T, String> {
    let password = super::secret(&password_key(instance)).ok_or("PandaCode did not create this desktop, so it has no screen password. Use vm_exec for commands.")?;
    let mut map = tunnels().lock().map_err(|_| "Screen state is unavailable.")?;
    for attempt in 0..2 {
        let alive = map.get_mut(instance).is_some_and(|t| t.child.try_wait().ok().flatten().is_none());
        if !alive {
            if let Some(mut old) = map.remove(instance) { let _ = old.child.kill(); }
            map.insert(instance.to_string(), open(aws, instance, os)?);
        }
        let tunnel = map.get_mut(instance).expect("inserted above");
        if tunnel.rfb.is_none() {
            match Rfb::connect(tunnel.address, Some(&password), Duration::from_secs(8)) {
                Ok(rfb) => tunnel.rfb = Some(rfb),
                Err(error) if attempt == 0 => { let _ = tunnel.child.kill(); map.remove(instance); let _ = error; continue; }
                Err(error) => return Err(format!("{error} The desktop may still be installing; try again in a minute.")),
            }
        }
        match action(tunnel.rfb.as_mut().expect("connected above")) {
            Err(error) if attempt == 0 && error.starts_with("VNC connection failed") => { tunnel.rfb = None; }
            result => return result,
        }
    }
    Err("Could not keep a connection to the desktop.".into())
}

pub fn screenshot(aws: &Aws, instance: &str, os: &str) -> Result<screen::Shot, String> {
    with_screen(aws, instance, os, |rfb| screen::screenshot(rfb, 75))
}
pub fn act(aws: &Aws, instance: &str, os: &str, action: &screen::Action) -> Result<String, String> {
    with_screen(aws, instance, os, |rfb| screen::act(rfb, action))
}

/// Close one instance's tunnel (it stopped or was deleted), or all of them on quit.
pub fn close(instance: Option<&str>) {
    if let Ok(mut map) = tunnels().lock() {
        let names: Vec<String> = map.keys().filter(|k| instance.is_none_or(|i| i == k.as_str())).cloned().collect();
        for name in names {
            if let Some(mut tunnel) = map.remove(&name) {
                let _ = tunnel.child.kill();
                let _ = tunnel.child.wait();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn passwords_are_kept_per_instance() {
        assert_eq!(super::password_key("i-0abc"), "aws_vnc:i-0abc");
        assert!(super::free_port().unwrap() > 0);
    }
}
