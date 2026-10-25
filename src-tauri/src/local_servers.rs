use serde::Serialize;
use std::{collections::BTreeMap, net::IpAddr};

const OUTPUT_LIMIT: usize = 1024 * 1024;
const SERVER_LIMIT: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalServer {
    pub port: u16,
    pub process: String,
    pub url: String,
}

/// Lists listeners without connecting to them; opening a URL is always explicit.
#[cfg(unix)]
pub async fn list() -> Result<Vec<LocalServer>, String> {
    let executable = ["/usr/sbin/lsof", "/usr/bin/lsof", "/sbin/lsof", "/bin/lsof"]
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file());
    if let Some(executable) = executable {
        return discovery(
            executable,
            &["-nP", "-iTCP", "-sTCP:LISTEN", "-F0pcfPtTn"],
            true,
        )
        .await
        .map(|bytes| parse(&bytes));
    }
    #[cfg(target_os = "linux")]
    if let Some(executable) = ["/usr/bin/ss", "/usr/sbin/ss", "/bin/ss", "/sbin/ss"]
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file())
    {
        return discovery(executable, &["-H", "-ltnp"], false)
            .await
            .map(|bytes| parse_ss(&bytes));
    }
    Err("Local server discovery requires lsof or, on Linux, ss. You can still enter a URL.".into())
}

#[cfg(windows)]
pub async fn list() -> Result<Vec<LocalServer>, String> {
    let executable = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "C:\\Windows".into())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let script = "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[System.Text.UTF8Encoding]::new(); $listeners=@(Get-NetTCPConnection -State Listen | Select-Object -First 512 | ForEach-Object { $p=Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue; @{address=$_.LocalAddress;port=$_.LocalPort;process=if($p){$p.ProcessName}else{'Process '+$_.OwningProcess}} }); ConvertTo-Json -InputObject $listeners -Compress";
    let bytes = discovery(
        &executable.to_string_lossy(),
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ],
        false,
    )
    .await?;
    parse_windows(&bytes)
}

async fn discovery(executable: &str, args: &[&str], empty_exit: bool) -> Result<Vec<u8>, String> {
    use std::{process::Stdio, time::Duration};
    use tokio::{io::AsyncReadExt, process::Command};
    let mut command = Command::new(executable);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let mut child = command
        .args(args)
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "Could not start local server discovery.")?;
    let stdout = child.stdout.take().ok_or("Server output unavailable.")?;
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let mut bytes = Vec::new();
        stdout
            .take((OUTPUT_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "Could not read local server details.")?;
        if bytes.len() > OUTPUT_LIMIT {
            return Err("Local server discovery exceeded its output limit.".into());
        }
        let status = child
            .wait()
            .await
            .map_err(|_| "Could not finish local server discovery.")?;
        if !(status.success() || empty_exit && status.code() == Some(1)) {
            return Err("Local server discovery is unavailable. You can still enter a URL.".into());
        }
        Ok(bytes)
    })
    .await
    .unwrap_or_else(|_| Err("Local server discovery timed out. Try refreshing.".into()));
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
    result
}

#[cfg(any(target_os = "linux", test))]
fn parse_ss(bytes: &[u8]) -> Vec<LocalServer> {
    let mut servers = BTreeMap::new();
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(OUTPUT_LIMIT)]);
    for line in text.lines().take(4096) {
        let mut fields = line.split_whitespace();
        if fields.next() != Some("LISTEN") {
            continue;
        }
        let Some(address) = fields.nth(2) else {
            continue;
        };
        let family = if address.starts_with('[') || address.matches(':').count() > 1 {
            "IPv6"
        } else {
            "IPv4"
        };
        let name = line
            .split_once("users:((\"")
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(name, _)| name)
            .unwrap_or("Local server");
        let process: String = name.chars().filter(|c| !c.is_control()).take(128).collect();
        retain_socket(
            &mut servers,
            &process,
            &Socket {
                active: true,
                tcp: true,
                listening: true,
                family,
                address,
            },
        );
    }
    servers.into_values().map(|(_, server)| server).collect()
}

#[cfg(any(windows, test))]
fn parse_windows(bytes: &[u8]) -> Result<Vec<LocalServer>, String> {
    if bytes.len() > OUTPUT_LIMIT {
        return Err("Local server discovery exceeded its output limit.".into());
    }
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }
    #[derive(serde::Deserialize)]
    struct Listener {
        address: String,
        port: u16,
        process: String,
    }
    let listeners: Vec<Listener> = serde_json::from_slice(bytes)
        .map_err(|_| "Local server discovery returned invalid data.")?;
    let mut servers = BTreeMap::new();
    for listener in listeners.into_iter().take(SERVER_LIMIT) {
        let family = if listener.address.contains(':') {
            "IPv6"
        } else {
            "IPv4"
        };
        let address = format!("{}:{}", listener.address, listener.port);
        let process: String = listener
            .process
            .chars()
            .filter(|c| !c.is_control())
            .take(128)
            .collect();
        retain_socket(
            &mut servers,
            &process,
            &Socket {
                active: true,
                tcp: true,
                listening: true,
                family,
                address: &address,
            },
        );
    }
    Ok(servers.into_values().map(|(_, server)| server).collect())
}

#[derive(Default)]
struct Socket<'a> {
    active: bool,
    family: &'a str,
    tcp: bool,
    listening: bool,
    address: &'a str,
}

fn loopback_url(address: &str, family: &str) -> Option<(u16, String, u8)> {
    let (host, port) = address.rsplit_once(':')?;
    let port = port.parse::<u16>().ok().filter(|port| *port != 0)?;
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let (host, rank) = if host == "*" {
        match family {
            "IPv4" => ("localhost".to_owned(), 0),
            "IPv6" => ("[::1]".to_owned(), 1),
            _ => return None,
        }
    } else {
        match host.parse::<IpAddr>().ok()? {
            IpAddr::V4(ip) if ip.is_unspecified() || ip == std::net::Ipv4Addr::LOCALHOST => {
                ("localhost".to_owned(), 0)
            }
            IpAddr::V4(ip) if ip.is_loopback() => (ip.to_string(), 2),
            IpAddr::V6(ip) if ip.is_unspecified() || ip.is_loopback() => ("[::1]".to_owned(), 1),
            IpAddr::V6(ip) => {
                let ipv4 = ip.to_ipv4_mapped().filter(|ip| ip.is_loopback())?;
                (format!("[::ffff:{ipv4}]"), 2)
            }
            _ => return None,
        }
    };
    Some((port, format!("http://{host}:{port}"), rank))
}

fn retain_socket(
    servers: &mut BTreeMap<u16, (u8, LocalServer)>,
    process: &str,
    socket: &Socket<'_>,
) {
    if process.is_empty()
        || !socket.active
        || !socket.tcp
        || !socket.listening
        || !matches!(socket.family, "IPv4" | "IPv6")
    {
        return;
    }
    let Some((port, url, rank)) = loopback_url(socket.address, socket.family) else {
        return;
    };
    // Prefer IPv4 localhost if a process listens on both families. Ties are
    // deterministic even when multiple processes share the same listening port.
    if servers.get(&port).is_some_and(|(old_rank, old)| {
        (*old_rank, old.process.as_str(), old.url.as_str()) <= (rank, process, url.as_str())
    }) {
        return;
    }
    servers.insert(
        port,
        (
            rank,
            LocalServer {
                port,
                process: process.into(),
                url,
            },
        ),
    );
    if servers.len() > SERVER_LIMIT {
        servers.pop_last();
    }
}

#[cfg(any(unix, test))]
fn parse(bytes: &[u8]) -> Vec<LocalServer> {
    let mut servers = BTreeMap::new();
    let mut process = String::new();
    let mut valid_process = false;
    let mut socket = Socket::default();
    // With -F0, each field ends in NUL and each process/file record in a newline.
    // Parsing field markers avoids depending on column widths or process spaces.
    for field in bytes.split(|byte| *byte == 0) {
        let Ok(field) = std::str::from_utf8(field) else {
            continue;
        };
        let field = field.trim_start_matches(['\r', '\n']);
        let Some((&kind, value)) = field.as_bytes().split_first() else {
            continue;
        };
        let value = std::str::from_utf8(value).unwrap_or_default();
        match kind {
            b'p' => {
                retain_socket(&mut servers, &process, &socket);
                socket = Socket::default();
                process.clear();
                valid_process = value.parse::<u32>().is_ok_and(|pid| pid != 0);
            }
            b'c' if valid_process && !socket.active => {
                process = value
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(256)
                    .collect();
            }
            b'f' => {
                retain_socket(&mut servers, &process, &socket);
                socket = Socket {
                    active: valid_process && value.parse::<u32>().is_ok(),
                    ..Socket::default()
                };
            }
            b't' => socket.family = value,
            b'P' => socket.tcp = value == "TCP",
            b'T' if value.starts_with("ST=") => socket.listening = value == "ST=LISTEN",
            b'n' => socket.address = if value.len() <= 128 { value } else { "" },
            _ => {}
        }
    }
    retain_socket(&mut servers, &process, &socket);
    servers.into_values().map(|(_, server)| server).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_ss_without_requiring_process_visibility() {
        let rows = parse_ss(b"LISTEN 0 128 127.0.0.1:3000 0.0.0.0:* users:((\"node\",pid=42,fd=3))\nLISTEN 0 128 [::1]:4000 [::]:*\nLISTEN 0 128 192.168.1.5:9000 0.0.0.0:*\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].process, "node");
        assert_eq!(rows[1].url, "http://[::1]:4000");
        assert_eq!(rows[1].process, "Local server");
    }

    #[test]
    fn windows_listeners_are_bounded_deduplicated_and_loopback_only() {
        let rows = parse_windows(br#"[{"address":"0.0.0.0","port":3000,"process":"node"},{"address":"::","port":3000,"process":"node"},{"address":"::1","port":4000,"process":"python"},{"address":"192.168.1.5","port":9000,"process":"remote"}]"#).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].url, "http://localhost:3000");
        assert_eq!(rows[1].process, "python");
        assert!(parse_windows(b"not json").is_err());
        assert!(parse_windows(b"[]").unwrap().is_empty());
    }

    #[test]
    fn parses_macos_lsof_records_and_preserves_process_names() {
        // Field order and record separators captured from macOS lsof -F0pcfPtTn.
        let servers = parse(b"p50396\0cDiscord Helper (Renderer)\0\nf38\0tIPv4\0PTCP\0n127.0.0.1:6463\0TST=LISTEN\0TQR=0\0TQS=0\0\np91937\0cnode\0\nf13\0tIPv6\0PTCP\0n*:3000\0TST=LISTEN\0TQR=0\0TQS=0\0\n");
        assert_eq!(
            servers,
            vec![
                LocalServer {
                    port: 3000,
                    process: "node".into(),
                    url: "http://[::1]:3000".into()
                },
                LocalServer {
                    port: 6463,
                    process: "Discord Helper (Renderer)".into(),
                    url: "http://localhost:6463".into()
                },
            ]
        );
    }

    #[test]
    fn deduplicates_wildcards_and_both_loopback_families() {
        let servers = parse(b"p20\0cnode\0\nf4\0tIPv6\0PTCP\0n[::1]:3000\0TST=LISTEN\0\nf5\0tIPv4\0PTCP\0n*:3000\0TST=LISTEN\0\nf6\0tIPv4\0PTCP\0n0.0.0.0:4000\0TST=LISTEN\0\nf7\0tIPv6\0PTCP\0n[::]:4000\0TST=LISTEN\0\n");
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].url, "http://localhost:3000");
        assert_eq!(servers[1].url, "http://localhost:4000");
    }

    #[test]
    fn rejects_remote_addresses_and_irrelevant_socket_states() {
        let servers = parse(b"p20\0cnode\0\nf4\0tIPv4\0PTCP\0n192.168.1.2:3000\0TST=LISTEN\0\nf5\0tIPv6\0PTCP\0n[fe80::1]:3001\0TST=LISTEN\0\nf6\0tIPv4\0PTCP\0n127.0.0.1:3002->127.0.0.1:4000\0TST=ESTABLISHED\0\nf7\0tIPv4\0PUDP\0n*:3003\0TST=LISTEN\0\nf8\0tunix\0PTCP\0n*:3004\0TST=LISTEN\0\nf9\0tIPv4\0PTCP\0n127.0.0.1:3005\0\n");
        assert!(servers.is_empty());
    }

    #[test]
    fn ignores_malformed_records_without_reusing_previous_process() {
        let servers = parse(b"garbage\0p20\0cnode\0\nf4\0tIPv4\0PTCP\0n127.0.0.1:0\0TST=LISTEN\0\nf5\0tIPv4\0PTCP\0n127.0.0.1:65536\0TST=LISTEN\0\nf6\0tIPv4\0PTCP\0nlocalhost:3000\0TST=LISTEN\0\nf7\0tIPv4\0PTCP\0n127.0.0.1:nope\0TST=LISTEN\0\npbad\0cwrong\0\nf8\0tIPv4\0PTCP\0n*:3000\0TST=LISTEN\0\np30\0\nf9\0tIPv4\0PTCP\0n*:4000\0TST=LISTEN\0\n");
        assert!(servers.is_empty());
    }

    #[test]
    fn keeps_other_loopback_addresses_reachable() {
        assert_eq!(
            loopback_url("127.0.0.2:1234", "IPv4"),
            Some((1234, "http://127.0.0.2:1234".into(), 2))
        );
        assert_eq!(
            loopback_url("[::ffff:127.0.0.1]:1234", "IPv6"),
            Some((1234, "http://[::ffff:127.0.0.1]:1234".into(), 2))
        );
        assert!(loopback_url("[::ffff:10.0.0.1]:1234", "IPv6").is_none());
    }

    #[test]
    fn bounds_results_and_sorts_independently_of_discovery_order() {
        let mut records = String::from("p1\0cnode\0\n");
        for port in (1024..1700).rev() {
            records.push_str(&format!("f{port}\0tIPv4\0PTCP\0n*:{port}\0TST=LISTEN\0\n"));
        }
        let servers = parse(records.as_bytes());
        assert_eq!(servers.len(), SERVER_LIMIT);
        assert_eq!(servers.first().unwrap().port, 1024);
        assert_eq!(servers.last().unwrap().port, 1024 + SERVER_LIMIT as u16 - 1);
    }
}
