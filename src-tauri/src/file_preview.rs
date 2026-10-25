//! Workspace-scoped file links and an on-demand, unprivileged browser preview.
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
};

type Result<T> = std::result::Result<T, String>;
const MAX_FILE: u64 = 32 * 1024 * 1024;
const IDLE: Duration = Duration::from_secs(30 * 60);
const MAX_SERVERS: usize = 8;
const MAX_CONNECTIONS: usize = 12;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedFile {
    pub input: String,
    pub path: String,
    pub previewable: bool,
}
#[derive(Debug, Serialize)]
pub struct FilePreview {
    pub url: String,
    pub path: String,
    pub name: String,
}

fn public_path(path: &Path) -> bool {
    path.components().all(|part| match part {
        Component::Normal(name) => name
            .to_str()
            .is_some_and(|s| !s.starts_with('.') && !s.contains(['\\', ':', '\0'])),
        Component::CurDir => true,
        _ => false,
    }) && !matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "pem" | "key" | "p12" | "pfx" | "keystore"
    )
}

/// Revalidate both the lexical path and canonical target. A harmless-looking link
/// cannot expose a dotfile, another checkout, a FIFO, or a symlink to either.
fn checked_file(root: &Path, relative: &Path) -> Result<PathBuf> {
    if !public_path(relative) {
        return Err("This file cannot be previewed.".into());
    }
    let target = root
        .join(relative)
        .canonicalize()
        .map_err(|_| "File is no longer available.")?;
    let resolved = target
        .strip_prefix(root)
        .map_err(|_| "File must stay inside the workspace.")?;
    if !public_path(resolved) || !target.is_file() {
        return Err("This file cannot be previewed.".into());
    }
    Ok(target)
}

fn relative_string(path: &Path) -> Option<String> {
    path.components()
        .filter(|part| !matches!(part, Component::CurDir))
        .map(|part| part.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .map(|v| v.join("/"))
}

fn trim_location(input: &str) -> &str {
    let mut value = input;
    if let Some((path, suffix)) = value.rsplit_once('#') {
        if suffix.starts_with('L')
            && suffix
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, 'L' | 'C' | '-'))
        {
            value = path;
        }
    }
    for _ in 0..2 {
        if let Some((path, suffix)) = value.rsplit_once(':') {
            if !suffix.is_empty() && suffix.bytes().all(|c| c.is_ascii_digit()) {
                value = path;
            }
        }
    }
    value
}

fn candidate(root: &Path, input: &str) -> Option<PathBuf> {
    if input.is_empty() || input.len() > 4096 || input.chars().any(char::is_control) {
        return None;
    }
    let value = input.trim().trim_matches('`');
    let path = if value.starts_with("file://") {
        let url = tauri::Url::parse(value).ok()?;
        if url
            .host_str()
            .is_some_and(|host| !host.is_empty() && host != "localhost")
        {
            return None;
        }
        url.to_file_path().ok()?
    } else {
        if value.contains("://") {
            return None;
        }
        PathBuf::from(value)
    };
    if path.is_absolute() {
        // Absolute links may use an OS alias such as /tmp versus /private/tmp.
        let target = path.canonicalize().ok()?;
        return target.strip_prefix(root).ok().map(Path::to_path_buf);
    }
    public_path(&path).then_some(path)
}

pub fn resolve_files(root: &Path, paths: Vec<String>) -> Result<Vec<ResolvedFile>> {
    if paths.len() > 128 {
        return Err("Too many file references.".into());
    }
    let root = root
        .canonicalize()
        .map_err(|_| "Workspace is unavailable.")?;
    let mut resolved = Vec::new();
    let mut pending = Vec::new();
    for (position, input) in paths.into_iter().enumerate() {
        let mut found = None;
        let trimmed = trim_location(input.trim().trim_matches('`'));
        let decoded = decode_path(trimmed);
        for value in [
            input.as_str(),
            trimmed,
            decoded.as_deref().unwrap_or(trimmed),
        ] {
            if let Some(path) = candidate(&root, value) {
                if checked_file(&root, &path).is_ok() {
                    found = relative_string(&path);
                    break;
                }
            }
        }
        if let Some(path) = found {
            resolved.push((position, input, path));
        } else if let Some(path) = candidate(&root, trim_location(input.trim().trim_matches('`'))) {
            if path.components().count() == 1 {
                pending.push((position, input, path));
            }
        }
    }
    if !pending.is_empty() {
        // One bounded walk for an entire streamed message. Never claim a basename
        // is unique after an incomplete scan; explicit paths still work in huge repos.
        let mut names: HashMap<String, Option<String>> = HashMap::new();
        let start = Instant::now();
        let mut complete = true;
        let walk = ignore::WalkBuilder::new(&root)
            .hidden(true)
            .follow_links(false)
            .filter_entry(|entry| {
                entry.depth() == 0
                    || !crate::workspace::excluded_entry(
                        &entry.file_name().to_string_lossy(),
                        entry.file_type().is_some_and(|t| t.is_dir()),
                    )
            })
            .build();
        for (count, entry) in walk.enumerate() {
            if count >= 20_000 || start.elapsed() > Duration::from_millis(120) {
                complete = false;
                break;
            }
            let Ok(entry) = entry else {
                complete = false;
                break;
            };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let Some(name) = entry.file_name().to_str() else {
                continue;
            };
            if !pending.iter().any(|(_, _, path)| path.as_os_str() == name) {
                continue;
            }
            let Ok(relative) = entry.path().strip_prefix(&root) else {
                continue;
            };
            if checked_file(&root, relative).is_err() {
                continue;
            }
            if let Some(path) = relative_string(relative) {
                names
                    .entry(name.to_owned())
                    .and_modify(|value| *value = None)
                    .or_insert(Some(path));
            }
        }
        if complete {
            for (position, input, name) in pending {
                if let Some(Some(path)) = name.to_str().and_then(|name| names.get(name)) {
                    resolved.push((position, input, path.clone()));
                }
            }
        }
    }
    resolved.sort_unstable_by_key(|entry| entry.0);
    Ok(resolved
        .into_iter()
        .map(|(_, input, path)| ResolvedFile {
            previewable: previewable(&path),
            input,
            path,
        })
        .collect())
}

fn previewable(path: &str) -> bool {
    matches!(
        extension(Path::new(path)).as_str(),
        "html"
            | "htm"
            | "svg"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "avif"
            | "ico"
            | "bmp"
            | "pdf"
    )
}
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}
fn mime(path: &Path) -> Option<&'static str> {
    Some(match extension(path).as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "txt" | "md" | "csv" | "log" | "xml" => "text/plain; charset=utf-8",
        _ => return None,
    })
}

struct Server {
    root: PathBuf,
    origin: String,
    token: String,
    stop: watch::Sender<bool>,
    touched: Instant,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}
#[derive(Default)]
pub struct FilePreviews {
    servers: Mutex<VecDeque<Server>>,
}
impl FilePreviews {
    /// Called only by the trusted app's browser navigation command. A recreated
    /// private webview has no preview cookie, even though the saved URL is clean.
    /// Never authorize guest-initiated navigation or an unrelated localhost server.
    pub(crate) fn navigation_url(&self, url: &tauri::Url) -> Result<tauri::Url> {
        let mut servers = self.servers.lock().map_err(|_| "File preview is unavailable.")?;
        let Some(server) = servers.iter_mut().find(|server| {
            server.origin == url.origin().ascii_serialization() && !server.stop.is_closed()
        }) else {
            return Ok(url.clone());
        };
        server.touched = Instant::now();
        let mut navigation = url.clone();
        if !url.path().starts_with("/__pandacode_preview/") {
            navigation.set_path(&format!("/__pandacode_preview/{}{}", server.token, url.path()));
        }
        Ok(navigation)
    }

    pub fn prepare(&self, root: &Path, path: &str) -> Result<FilePreview> {
        let root = root
            .canonicalize()
            .map_err(|_| "Workspace is unavailable.")?;
        let file = resolve_files(&root, vec![path.to_owned()])?
            .into_iter()
            .next()
            .ok_or("File is no longer available.")?;
        if !file.previewable {
            return Err("This file does not have a browser preview.".into());
        }
        let target = checked_file(&root, Path::new(&file.path))?;
        if fs::metadata(target)
            .map_err(|_| "File is no longer available.")?
            .len()
            > MAX_FILE
        {
            return Err("Browser previews support files up to 32 MiB.".into());
        }
        let mut servers = self
            .servers
            .lock()
            .map_err(|_| "File preview is unavailable.")?;
        servers.retain(|server| server.touched.elapsed() < IDLE && !server.stop.is_closed());
        let mut server =
            if let Some(position) = servers.iter().position(|server| server.root == root) {
                servers.remove(position).unwrap()
            } else {
                if servers.len() >= MAX_SERVERS {
                    servers.pop_front();
                }
                let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
                    .map_err(|_| "Could not start local file preview.")?;
                listener
                    .set_nonblocking(true)
                    .map_err(|_| "Could not start local file preview.")?;
                let port = listener
                    .local_addr()
                    .map_err(|_| "Could not start local file preview.")?
                    .port();
                let listener = TcpListener::from_std(listener)
                    .map_err(|_| "Could not start local file preview.")?;
                let token = uuid::Uuid::new_v4().simple().to_string();
                let cookie = format!("pandacode_preview_{port}={token}");
                let origin = format!("http://127.0.0.1:{port}");
                let (stop, receiver) = watch::channel(false);
                let access = Arc::new(Access {
                    root: root.clone(),
                    origin: origin.clone(),
                    token: token.clone(),
                    cookie: cookie.clone(),
                });
                tokio::spawn(serve(listener, receiver, access));
                Server {
                    root: root.clone(),
                    origin,
                    token,
                    stop,
                    touched: Instant::now(),
                }
            };
        server.touched = Instant::now();
        let mut url = tauri::Url::parse(&server.origin).map_err(|_| "Invalid preview URL.")?;
        url.path_segments_mut()
            .map_err(|_| "Invalid preview URL.")?
            .extend(["__pandacode_preview", &server.token])
            .extend(file.path.split('/'));
        let result = FilePreview {
            url: url.to_string(),
            name: file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&file.path)
                .to_owned(),
            path: file.path,
        };
        servers.push_back(server);
        Ok(result)
    }
    pub fn shutdown(&self) {
        if let Ok(mut servers) = self.servers.lock() {
            servers.clear();
        }
    }
}
struct Access {
    root: PathBuf,
    origin: String,
    token: String,
    cookie: String,
}
async fn serve(listener: TcpListener, mut stop: watch::Receiver<bool>, access: Arc<Access>) {
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            _ = tokio::time::sleep(IDLE) => break,
            Some(_) = connections.join_next(), if !connections.is_empty() => {},
            connection = listener.accept() => {
                let Ok((stream, peer)) = connection else { break; };
                if !peer.ip().is_loopback() || connections.len() >= MAX_CONNECTIONS { drop(stream); continue; }
                let access = access.clone();
                connections.spawn(async move { let _ = tokio::time::timeout(Duration::from_secs(30), handle(stream, &access)).await; });
            }
        }
    }
    connections.abort_all();
}

fn decode_path(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut position = 0;
    while position < bytes.len() {
        if bytes[position] == b'%' {
            let a = *bytes.get(position + 1)? as char;
            let b = *bytes.get(position + 2)? as char;
            result.push((a.to_digit(16)? * 16 + b.to_digit(16)?) as u8);
            position += 3;
        } else {
            result.push(bytes[position]);
            position += 1;
        }
    }
    let result = String::from_utf8(result).ok()?;
    (!result.chars().any(char::is_control) && !result.contains('\\')).then_some(result)
}

struct Request {
    path: String,
    head: bool,
    headers: HashMap<String, String>,
}
fn parse_request(raw: &[u8]) -> Option<Request> {
    let raw = std::str::from_utf8(raw).ok()?;
    let mut lines = raw.split("\r\n");
    let mut parts = lines.next()?.split(' ');
    let method = parts.next()?;
    let path = parts.next()?;
    let protocol = parts.next()?;
    if !matches!(method, "GET" | "HEAD")
        || !matches!(protocol, "HTTP/1.1" | "HTTP/1.0")
        || parts.next().is_some()
        || !path.starts_with('/')
        || path.starts_with("//")
    {
        return None;
    }
    let mut headers = HashMap::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':')?;
        let name = name.to_ascii_lowercase();
        if name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || headers.insert(name, value.trim().to_owned()).is_some()
        {
            return None;
        }
    }
    Some(Request {
        path: path.split(['?', '#']).next()?.to_owned(),
        head: method == "HEAD",
        headers,
    })
}
fn authorized(request: &Request, access: &Access) -> bool {
    if request
        .headers
        .get("host")
        .is_none_or(|v| v != access.origin.trim_start_matches("http://"))
    {
        return false;
    }
    if request
        .headers
        .get("origin")
        .is_some_and(|v| v != &access.origin)
    {
        return false;
    }
    if request
        .headers
        .get("sec-fetch-site")
        .is_some_and(|v| !matches!(v.as_str(), "none" | "same-origin"))
    {
        return false;
    }
    if request
        .headers
        .get("referer")
        .is_some_and(|v| !v.starts_with(&format!("{}/", access.origin)))
    {
        return false;
    }
    true
}

enum Reply {
    Redirect(String),
    Bootstrap(String, String),
    File(tokio::fs::File, &'static str, u64),
}

async fn route(request: &Request, access: &Access) -> std::result::Result<Reply, u16> {
    if !authorized(request, access) {
        return Err(403);
    }
    let prefix = format!("/__pandacode_preview/{}/", access.token);
    if let Some(path) = request.path.strip_prefix(&prefix) {
        if request
            .headers
            .get("sec-fetch-dest")
            .is_some_and(|v| v != "document")
            || request
                .headers
                .get("sec-fetch-mode")
                .is_some_and(|v| v != "navigate")
        {
            return Err(403);
        }
        let mut decoded = decode_path(path).ok_or(404u16)?;
        let mut relative = PathBuf::from(&decoded);
        if access.root.join(&relative).is_dir() {
            relative.push("index.html");
            if !decoded.is_empty() && !decoded.ends_with('/') {
                decoded.push('/');
            }
        }
        checked_file(&access.root, &relative).map_err(|_| 404u16)?;
        // Commit this origin before navigating again: WebKit drops a Strict cookie
        // during an immediate HTTP redirect after leaving an unrelated site.
        // This tiny page keeps Strict protection and clean relative asset URLs.
        let mut destination = tauri::Url::parse(&access.origin).map_err(|_| 404u16)?;
        destination
            .path_segments_mut()
            .map_err(|_| 404u16)?
            .extend(decoded.split('/'));
        let encoded = serde_json::to_string(destination.path()).map_err(|_| 404u16)?;
        let body = format!(
            "<!doctype html><meta charset=utf-8><script>location.replace({encoded} + location.search + location.hash)</script>"
        );
        return Ok(Reply::Bootstrap(
            format!(
                "Set-Cookie: {}; Path=/; HttpOnly; SameSite=Strict\r\n",
                access.cookie
            ),
            body,
        ));
    }
    if !request.headers.get("cookie").is_some_and(|cookies| {
        cookies
            .split(';')
            .any(|cookie| cookie.trim() == access.cookie)
    }) {
        return Err(403);
    }
    let relative = decode_path(request.path.trim_start_matches('/')).ok_or(404u16)?;
    let mut relative = PathBuf::from(relative);
    if !public_path(&relative) {
        return Err(404);
    }
    if access.root.join(&relative).is_dir() {
        relative.push("index.html");
        checked_file(&access.root, &relative).map_err(|_| 404u16)?;
        if !request.path.ends_with('/') {
            return Ok(Reply::Redirect(format!("Location: {}/\r\n", request.path)));
        }
    }
    let path = checked_file(&access.root, &relative).map_err(|_| 404u16)?;
    let content_type = mime(&path).ok_or(404u16)?;
    let file = tokio::fs::File::open(path).await.map_err(|_| 404u16)?;
    let metadata = file.metadata().await.map_err(|_| 404u16)?;
    if !metadata.is_file() {
        return Err(404);
    }
    if metadata.len() > MAX_FILE {
        return Err(413);
    }
    Ok(Reply::File(file, content_type, metadata.len()))
}

async fn handle(mut stream: TcpStream, access: &Access) -> std::io::Result<()> {
    let mut header = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(5), async {
        let mut bytes = [0; 2048];
        loop {
            let size = stream.read(&mut bytes).await?;
            if size == 0 {
                return Ok::<_, std::io::Error>(false);
            }
            header.extend_from_slice(&bytes[..size]);
            if header.len() > 16 * 1024 {
                return Ok(false);
            }
            if header.windows(4).any(|w| w == b"\r\n\r\n") {
                return Ok(true);
            }
        }
    })
    .await;
    let request = matches!(read, Ok(Ok(true)))
        .then(|| parse_request(&header))
        .flatten();
    let head = request.as_ref().is_some_and(|r| r.head);
    let reply = match request {
        Some(request) => route(&request, access).await,
        None => Err(403),
    };
    let (code, mime, length, extra) = match &reply {
        Ok(Reply::Redirect(extra)) => (303, "text/plain", 0, extra.as_str()),
        Ok(Reply::Bootstrap(extra, body)) => (
            200,
            "text/html; charset=utf-8",
            body.len() as u64,
            extra.as_str(),
        ),
        Ok(Reply::File(_, mime, length)) => (200, *mime, *length, ""),
        Err(code) => (*code, "text/plain", 0, ""),
    };
    let reason = match code {
        200 => "OK",
        303 => "See Other",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Content Too Large",
        _ => "Bad Request",
    };
    let headers = format!("HTTP/1.1 {code} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {length}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nCross-Origin-Resource-Policy: same-origin\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: frame-ancestors 'none'\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n{extra}\r\n");
    stream.write_all(headers.as_bytes()).await?;
    if !head {
        if let Ok(Reply::Bootstrap(_, body)) = &reply {
            stream.write_all(body.as_bytes()).await?;
        }
        if let Ok(Reply::File(file, _, length)) = reply {
            // Fixed-buffer streaming keeps a large preview out of the UI process's
            // heap; taking the measured length also bounds a concurrently growing file.
            tokio::io::copy(&mut file.take(length), &mut stream).await?;
        }
    }
    stream.shutdown().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("pandacode-file-preview-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
        fn write(&self, path: &str, text: &str) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn resolves_real_files_and_normalizes_response_links_without_guessing_ambiguous_names() {
        let f = Fixture::new();
        f.write("web/hello world.html", "<h1>Hello</h1>");
        f.write("src/main.rs", "fn main() {}");
        f.write("a/duplicate.svg", "<svg/>");
        f.write("b/duplicate.svg", "<svg/>");
        f.write(".env", "SECRET=private");
        f.write(".secrets/config.json", "{}");
        let file_url = tauri::Url::from_file_path(f.0.join("web/hello world.html"))
            .unwrap()
            .to_string();
        let paths = vec![
            "./web/hello world.html:12:3".into(),
            "main.rs".into(),
            file_url,
            "web/hello world.html#L12-L14".into(),
            "web/hello%20world.html".into(),
            "duplicate.svg".into(),
            ".env".into(),
            ".secrets/config.json".into(),
            "../outside.html".into(),
            "https://example.com/index.html".into(),
        ];
        let files = resolve_files(&f.0, paths.clone()).unwrap();
        assert_eq!(files.len(), 5);
        assert_eq!(files[0].input, paths[0]);
        assert_eq!(files[0].path, "web/hello world.html");
        assert!(files[0].previewable);
        assert_eq!(files[1].path, "src/main.rs");
        assert!(!files[1].previewable);
        assert_eq!(files[2].path, files[0].path);
        assert_eq!(files[3].path, files[0].path);
        assert_eq!(files[4].path, files[0].path);
    }

    async fn request(url: &tauri::Url, path: &str, extra: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", url.port().unwrap()))
            .await
            .unwrap();
        stream
            .write_all(
                format!(
                    "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{extra}\r\n",
                    url.port().unwrap()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        String::from_utf8(bytes).unwrap()
    }

    async fn authorize(url: &tauri::Url) -> String {
        let response = request(url, url.path(), "").await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("location.replace(\"/web/hello%20world.html\" + location.search + location.hash)"));
        let cookie = response
            .lines()
            .find_map(|line| line.strip_prefix("Set-Cookie: "))
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        assert!(response.contains("HttpOnly; SameSite=Strict"));
        format!("Cookie: {cookie}\r\n")
    }

    #[tokio::test]
    async fn serves_html_svg_and_relative_assets_on_demand_with_isolated_origins() {
        let f = Fixture::new();
        f.write(
            "web/hello world.html",
            "<link href='style.css'><img src='/logo.svg'><script src='app.js'></script>",
        );
        f.write("web/style.css", "body { color: gold; }");
        f.write("web/app.js", "document.title = 'Preview';");
        f.write(
            "logo.svg",
            "<svg xmlns='http://www.w3.org/2000/svg'><circle r='10'/></svg>",
        );
        let state = FilePreviews::default();
        let preview = state.prepare(&f.0, "web/hello world.html").unwrap();
        let url = tauri::Url::parse(&preview.url).unwrap();
        let cookie = authorize(&url).await;
        for (path, mime, body) in [
            (
                "/web/hello%20world.html",
                "text/html",
                "<link href='style.css'>",
            ),
            ("/web/style.css", "text/css", "color: gold"),
            ("/web/app.js", "text/javascript", "document.title"),
            ("/logo.svg", "image/svg+xml", "<circle r='10'/>"),
        ] {
            let response = request(&url, path, &cookie).await;
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
            assert!(response.contains(&format!("Content-Type: {mime}")));
            assert!(response.contains("X-Content-Type-Options: nosniff"));
            assert!(response.contains("Cache-Control: no-store"));
            assert!(response.contains(body));
        }
        let other = Fixture::new();
        other.write("logo.svg", "<svg/>");
        let second = state.prepare(&other.0, "logo.svg").unwrap();
        assert_ne!(tauri::Url::parse(&second.url).unwrap().port(), url.port());
        assert_eq!(
            tauri::Url::parse(&state.prepare(&f.0, "logo.svg").unwrap().url)
                .unwrap()
                .port(),
            url.port()
        );
    }

    #[tokio::test]
    async fn restored_navigation_only_bootstraps_owned_preview_origins() {
        let f = Fixture::new();
        f.write("web/hello world.html", "<h1>Hello</h1>");
        let state = FilePreviews::default();
        let bootstrap = tauri::Url::parse(&state.prepare(&f.0, "web/hello world.html").unwrap().url).unwrap();
        let clean = bootstrap.join("/web/hello%20world.html?tab=design#preview").unwrap();
        let restored = state.navigation_url(&clean).unwrap();
        assert_eq!(restored.path(), bootstrap.path());
        assert_eq!(restored.query(), clean.query());
        assert_eq!(restored.fragment(), clean.fragment());
        assert_eq!(state.navigation_url(&restored).unwrap(), restored, "do not wrap twice");
        for value in [
            "https://example.com/index.html".into(),
            "http://127.0.0.1:3000/index.html".into(),
            clean.as_str().replace("127.0.0.1", "localhost"),
            clean.as_str().replace("127.0.0.1", "127.0.0.1.evil.example"),
            clean.as_str().replace("http:", "https:"),
        ] {
            let url = tauri::Url::parse(&value).unwrap();
            assert_eq!(state.navigation_url(&url).unwrap(), url, "never authorize unrelated origins");
        }
        state.shutdown();
        assert_eq!(state.navigation_url(&clean).unwrap(), clean);
    }

    #[tokio::test]
    async fn restored_directory_navigation_bootstraps_without_exposing_private_files() {
        let f = Fixture::new();
        f.write("index.html", "<h1>Root preview</h1>");
        f.write("web/index.html", "<h1>Directory preview</h1>");
        f.write(".env", "PRIVATE");
        let state = FilePreviews::default();
        let initial = tauri::Url::parse(&state.prepare(&f.0, "web/index.html").unwrap().url).unwrap();
        let directory = state.navigation_url(&initial.join("/web/").unwrap()).unwrap();
        let response = request(&directory, directory.path(), "").await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("location.replace(\"/web/\" + location.search + location.hash)"));
        let cookie = response.lines().find_map(|line| line.strip_prefix("Set-Cookie: ")).unwrap().split(';').next().unwrap();
        let cookie = format!("Cookie: {cookie}\r\n");
        assert!(request(&directory, "/web/", &cookie).await.contains("Directory preview"));
        let root = state.navigation_url(&initial.join("/").unwrap()).unwrap();
        let response = request(&root, root.path(), "").await;
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.contains("location.replace(\"/\" + location.search + location.hash)"));
        let secret = state.navigation_url(&initial.join("/.env").unwrap()).unwrap();
        let response = request(&secret, secret.path(), "").await;
        assert!(response.starts_with("HTTP/1.1 404"));
        assert!(!response.contains("PRIVATE"));
    }

    #[tokio::test]
    async fn rejects_unauthorized_cross_origin_sensitive_and_traversal_requests() {
        let f = Fixture::new();
        f.write("web/hello world.html", "<h1>Hello</h1>");
        f.write(".env", "SECRET=private");
        f.write(".git/config", "private");
        f.write(".secrets/key.json", "private");
        f.write("key.pem", "private");
        let state = FilePreviews::default();
        let url =
            tauri::Url::parse(&state.prepare(&f.0, "web/hello world.html").unwrap().url).unwrap();
        assert!(request(&url, "/web/hello%20world.html", "")
            .await
            .starts_with("HTTP/1.1 403"));
        let cookie = authorize(&url).await;
        for path in [
            "/../outside.html",
            "/%2e%2e/outside.html",
            "/%2eenv",
            "/.git/config",
            "/.secrets/key.json",
            "/key.pem",
            "/web%2f..%2f.env",
            "/web\\..\\.env",
        ] {
            let response = request(&url, path, &cookie).await;
            assert!(!response.starts_with("HTTP/1.1 200"), "{path}: {response}");
            assert!(!response.contains("SECRET=private"));
        }
        for header in [
            "Origin: https://evil.example\r\n",
            "Sec-Fetch-Site: cross-site\r\n",
            "Sec-Fetch-Site: same-site\r\n",
            "Host: evil.example\r\n",
        ] {
            assert!(request(
                &url,
                "/web/hello%20world.html",
                &format!("{cookie}{header}")
            )
            .await
            .starts_with("HTTP/1.1 403"));
        }
        assert!(request(&url, url.path(), "Sec-Fetch-Site: cross-site\r\n")
            .await
            .starts_with("HTTP/1.1 403"));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_outside_and_hidden_targets_through_symlinks() {
        let f = Fixture::new();
        let outside = Fixture::new();
        outside.write("secret.html", "private");
        f.write(".secrets/secret.svg", "private");
        std::os::unix::fs::symlink(outside.0.join("secret.html"), f.0.join("escape.html")).unwrap();
        std::os::unix::fs::symlink(f.0.join(".secrets/secret.svg"), f.0.join("linked.svg"))
            .unwrap();
        assert!(
            resolve_files(&f.0, vec!["escape.html".into(), "linked.svg".into()])
                .unwrap()
                .is_empty()
        );
    }
    #[tokio::test]
    async fn limits_files_and_releases_listeners_on_shutdown() {
        let f = Fixture::new();
        f.write("index.html", "<h1>Preview</h1>");
        let state = FilePreviews::default();
        let url = tauri::Url::parse(&state.prepare(&f.0, "index.html").unwrap().url).unwrap();
        fs::File::create(f.0.join("large.png"))
            .unwrap()
            .set_len(MAX_FILE + 1)
            .unwrap();
        assert!(state
            .prepare(&f.0, "large.png")
            .unwrap_err()
            .contains("32 MiB"));
        assert!(resolve_files(&f.0, vec!["index.html".into(); 129]).is_err());
        let stopped = state.servers.lock().unwrap().front().unwrap().stop.clone();
        state.shutdown();
        // Observe server completion directly: a refused Windows TCP connect may
        // itself exceed this budget even after the listener has been released.
        tokio::time::timeout(Duration::from_secs(2), stopped.closed())
            .await
            .unwrap();
        let _rebound = std::net::TcpListener::bind(("127.0.0.1", url.port().unwrap()))
            .expect("shutdown must release the preview listener");
    }

    #[tokio::test]
    async fn directory_navigation_keeps_relative_assets_under_its_directory() {
        let f = Fixture::new();
        f.write("web/hello world.html", "<h1>Preview</h1>");
        f.write("web/index.html", "<link href='style.css'>");
        f.write("web/style.css", "body { color: gold; }");
        let state = FilePreviews::default();
        let url =
            tauri::Url::parse(&state.prepare(&f.0, "web/hello world.html").unwrap().url).unwrap();
        let cookie = authorize(&url).await;
        let response = request(&url, "/web", &cookie).await;
        assert!(response.starts_with("HTTP/1.1 303"));
        assert!(response.contains("Location: /web/\r\n"));
        assert!(request(&url, "/web/", &cookie)
            .await
            .contains("<link href='style.css'>"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "Requires Node, Chrome and Playwright WebKit; run this fixture explicitly with --ignored."]
    async fn real_browser_bootstrap_and_assets() {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/file-preview-browser.mjs");
        let f = Fixture::new();
        f.write("web/hello world.html", "<!doctype html><link href='style.css' rel='stylesheet'><h1 id='preview-heading'>Workspace preview</h1><img id='preview-image' src='/logo.svg'><script src='app.js'></script>");
        f.write(
            "web/style.css",
            "#preview-heading { color: rgb(212, 170, 25); }",
        );
        f.write(
            "web/app.js",
            "document.title = 'Preview ready'; document.body.dataset.loaded = 'true';",
        );
        f.write("logo.svg", "<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'><rect width='32' height='32' fill='gold'/></svg>");
        let state = FilePreviews::default();
        let preview = state.prepare(&f.0, "web/hello world.html").unwrap();
        let clean = tauri::Url::parse(&preview.url).unwrap()
            .join("/web/hello%20world.html?restored=two%20words#preview-heading").unwrap();
        let restored = state.navigation_url(&clean).unwrap();
        let output = tokio::task::spawn_blocking(move || {
            let mut command = std::process::Command::new("node");
            command.arg(script).arg(preview.url).arg(restored.as_str());
            crate::workspace::run_process(
                command,
                "Preview browser check",
                64 * 1024,
                Duration::from_secs(90),
            )
        })
        .await
        .unwrap()
        .unwrap();
        state.shutdown();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout.bytes),
            String::from_utf8_lossy(&output.stderr.bytes)
        );
    }
    #[tokio::test]
    async fn selected_task_previews_use_its_worktree_and_reject_other_projects() {
        let main = Fixture::new();
        let checkout = Fixture::new();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&main.0)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "-q"]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--allow-empty",
            "-qm",
            "Initial",
        ]);
        #[cfg(windows)]
        let checkout_git = dunce::simplified(&checkout.0);
        #[cfg(not(windows))]
        let checkout_git = checkout.0.as_path();
        git(&[
            "worktree",
            "add",
            "-qb",
            "file-preview",
            checkout_git.to_str().unwrap(),
        ]);
        main.write("web/hello world.html", "MAIN CHECKOUT");
        checkout.write("web/hello world.html", "TASK WORKTREE");
        let store = crate::storage::Store::open(Path::new(":memory:")).unwrap();
        let project = store.add_project(&main.0).unwrap();
        let source = store
            .create_session(&project.id, "codex", "default", "ask")
            .unwrap();
        let task = store
            .create_worktree_session_on(&source, &checkout.0, None)
            .unwrap();
        let root = store.workspace_root(&project.id, Some(&task.id)).unwrap();
        assert_eq!(root, checkout.0);
        let state = FilePreviews::default();
        let url =
            tauri::Url::parse(&state.prepare(&root, "web/hello world.html").unwrap().url).unwrap();
        let cookie = authorize(&url).await;
        let response = request(&url, "/web/hello%20world.html", &cookie).await;
        assert!(response.contains("TASK WORKTREE"));
        assert!(!response.contains("MAIN CHECKOUT"));
        assert!(resolve_files(
            &root,
            vec![main.0.join("web/hello world.html").display().to_string()]
        )
        .unwrap()
        .is_empty());
        let other = Fixture::new();
        let other_project = store.add_project(&other.0).unwrap();
        assert!(store
            .workspace_root(&other_project.id, Some(&task.id))
            .is_err());
    }
}
