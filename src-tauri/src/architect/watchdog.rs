//! A team turn that goes silent is stopped, so one hung tool call cannot block the team.
use super::*;

/// Claude's longest tool timeout is 10 minutes, so a turn with no event for twice that
/// is stuck. Approvals and questions that wait on the user never count as silence.
pub(crate) const SILENCE_MINUTES: i64 = 20;
const SILENCE_LIMIT_MS: i64 = SILENCE_MINUTES * 60 * 1000;
const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

pub(crate) fn architect_note() -> String {
    format!("The architect's turn was stopped after {SILENCE_MINUTES} minutes without any output.")
}
pub(crate) fn worker_note() -> String {
    format!("No output for {SILENCE_MINUTES} minutes, so PandaCode stopped this worker. Inspect its conversation before retrying.")
}

pub(crate) fn watch(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(CHECK_EVERY).await;
            let state = app.state::<AppState>();
            if state.shutdown.load(Ordering::SeqCst) != 0 {
                break;
            }
            if let Err(error) = stop_silent(&state, storage::now(), &mut |team| emit(&app, team)) {
                eprintln!("Team watchdog: {error}");
            }
        }
    });
}

/// Stops every silent team turn and returns how many it stopped. The flag is saved
/// before the stop, so the completion settles the run as a timeout and not a user stop.
pub(super) fn stop_silent(
    state: &AppState,
    now: i64,
    notify: &mut impl FnMut(&TeamSnapshot),
) -> Result<usize> {
    let agents = state.agents.lock().map_err(|e| e.to_string())?;
    let silent = agents
        .iter()
        .filter(|(_, agent)| {
            agent.silent_for(now) >= SILENCE_LIMIT_MS
                && !agent.stopped.load(Ordering::SeqCst)
                && !agent.has_pending_approvals()
        })
        .map(|(sid, _)| sid.clone())
        .collect::<Vec<_>>();
    if silent.is_empty() {
        return Ok(0);
    }
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let mut stopped = 0;
    for sid in silent {
        if store.session(&sid).is_ok_and(|task| task.status == "waiting") {
            continue;
        }
        let parent = match load(&store, &sid)? {
            Some(_) => sid.clone(),
            None => match parent_of(&store, &sid)? {
                Some(parent) => parent,
                None => continue,
            },
        };
        let Some(mut team) = load(&store, &parent)? else {
            continue;
        };
        if !active(&team.status) {
            continue;
        }
        if parent == sid {
            team.timed_out = true;
        } else if let Some(worker) = team
            .workers
            .iter_mut()
            .find(|w| w.session_id == sid && active(&w.status))
        {
            worker.timed_out = true;
        } else {
            continue;
        }
        save(&store, &mut team)?;
        notify(&team);
        if let Some(agent) = agents.get(&sid) {
            let _ = agent.send(RunInput::Stop);
        }
        stopped += 1;
    }
    Ok(stopped)
}
