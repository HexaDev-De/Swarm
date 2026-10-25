//! Estimated API cost from the agents' own local session logs (Claude Code
//! project logs and Codex rollouts). Files are read only when Agent usage is
//! opened; token counts are cached per file so later opens only read what was
//! appended. Nothing leaves the machine except the public price-list request.

mod parse;
mod pricing;
#[cfg(test)]
mod tests;

use parse::{CodexState, Record};
pub(crate) use parse::unix_seconds;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use tauri::Manager;

const HISTORY_DAYS: i64 = 366;
const RESULT_TTL_MS: i64 = 60_000;
const CACHE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Agent {
    Claude,
    Codex,
    Antigravity,
}

/// Parsed token counts for one log file, with where to resume once it grows.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct FileEntry {
    agent: Option<Agent>,
    length: u64,
    modified: i64,
    offset: u64,
    session: bool,
    #[serde(default)]
    codex: CodexState,
    records: Vec<Record>,
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    version: u32,
    files: HashMap<String, FileEntry>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelInfo {
    provider: Agent,
    name: String,
    priced: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionSpan {
    provider: Agent,
    first: i64,
    last: i64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceInfo {
    provider: Agent,
    files: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Pricing {
    source: pricing::Source,
    fetched_at: Option<i64>,
}

/// `[unixHour, modelIndex, input, cacheRead, cacheWrite, output, costUsd, savingsUsd]`
type Row = (i64, usize, u64, u64, u64, u64, f64, f64);
/// Token and dollar sums for one hour of one model.
type Sums = (u64, u64, u64, u64, f64, f64);

/// Rows are `[unixHour, modelIndex, input, cacheRead, cacheWrite, output, costUsd, savingsUsd]`;
/// the UI groups hours into local calendar days.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Ledger {
    scanned_at: i64,
    pricing: Pricing,
    sources: Vec<SourceInfo>,
    models: Vec<ModelInfo>,
    rows: Vec<Row>,
    sessions: Vec<SessionSpan>,
}

/// Claude Code `projects` folders across every configuration location.
pub(crate) fn claude_roots() -> Vec<PathBuf> {
    roots(Agent::Claude)
}

fn roots(agent: Agent) -> Vec<PathBuf> {
    let home = crate::agent_usage_providers::home();
    let mut roots: Vec<PathBuf> = match agent {
        Agent::Claude => {
            let mut dirs: Vec<PathBuf> = crate::agent_usage::claude_config_dir().into_iter().collect();
            dirs.extend(home.map(|home| home.join(".config").join("claude")));
            dirs.into_iter().map(|dir| dir.join("projects")).collect()
        }
        Agent::Codex => {
            let base = std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .or_else(|| home.map(|home| home.join(".codex")));
            base.into_iter().flat_map(|base| [base.join("sessions"), base.join("archived_sessions")]).collect()
        }
        Agent::Antigravity => {
            let mut dirs: Vec<PathBuf> = Vec::new();
            if let Some(home) = home {
                dirs.push(home.join(".gemini").join("antigravity").join("brain"));
                dirs.push(home.join(".gemini").join("antigravity"));
                dirs.push(home.join(".config").join("antigravity"));
            }
            dirs
        }
    };
    roots.dedup();
    roots
}

fn modified_ms(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// `.jsonl` files changed within the history window, without following links.
fn log_files(root: &Path, since_ms: i64, output: &mut Vec<(PathBuf, std::fs::Metadata)>) {
    let mut pending = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() && depth < 8 {
                pending.push((path, depth + 1));
            } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "jsonl") {
                if path.file_name().is_some_and(|name| name == "transcript_full.jsonl") {
                    continue;
                }
                if let Ok(metadata) = entry.metadata() {
                    if modified_ms(&metadata) >= since_ms {
                        output.push((path, metadata));
                    }
                }
            }
        }
    }
}

/// Reads complete lines from `entry.offset`, so an appended log resumes where it
/// stopped and a line still being written is read next time.
fn read_file(path: &Path, agent: Agent, entry: &mut FileEntry) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(entry.offset))?;
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 || line.last() != Some(&b'\n') {
            break;
        }
        entry.offset += read as u64;
        let Ok(text) = std::str::from_utf8(&line) else { continue };
        match agent {
            Agent::Claude => entry.records.extend(parse::claude_line(text)),
            Agent::Codex => {
                if let Some(record) = parse::codex_line(text, &mut entry.codex) {
                    parse::merge_hourly(&mut entry.records, record);
                }
            }
            Agent::Antigravity => {
                if let Some(record) = parse::antigravity_line(text) {
                    parse::merge_hourly(&mut entry.records, record);
                }
            }
        }
    }
    Ok(())
}

/// Brings the cache up to date with the logs on disk; returns whether it changed.
fn refresh_cache(cache: &mut Cache, since_ms: i64) -> bool {
    let mut changed = false;
    let mut seen = HashSet::new();
    for agent in [Agent::Claude, Agent::Codex, Agent::Antigravity] {
        let mut files = Vec::new();
        for root in roots(agent) {
            log_files(&root, since_ms, &mut files);
        }
        for (path, metadata) in files {
            let key = path.to_string_lossy().into_owned();
            let (length, modified) = (metadata.len(), modified_ms(&metadata));
            let entry = cache.files.entry(key.clone()).or_default();
            seen.insert(key);
            if entry.agent == Some(agent) && entry.length == length && entry.modified == modified {
                continue;
            }
            // Logs are append-only; anything else (truncated, replaced, first sight) is read from the start.
            if entry.agent != Some(agent) || length < entry.offset {
                *entry = FileEntry {
                    agent: Some(agent),
                    session: agent == Agent::Codex || !path.components().any(|part| part.as_os_str() == "subagents"),
                    ..FileEntry::default()
                };
            }
            if read_file(&path, agent, entry).is_err() {
                *entry = FileEntry::default();
                continue;
            }
            entry.length = length;
            entry.modified = modified;
            changed = true;
        }
    }
    let before = cache.files.len();
    cache.files.retain(|path, _| seen.contains(path));
    changed || cache.files.len() != before
}

fn aggregate(cache: &Cache, book: &pricing::PriceBook, since_hour: i64, scanned_at: i64) -> Ledger {
    let mut models: Vec<ModelInfo> = Vec::new();
    let mut model_index: HashMap<(Agent, &str), usize> = HashMap::new();
    let mut rows: HashMap<(i64, usize), Sums> = HashMap::new();
    let mut replies = HashSet::new();
    let mut sessions = Vec::new();
    let mut sources: HashMap<Agent, usize> = HashMap::new();
    sources.insert(Agent::Claude, 0);
    sources.insert(Agent::Codex, 0);
    sources.insert(Agent::Antigravity, 0);
    let mut paths: Vec<&String> = cache.files.keys().collect();
    paths.sort();
    for path in paths {
        let entry = &cache.files[path];
        let Some(agent) = entry.agent else { continue };
        *sources.entry(agent).or_default() += 1;
        let mut span: Option<(i64, i64)> = None;
        for record in entry.records.iter().filter(|record| record.hour >= since_hour) {
            if record.key.is_some_and(|key| !replies.insert(key)) {
                continue;
            }
            let index = *model_index.entry((agent, record.model.as_str())).or_insert_with(|| {
                models.push(ModelInfo { provider: agent, name: record.model.clone(), priced: book.find(&record.model).is_some() });
                models.len() - 1
            });
            let (cost, savings) = book.cost(record).unwrap_or((0.0, 0.0));
            let row = rows.entry((record.hour, index)).or_default();
            row.0 += record.input;
            row.1 += record.cache_read;
            row.2 += record.cache_write + record.cache_write_1h;
            row.3 += record.output;
            row.4 += cost;
            row.5 += savings;
            span = Some(span.map_or((record.hour, record.hour), |(first, last)| (first.min(record.hour), last.max(record.hour))));
        }
        if let (true, Some((first, last))) = (entry.session, span) {
            sessions.push(SessionSpan { provider: agent, first, last });
        }
    }
    let mut rows: Vec<_> = rows
        .into_iter()
        .map(|((hour, model), (input, read, write, output, cost, savings))| (hour, model, input, read, write, output, cost, savings))
        .collect();
    rows.sort_by_key(|row| (row.0, row.1));
    let mut sources: Vec<SourceInfo> = sources.into_iter().map(|(provider, files)| SourceInfo { provider, files }).collect();
    sources.sort_by_key(|source| source.provider as u8);
    Ledger {
        scanned_at,
        pricing: Pricing { source: book.source, fetched_at: book.fetched_at },
        sources,
        models,
        rows,
        sessions,
    }
}

fn load_cache(path: &Path) -> Cache {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Cache>(&bytes).ok())
        .filter(|cache| cache.version == CACHE_VERSION)
        .unwrap_or(Cache { version: CACHE_VERSION, files: HashMap::new() })
}

fn save_cache(path: &Path, cache: &Cache) {
    let Ok(bytes) = serde_json::to_vec(cache) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let temporary = path.with_extension("tmp");
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(&temporary, path);
    }
}

struct State {
    cache: Option<Cache>,
    last: Option<(i64, std::sync::Arc<Ledger>)>,
}

fn state() -> &'static tokio::sync::Mutex<State> {
    static STATE: std::sync::OnceLock<tokio::sync::Mutex<State>> = std::sync::OnceLock::new();
    STATE.get_or_init(|| tokio::sync::Mutex::new(State { cache: None, last: None }))
}

async fn build(dir: PathBuf, refresh: bool) -> crate::Result<std::sync::Arc<Ledger>> {
    // One scan at a time; a second window or a quick reopen shares the result.
    let mut state = state().lock().await;
    let now = crate::storage::now();
    if let Some((_, ledger)) = state.last.as_ref().filter(|(at, _)| !refresh && now - at < RESULT_TTL_MS) {
        return Ok(ledger.clone());
    }
    let book = pricing::load(&dir.join("usage-prices.json"), now).await;
    let cache = state.cache.take();
    let cache_path = dir.join("usage-ledger-cache.json");
    let since_ms = now - HISTORY_DAYS * 86_400_000;
    let (cache, ledger) = tokio::time::timeout(
        Duration::from_secs(300),
        tauri::async_runtime::spawn_blocking(move || {
            let mut cache = cache.unwrap_or_else(|| load_cache(&cache_path));
            if refresh_cache(&mut cache, since_ms) {
                save_cache(&cache_path, &cache);
            }
            let ledger = aggregate(&cache, &book, since_ms.div_euclid(3_600_000), now);
            (cache, ledger)
        }),
    )
    .await
    .map_err(|_| "Reading agent logs took too long. Try again later.".to_string())?
    .map_err(|e| e.to_string())?;
    let ledger = std::sync::Arc::new(ledger);
    state.cache = Some(cache);
    state.last = Some((now, ledger.clone()));
    Ok(ledger)
}

#[tauri::command]
pub(crate) async fn usage_ledger(app: tauri::AppHandle, window: tauri::WebviewWindow, refresh: Option<bool>) -> crate::Result<Ledger> {
    if window.label() != "main" {
        return Err("Usage estimates are available only in the main application window.".into());
    }
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(Ledger::clone(&*build(dir, refresh.unwrap_or(false)).await?))
}
