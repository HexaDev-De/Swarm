//! Per-run snapshots keep child streaming off SQLite's shared write lock.
use crate::{
    providers::{AgentEvent, SubagentActivity},
    storage::Store,
    Result,
};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct SubagentState {
    entries: HashMap<String, Entry>,
    next_number: usize,
}

struct Entry {
    activity: SubagentActivity,
    last_saved: Option<Instant>,
    saved_status: String,
    number: usize,
}

/// Launcher metadata deliberately excludes responses, prompts, and tool output.
pub(crate) struct ActiveSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub updated_at: i64,
}

fn running(status: &str) -> bool {
    matches!(status, "running" | "waiting")
}

impl SubagentState {
    pub(crate) fn record(
        &mut self,
        store: &Mutex<Store>,
        run_id: &str,
        event: &mut AgentEvent,
    ) -> Result<()> {
        let mut activity: SubagentActivity =
            serde_json::from_value(event.data.take().ok_or("Subagent snapshot is missing.")?)
                .map_err(|error| error.to_string())?;
        activity.id = format!("{run_id}:{}", activity.id);
        activity.parent_id = activity.parent_id.map(|id| format!("{run_id}:{id}"));
        for tool in &mut activity.tools {
            tool.id = format!("{}:{}", activity.id, tool.id);
        }
        if !self.entries.contains_key(&activity.id) && self.entries.len() >= 64 {
            let oldest = self
                .entries
                .iter()
                .filter(|(_, entry)| !running(&entry.activity.status))
                .min_by_key(|(_, entry)| entry.activity.updated_at)
                .map(|(id, _)| id.clone());
            if let Some(id) = oldest {
                self.entries.remove(&id);
            } else {
                return Err("This run already has 64 active subagents.".into());
            }
        }
        event.data = Some(serde_json::to_value(&activity).map_err(|error| error.to_string())?);
        if !self.entries.contains_key(&activity.id) {
            self.next_number += 1;
        }
        let number = self.next_number;
        let entry = self
            .entries
            .entry(activity.id.clone())
            .or_insert_with(|| Entry {
                activity: activity.clone(),
                last_saved: None,
                saved_status: String::new(),
                number,
            });
        entry.activity = activity;
        // First sighting and lifecycle changes are durable immediately. Live
        // text/tools overwrite one in-memory snapshot and checkpoint at 1 Hz.
        if entry
            .last_saved
            .is_none_or(|saved| saved.elapsed() >= Duration::from_secs(1))
            || entry.saved_status != entry.activity.status
        {
            store
                .lock()
                .map_err(|error| error.to_string())?
                .save_subagent(&event.session_id, &entry.activity)?;
            entry.last_saved = Some(Instant::now());
            entry.saved_status.clone_from(&entry.activity.status);
        }
        Ok(())
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    pub(crate) fn get(&self, id: &str) -> Option<SubagentActivity> {
        self.entries.get(id).map(|entry| entry.activity.clone())
    }

    pub(crate) fn active_summaries(&self) -> Vec<ActiveSummary> {
        let mut entries: Vec<_> = self
            .entries
            .values()
            .filter(|entry| running(&entry.activity.status))
            .collect();
        entries.sort_by_key(|entry| entry.number);
        entries
            .into_iter()
            .map(|entry| {
                let name = entry.activity.name.trim();
                let name = if name.is_empty()
                    || name.contains(['/', '\\'])
                    || name.eq_ignore_ascii_case("agent")
                    || name.eq_ignore_ascii_case("subagent")
                {
                    format!("#{}", entry.number)
                } else {
                    name.chars().take(160).collect()
                };
                ActiveSummary {
                    id: entry.activity.id.clone(),
                    name,
                    status: entry.activity.status.clone(),
                    updated_at: entry.activity.updated_at,
                }
            })
            .collect()
    }

    pub(crate) fn merge_history(&self, history: Vec<SubagentActivity>) -> Vec<SubagentActivity> {
        let mut combined: HashMap<String, SubagentActivity> = history
            .into_iter()
            .map(|activity| (activity.id.clone(), activity))
            .collect();
        combined.extend(
            self.entries
                .iter()
                .map(|(id, entry)| (id.clone(), entry.activity.clone())),
        );
        let mut activities: Vec<_> = combined.into_values().collect();
        activities.sort_by(|a, b| {
            running(&b.status)
                .cmp(&running(&a.status))
                .then_with(|| b.updated_at.cmp(&a.updated_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        activities.truncate(64);
        activities
    }

    pub(crate) fn finish(
        &mut self,
        store: &Store,
        session_id: &str,
        status: &str,
    ) -> (Vec<SubagentActivity>, Result<()>) {
        let mut changed = Vec::new();
        let mut first_error = None;
        for entry in self.entries.values_mut() {
            if running(&entry.activity.status) {
                entry.activity.status = status.into();
                entry.activity.updated_at = crate::storage::now();
                for tool in &mut entry.activity.tools {
                    if running(&tool.status) {
                        tool.status = "failed".into();
                    }
                }
                changed.push(entry.activity.clone());
            }
            // Always flush: the final text snapshot may still be inside the
            // checkpoint interval even when the child already reported done.
            if let Err(error) = store.save_subagent(session_id, &entry.activity) {
                first_error.get_or_insert(error);
            }
        }
        if let Err(error) = store.finish_subagents(session_id, status) {
            first_error.get_or_insert(error);
        }
        // The run has ended even if its final database write failed. Return
        // settlement snapshots independently so the live viewer also stops.
        (changed, first_error.map_or(Ok(()), Err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ToolActivity;

    fn task(store: &Store) -> String {
        let project = store.add_project(&std::env::temp_dir()).unwrap();
        store
            .create_session(&project.id, "codex", "default", "ask")
            .unwrap()
            .id
    }

    fn child(id: &str, status: &str) -> SubagentActivity {
        SubagentActivity {
            id: id.into(),
            parent_id: None,
            name: "Read project".into(),
            prompt: "Find the build command".into(),
            status: status.into(),
            response: "Reading".into(),
            tools: vec![ToolActivity {
                id: "read".into(),
                name: "Read".into(),
                status: "running".into(),
                input: "README.md".into(),
                output: String::new(),
                write_paths: None,
            }],
            started_at: 100,
            updated_at: 100,
        }
    }

    fn record(
        state: &mut SubagentState,
        store: &Mutex<Store>,
        session_id: &str,
        run: &str,
        child: SubagentActivity,
    ) -> SubagentActivity {
        let mut event = crate::event(session_id, "subagent", "");
        event.data = Some(serde_json::to_value(child).unwrap());
        state.record(store, run, &mut event).unwrap();
        serde_json::from_value(event.data.unwrap()).unwrap()
    }

    #[test]
    fn active_metadata_excludes_settled_children_and_uses_stable_safe_names() {
        let store = Mutex::new(Store::open(std::path::Path::new(":memory:")).unwrap());
        let sid = task(&store.lock().unwrap());
        let mut state = SubagentState::default();
        let mut first = child("first", "running");
        first.name = "/private/tmp/worker.jsonl".into();
        first.response = "x".repeat(500_000);
        record(&mut state, &store, &sid, "run", first.clone());
        let mut second = child("second", "waiting");
        second.name = "Design audit".into();
        record(&mut state, &store, &sid, "run", second);
        let rows = state.active_summaries();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "#1");
        assert_eq!(rows[1].name, "Design audit");
        assert!(state.contains("run:first"));
        assert!(!state.contains("another:first"));
        first.status = "completed".into();
        record(&mut state, &store, &sid, "run", first);
        let mut third = child("third", "running");
        third.name = "agent".into();
        record(&mut state, &store, &sid, "run", third);
        let rows = state.active_summaries();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "run:second");
        assert_eq!(rows[1].name, "#3");
    }

    #[test]
    fn ids_parents_and_tools_are_scoped_to_the_run_and_history_isolated_by_task() {
        let store = Mutex::new(Store::open(std::path::Path::new(":memory:")).unwrap());
        let sid = task(&store.lock().unwrap());
        let other_sid = task(&store.lock().unwrap());
        let mut state = SubagentState::default();
        let mut activity = child("worker", "completed");
        activity.parent_id = Some("parent".into());
        let saved = record(&mut state, &store, &sid, "first", activity.clone());
        assert_eq!(saved.id, "first:worker");
        assert_eq!(saved.parent_id.as_deref(), Some("first:parent"));
        assert_eq!(saved.tools[0].id, "first:worker:read");
        record(
            &mut SubagentState::default(),
            &store,
            &sid,
            "second",
            activity.clone(),
        );
        record(
            &mut SubagentState::default(),
            &store,
            &other_sid,
            "third",
            activity,
        );
        let db = store.lock().unwrap();
        assert_eq!(db.subagents(&sid).unwrap().len(), 2);
        assert_eq!(db.subagents(&other_sid).unwrap()[0].id, "third:worker");
        assert!(db.messages(&sid, None).unwrap().is_empty());
        assert!(db.subagents("missing").is_err());
    }

    #[test]
    fn live_snapshots_override_history_without_writing_for_every_chunk() {
        let store = Mutex::new(Store::open(std::path::Path::new(":memory:")).unwrap());
        let sid = task(&store.lock().unwrap());
        let mut state = SubagentState::default();
        let mut activity = child("worker", "running");
        record(&mut state, &store, &sid, "run", activity.clone());
        let changes = store.lock().unwrap().db.total_changes();
        for n in 0..200 {
            activity.response = format!("Read {n} lines");
            activity.updated_at += 1;
            record(&mut state, &store, &sid, "run", activity.clone());
        }
        let history = store.lock().unwrap().subagents(&sid).unwrap();
        assert_eq!(store.lock().unwrap().db.total_changes(), changes);
        assert_eq!(history[0].response, "Reading");
        assert_eq!(state.merge_history(history)[0].response, "Read 199 lines");
        state.entries.get_mut("run:worker").unwrap().last_saved =
            Some(Instant::now() - Duration::from_secs(2));
        activity.response = "Checkpoint".into();
        record(&mut state, &store, &sid, "run", activity.clone());
        assert_eq!(
            store.lock().unwrap().subagents(&sid).unwrap()[0].response,
            "Checkpoint"
        );
        activity.status = "completed".into();
        activity.response = "Final answer".into();
        record(&mut state, &store, &sid, "run", activity.clone());
        assert_eq!(
            store.lock().unwrap().subagents(&sid).unwrap()[0].response,
            "Final answer"
        );
        activity.response = "Final answer with last suffix".into();
        record(&mut state, &store, &sid, "run", activity);
        let db = store.lock().unwrap();
        let (changed, saved) = state.finish(&db, &sid, "stopped");
        saved.unwrap();
        assert!(changed.is_empty());
        assert_eq!(
            db.subagents(&sid).unwrap()[0].response,
            "Final answer with last suffix"
        );
    }

    #[test]
    fn finishing_settles_only_unfinished_children_and_preserves_their_work() {
        for status in ["stopped", "failed"] {
            let store = Mutex::new(Store::open(std::path::Path::new(":memory:")).unwrap());
            let sid = task(&store.lock().unwrap());
            let other_sid = task(&store.lock().unwrap());
            let mut state = SubagentState::default();
            record(&mut state, &store, &sid, "run", child("active", "running"));
            record(
                &mut state,
                &store,
                &sid,
                "run",
                child("approval", "waiting"),
            );
            record(&mut state, &store, &sid, "run", child("done", "completed"));
            record(
                &mut SubagentState::default(),
                &store,
                &other_sid,
                "other",
                child("worker", "running"),
            );
            let db = store.lock().unwrap();
            let (changed, saved) = state.finish(&db, &sid, status);
            saved.unwrap();
            assert_eq!(changed.len(), 2);
            for activity in &changed {
                assert_eq!(activity.status, status);
                assert_eq!(activity.response, "Reading");
                assert_eq!(activity.tools[0].status, "failed");
            }
            let history = db.subagents(&sid).unwrap();
            assert_eq!(
                history.iter().find(|a| a.id == "run:done").unwrap().status,
                "completed"
            );
            assert_eq!(db.subagents(&other_sid).unwrap()[0].status, "running");
        }
    }

    #[test]
    fn restart_retains_history_and_recovers_children_even_if_parent_status_was_saved_first() {
        let dir = std::env::temp_dir().join(format!(
            "pandacode-subagent-history-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.sqlite");
        let sid;
        {
            let store = Store::open(&path).unwrap();
            sid = task(&store);
            store
                .save_subagent(&sid, &child("active", "running"))
                .unwrap();
            store
                .save_subagent(&sid, &child("waiting", "waiting"))
                .unwrap();
            store
                .save_subagent(&sid, &child("done", "completed"))
                .unwrap();
            for _ in 0..105 {
                store.message(&sid, "assistant", "Parent response").unwrap();
            }
            assert_eq!(store.messages(&sid, None).unwrap().len(), 100);
        }
        {
            let store = Store::open(&path).unwrap();
            let history = store.subagents(&sid).unwrap();
            assert_eq!(history.len(), 3);
            assert_eq!(history.iter().filter(|a| a.status == "stopped").count(), 2);
            assert!(history
                .iter()
                .filter(|a| a.status == "stopped")
                .all(|a| a.tools[0].status == "failed"));
            assert_eq!(
                history.iter().find(|a| a.id == "done").unwrap().status,
                "completed"
            );
            assert!(history.iter().all(|a| a.response == "Reading"));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bounded_history_keeps_active_children_and_cascades_with_their_task() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let sid = task(&store);
        store
            .save_subagent(&sid, &child("still-active", "running"))
            .unwrap();
        for n in 0..80 {
            let mut activity = child(&format!("done-{n}"), "completed");
            activity.updated_at = 1000 + n;
            store.save_subagent(&sid, &activity).unwrap();
        }
        let history = store.subagents(&sid).unwrap();
        assert_eq!(history.len(), 64);
        assert!(history.iter().any(|a| a.id == "still-active"));
        assert!(history.iter().any(|a| a.id == "done-79"));
        let mut live = SubagentState::default();
        live.entries.insert(
            "new-worker".into(),
            Entry {
                activity: child("new-worker", "running"),
                last_saved: None,
                saved_status: String::new(),
                number: 1,
            },
        );
        let merged = live.merge_history(history);
        assert_eq!(merged.len(), 64);
        assert!(merged.iter().take(2).all(|a| a.status == "running"));
        store
            .db
            .execute("DELETE FROM sessions WHERE id=?1", [&sid])
            .unwrap();
        assert_eq!(
            store
                .db
                .query_row("SELECT COUNT(*) FROM subagents", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn a_failed_checkpoint_keeps_live_work_for_retry() {
        let store = Mutex::new(Store::open(std::path::Path::new(":memory:")).unwrap());
        let sid = task(&store.lock().unwrap());
        let mut state = SubagentState::default();
        store.lock().unwrap().db.execute_batch("CREATE TRIGGER reject_child BEFORE INSERT ON subagents BEGIN SELECT RAISE(ABORT,'test full disk'); END;").unwrap();
        let mut event = crate::event(&sid, "subagent", "");
        event.data = Some(serde_json::to_value(child("worker", "running")).unwrap());
        assert!(state.record(&store, "run", &mut event).is_err());
        assert_eq!(state.merge_history(Vec::new())[0].response, "Reading");
        let db = store.lock().unwrap();
        db.db.execute_batch("DROP TRIGGER reject_child;").unwrap();
        state.finish(&db, &sid, "stopped").1.unwrap();
        assert_eq!(db.subagents(&sid).unwrap()[0].response, "Reading");
    }

    #[test]
    fn history_command_returns_live_work_and_retries_cleanup_when_the_run_is_gone() {
        use crate::{AppState, RunningAgent};
        use std::sync::{
            atomic::{AtomicBool, AtomicU8},
            Arc,
        };
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let sid = task(&store);
        let state = AppState {
            store: Arc::new(Mutex::new(store)),
            agents: Arc::new(Mutex::new(HashMap::new())),
            terminals: Arc::new(Mutex::new(HashMap::new())),
            shutdown: AtomicU8::new(0),
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let agent = RunningAgent::new(tx, Arc::new(AtomicBool::new(false)));
        let snapshots = agent.subagents.clone();
        let run = agent.run_id.clone();
        state.agents.lock().unwrap().insert(sid.clone(), agent);
        let mut activity = child("worker", "running");
        record(
            &mut snapshots.lock().unwrap(),
            &state.store,
            &sid,
            &run,
            activity.clone(),
        );
        activity.response = "Fresh live response".into();
        record(
            &mut snapshots.lock().unwrap(),
            &state.store,
            &sid,
            &run,
            activity,
        );
        assert_eq!(
            crate::task_subagents(&state, &sid).unwrap()[0].response,
            "Fresh live response"
        );
        assert_eq!(
            state.store.lock().unwrap().subagents(&sid).unwrap()[0].response,
            "Reading"
        );
        // A failed final persistence attempt can leave the on-disk child live
        // after the parent registry entry has been removed. Reads repair it.
        state.agents.lock().unwrap().remove(&sid);
        state.store.lock().unwrap().status(&sid, "error").unwrap();
        let recovered = crate::task_subagents(&state, &sid).unwrap();
        assert_eq!(recovered[0].status, "failed");
        assert_eq!(recovered[0].tools[0].status, "failed");
        assert_eq!(
            state.store.lock().unwrap().subagents(&sid).unwrap()[0].status,
            "failed"
        );
    }

    #[test]
    fn failed_final_writes_still_return_terminal_snapshots_for_live_emission() {
        let store = Mutex::new(Store::open(std::path::Path::new(":memory:")).unwrap());
        let sid = task(&store.lock().unwrap());
        let mut state = SubagentState::default();
        record(&mut state, &store, &sid, "run", child("worker", "running"));
        let db = store.lock().unwrap();
        db.db.execute_batch("CREATE TRIGGER reject_final_insert BEFORE INSERT ON subagents BEGIN SELECT RAISE(ABORT,'test full disk'); END;
            CREATE TRIGGER reject_final_update BEFORE UPDATE ON subagents BEGIN SELECT RAISE(ABORT,'test full disk'); END;").unwrap();
        let (changed, saved) = state.finish(&db, &sid, "stopped");
        assert!(
            saved.is_err(),
            "Parent completion must retain the persistence failure"
        );
        assert_eq!(
            changed.len(),
            1,
            "Live settlement must still be emitted on save failure"
        );
        assert_eq!(changed[0].status, "stopped");
        assert_eq!(changed[0].tools[0].status, "failed");
        assert_eq!(db.subagents(&sid).unwrap()[0].status, "running");
        assert_eq!(
            state.merge_history(db.subagents(&sid).unwrap())[0].status,
            "stopped"
        );
        db.db
            .execute_batch("DROP TRIGGER reject_final_insert; DROP TRIGGER reject_final_update;")
            .unwrap();
        state.finish(&db, &sid, "stopped").1.unwrap();
        assert_eq!(db.subagents(&sid).unwrap()[0].status, "stopped");
    }
}
