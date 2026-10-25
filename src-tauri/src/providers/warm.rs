//! Warm agents: after a Claude turn ends, the next turn's process starts in the background
//! so a follow-up skips the CLI's own startup (about 1.4 s). A warm agent is adopted only
//! when everything on its command line and in its tool catalog still matches; otherwise it
//! is discarded and the turn starts cold as before.
use super::*;
use std::sync::LazyLock;

/// A warm agent waits at most this long for its follow-up.
const IDLE: Duration = Duration::from_secs(10 * 60);
/// Each one holds an idle CLI process, so only the most recent tasks keep one.
const LIMIT: usize = 2;

pub(super) struct Spare {
    key: String,
    pub(super) child: Child,
    pub(super) group: ProcessGroup,
    pub(super) server: Option<mcp_host::Server>,
    born: std::time::Instant,
}

static SPARES: LazyLock<Mutex<HashMap<String, Spare>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
/// Counts each task's Claude launches. A spare started for launch N is useless once launch N+1
/// has begun: it would resume the conversation without that turn.
static LAUNCHES: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The task's current launch count, read when a turn ends and before the next can begin.
pub fn epoch(session_id: &str) -> u64 {
    LAUNCHES.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(session_id).copied().unwrap_or(0)
}

fn spares() -> std::sync::MutexGuard<'static, HashMap<String, Spare>> {
    SPARES.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Only plain Claude Code: key-based relays carry per-launch credentials.
pub fn supported(config: &RunConfig) -> bool {
    config.provider == "claude" && config.goal.is_none() && config.skill.is_none()
}

/// Everything that shapes the launch: the command line, the folder and the tool catalog.
pub(super) fn key(config: &RunConfig, tools: &[Value]) -> String {
    json!({
        "provider": config.provider, "model": config.model, "effort": config.reasoning_effort,
        "mode": config.mode, "cwd": config.cwd, "resume": config.resume_id,
        "instructions": config.instructions, "tools": tools,
        // The login a process starts with (API key or subscription) is fixed at spawn.
        "connection": format!("{:?}", crate::provider_settings::claude_connection_mode()),
    })
    .to_string()
}

/// The warm agent for this turn, if one exists, is still running and was started for
/// exactly this launch. Any other warm agent for the task is discarded.
/// Every launch of the task counts, with or without a spare to adopt.
pub(super) fn launched(session_id: &str) {
    *LAUNCHES.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).entry(session_id.to_owned()).or_default() += 1;
}

pub(super) fn take(session_id: &str, key: &str) -> Option<Spare> {
    launched(session_id);
    let mut spare = spares().remove(session_id)?;
    let alive = matches!(spare.child.try_wait(), Ok(None));
    (alive && spare.key == key && spare.born.elapsed() < IDLE).then_some(spare)
}

/// Drop a task's warm agent (its settings changed, or it was deleted).
pub fn discard(session_id: &str) {
    spares().remove(session_id);
}

/// Start the next turn's agent for a task. `tools` is the catalog its last turn used; the
/// placeholder server answers the agent's startup listing and is rebound on adoption.
pub async fn prewarm(mut config: RunConfig, tools: Vec<Value>, approval_tools: HashSet<String>, launch: u64) -> Result<(), String> {
    if !supported(&config) {
        return Ok(());
    }
    let path = executable(&config.provider).ok_or("Claude Code is not installed")?;
    config.cwd = external_path(&config.cwd);
    let key = key(&config, &tools);
    let mut command = command(&path)?;
    configure_command(&mut command, &config);
    let server = if tools.is_empty() {
        None
    } else {
        let placeholder = ToolHost {
            tools,
            approval_tools,
            handler: Arc::new(|_, _| Box::pin(async { Err("This turn has not started yet.".to_string()) })),
        };
        let sink: Sink = Arc::new(|_| {});
        let server = mcp_host::Server::start_with_approvals(placeholder, config.session_id.clone(), sink, &config.mode).await?;
        command.args(["--mcp-config", &server.claude_config().to_string()]);
        Some(server)
    };
    command
        .current_dir(&config.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let child = command.spawn().map_err(|e| format!("Could not start Claude Code: {e}"))?;
    let group = ProcessGroup::register(child.id());
    let born = std::time::Instant::now();
    let session_id = config.session_id.clone();
    {
        let mut spares = spares();
        // The next turn already started (a team review, a queued message): this spare would
        // carry an outdated conversation. Dropping it kills its process.
        if epoch(&session_id) != launch {
            return Ok(());
        }
        spares.insert(session_id.clone(), Spare { key, child, group, server, born });
        // Keep only the newest few; dropping a spare kills its process.
        while spares.len() > LIMIT {
            let Some(oldest) = spares.iter().min_by_key(|(_, spare)| spare.born).map(|(id, _)| id.clone()) else { break };
            spares.remove(&oldest);
        }
    }
    tokio::spawn(async move {
        tokio::time::sleep(IDLE).await;
        let mut spares = spares();
        if spares.get(&session_id).is_some_and(|spare| spare.born == born) {
            spares.remove(&session_id);
        }
    });
    Ok(())
}

#[cfg(test)]
pub(super) fn waiting(session_id: &str) -> bool {
    spares().contains_key(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_warm_agent_is_used_only_for_the_exact_launch_it_was_started_for() {
        let base = crate::providers::tests::config("claude");
        let tools = vec![json!({"type":"function","name":"pandacode_title"})];
        let original = key(&base, &tools);
        assert_eq!(original, key(&base.clone(), &tools));
        let changed = |edit: fn(&mut RunConfig)| { let mut config = base.clone(); edit(&mut config); key(&config, &tools) };
        for different in [
            changed(|c| c.model = "claude-sonnet-5-5".into()),
            changed(|c| c.mode = "full".into()),
            changed(|c| c.reasoning_effort = Some("high".into())),
            changed(|c| c.resume_id = Some("another".into())),
            changed(|c| c.instructions = "new".into()),
            key(&base, &[]),
        ] {
            assert_ne!(different, original);
        }
        // Relays, goals and skills always start cold.
        assert!(supported(&base));
        assert!(!supported(&crate::providers::tests::config("glm")));
        assert!(!supported(&RunConfig { skill: Some(("review".into(), PathBuf::from("/skills/review"))), ..base.clone() }));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_mismatched_or_exited_warm_agent_is_discarded() {
        let spawn = || {
            let mut command = Command::new("sleep");
            command.arg("30").kill_on_drop(true);
            let child = command.spawn().unwrap();
            let group = ProcessGroup::register(None);
            (child, group)
        };
        let (child, group) = spawn();
        spares().insert("warm-test".into(), Spare { key: "launch-a".into(), child, group, server: None, born: std::time::Instant::now() });
        assert!(take("warm-test", "launch-b").is_none(), "a different launch starts cold");
        assert!(!waiting("warm-test"), "and the stale agent is gone");
        let (child, group) = spawn();
        spares().insert("warm-test".into(), Spare { key: "launch-a".into(), child, group, server: None, born: std::time::Instant::now() });
        let mut adopted = take("warm-test", "launch-a").expect("the matching agent is adopted");
        adopted.child.kill().await.unwrap();
        let (mut child, group) = spawn();
        child.kill().await.unwrap();
        child.wait().await.unwrap();
        spares().insert("warm-test".into(), Spare { key: "launch-a".into(), child, group, server: None, born: std::time::Instant::now() });
        assert!(take("warm-test", "launch-a").is_none(), "an agent that exited is never adopted");
        discard("warm-test");
    }
}
