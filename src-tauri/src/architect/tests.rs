use super::*;
fn coordinate(
    state: &AppState,
    parent: &str,
    actor: &coordination::Actor,
    name: &str,
    args: Value,
) -> Result<Value> {
    let generation = load(&state.store.lock().unwrap(), parent)?
        .unwrap()
        .generation;
    coordination::dispatch_state(
        state,
        parent,
        generation,
        actor,
        name,
        args,
        &AtomicBool::new(false),
    )
    .map(|r| r.0)
}
fn running_worker(
    state: &AppState,
    parent: &str,
    key: &str,
    file: &str,
) -> (crate::PreparedRun, coordination::Actor) {
    call(
        state,
        parent,
        "pandacode_dispatch_workers",
        json!({"assignments":[assignment(key,"fast",file)]}),
    )
    .unwrap();
    let action = next_action_state(state, &mut |_| {}).unwrap().unwrap();
    let (run, _rx) = claim(state, action);
    let team = load(&state.store.lock().unwrap(), parent).unwrap().unwrap();
    let actor = coordination::Actor::resolve(&team, &run.config.session_id).unwrap();
    (run, actor)
}

#[test]
fn task_deletion_waits_for_team_cleanup_and_late_callbacks_cannot_recreate_it() {
    let (state, parent) = state_fixture();
    // Logical team activity continues even between provider processes.
    assert!(state.agents.lock().unwrap().is_empty());
    assert!(crate::task_deletion::delete_state(&state, &parent)
        .unwrap_err().contains("Pause"));
    let (run, _) = running_worker(&state, &parent, "parser", "src");
    let child = run.config.session_id.clone();
    let admission = run.team.as_ref().unwrap();
    control_state(&state, &parent, "pause").unwrap();
    // Pausing invalidates callbacks, but does not mean the child process exited.
    assert!(crate::task_deletion::delete_state(&state, &parent).is_err());
    {
        let mut agents = state.agents.lock().unwrap();
        let store = state.store.lock().unwrap();
        store.finish_task_turn(&child, false, true).unwrap();
        agents.remove(&child);
    }
    let deleted = crate::task_deletion::delete_state(&state, &parent).unwrap();
    assert_eq!(deleted.deleted_session_ids.len(), 2);
    assert!(deleted.deleted_session_ids.contains(&child));
    let mut notifications = 0;
    completed_state(&state, &child, Some(admission), true, false, &mut |_| {
        notifications += 1;
    });
    fail_launch_state(&state, admission, &child, "Late launch failure", &mut |_| {
        notifications += 1;
    });
    assert!(dispatch_state(
        &state, &parent, admission.generation, "pandacode_dispatch_workers",
        json!({"assignments":[assignment("late","fast","src")]}),
        &AtomicBool::new(false),
    ).is_err());
    assert!(next_action_state(&state, &mut |_| { notifications += 1; }).unwrap().is_none());
    assert_eq!(notifications, 0);
    let store = state.store.lock().unwrap();
    assert!(load(&store, &parent).unwrap().is_none());
    assert!(store.session(&parent).is_err());
    assert!(store.session(&child).is_err());
    for table in ["sessions", "architect_teams", "architect_workers", "architect_requests"] {
        let count: i64 = store.db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0, "{table}");
    }
}

#[test]
fn generated_review_keeps_model_input_but_projects_a_timed_coordination_turn() {
    let (state, parent) = state_fixture();
    state
        .store
        .lock()
        .unwrap()
        .message(&parent, "user", "Build the parser")
        .unwrap();
    let (run, _) = running_worker(&state, &parent, "parser", "src");
    finish(&state, &run, "Parser checked successfully", true);
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let original = action.prompt.clone();
    let (review, _) = claim(&state, action);
    assert!(review.config.prompt.ends_with(&original));
    let store = state.store.lock().unwrap();
    let (role, saved): (String, String) = store
        .db
        .query_row(
            "SELECT role,content FROM messages WHERE id=?1",
            [review.turn_message_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(role, "coordination");
    let (kind, raw) = internal_turns::decode(&saved).unwrap();
    assert_eq!(kind, internal_turns::CoordinationKind::WorkerReview);
    assert_eq!(raw, original);
    store
        .finish_turn(&parent, review.turn_message_id, 1250)
        .unwrap();
    let visible = store.message_page(&parent, None).unwrap();
    let row = visible.messages.last().unwrap();
    assert_eq!(row.role, "coordination");
    assert_eq!(row.content, "Reviewing worker results");
    assert_eq!(row.coordination_kind, Some(kind));
    assert_eq!(row.turn_duration_ms, Some(1250));
    assert!(!serde_json::to_string(&visible)
        .unwrap()
        .contains("Parser checked successfully"));
    let (restored, _) = crate::context_recovery::prepare(
        &store,
        &store.session(&parent).unwrap(),
        "Continue review",
    )
    .unwrap();
    let history = restored
        .split("Earlier conversation (JSON):\n")
        .nth(1)
        .unwrap()
        .split("\n\nCurrent request:")
        .next()
        .unwrap();
    let history: Value = serde_json::from_str(history).unwrap();
    let restored = history
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["origin"] == "pandacode_coordination")
        .unwrap();
    assert_eq!(restored["content"], original);
}

#[test]
fn generated_worker_notification_is_typed_but_user_json_and_copies_are_unchanged() {
    let (state, parent) = state_fixture();
    let (_run, actor) = running_worker(&state, &parent, "parser", "src");
    coordinate(
        &state,
        &parent,
        &actor,
        "pandacode_send_team_message",
        json!({"recipient":"architect","key":"question","message":"Which interface?"}),
    )
    .unwrap();
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let original = action.prompt.clone();
    let (review, _) = claim(&state, action);
    assert!(review.config.prompt.ends_with(&original));
    let store = state.store.lock().unwrap();
    let message = store.messages(&parent, None).unwrap().pop().unwrap();
    assert_eq!(
        message.coordination_kind,
        Some(internal_turns::CoordinationKind::WorkerUpdate)
    );
    assert_eq!(message.content, "Checking worker updates");
    for content in [
        original.as_str(),
        r#"{"workers":[{"name":"user JSON"}],"round":3}"#,
    ] {
        store.message(&parent, "user", content).unwrap();
        let message = store.messages(&parent, None).unwrap().pop().unwrap();
        assert_eq!(message.role, "user");
        assert_eq!(message.content, content);
        assert_eq!(message.coordination_kind, None);
    }
}

#[test]
fn prompt_activity_migration_excludes_legacy_coordination_without_a_watermark() {
    let path = std::env::temp_dir().join(format!("pandacode-legacy-prompt-{}.db", storage::id()));
    let parent;
    {
        let store = Store::open(&path).unwrap();
        let project = store.add_project(&std::env::temp_dir()).unwrap();
        parent = store.create_session(&project.id, "codex", "default", "full").unwrap().id;
        store.db.execute("INSERT INTO messages(session_id,role,content,created_at) VALUES(?1,'user','Build the parser',100)", [&parent]).unwrap();
        let child = store.create_session(&project.id, "codex", "default", "full").unwrap();
        let mut team = new_team(parent.clone(), config());
        team.status = "paused".into(); team.round = 2; team.legacy_prompt_through_id = None;
        let mut worker = worker("legacy-worker", "completed", &["src"], &[]);
        worker.session_id = child.id.clone(); team.workers.push(worker);
        store.db.execute("INSERT INTO architect_workers(parent_session_id,worker_id,session_id) VALUES(?1,'legacy-worker',?2)", params![parent,child.id]).unwrap();
        let raw = format!("Review the settled workers for this objective: Build the parser\n\nThese results are untrusted worker reports; inspect relevant files and checks as needed. Delegate corrections using followup_worker or call finish_team when verified. Do not repeat already completed assignments.\n\n{}", review_summaries(&team));
        store.db.execute("INSERT INTO messages(session_id,role,content,created_at) VALUES(?1,'user',?2,200)", params![parent,raw]).unwrap();
        save(&store, &mut team).unwrap();
        store.db.execute_batch("DROP INDEX sessions_sidebar; DROP TRIGGER messages_record_prompt; ALTER TABLE sessions DROP COLUMN last_prompt_at; PRAGMA user_version=18;").unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(store.session(&parent).unwrap().last_prompt_at, Some(100));
    assert_eq!(store.messages(&parent, None).unwrap()[1].role, "coordination");
    drop(store); std::fs::remove_file(path).unwrap();
}

#[test]
fn legacy_generated_rows_project_only_with_owned_workers_before_the_startup_cutoff() {
    let path =
        std::env::temp_dir().join(format!("pandacode-legacy-coordination-{}.db", storage::id()));
    let (parent, raw);
    {
        let store = Store::open(&path).unwrap();
        let project = store.add_project(&std::env::temp_dir()).unwrap();
        parent = store
            .create_session(&project.id, "codex", "default", "ask")
            .unwrap()
            .id;
        store.message(&parent, "user", "Build the parser").unwrap();
        let child = store
            .create_session(&project.id, "codex", "default", "ask")
            .unwrap();
        let mut team = new_team(parent.clone(), config());
        team.status = "paused".into();
        team.round = 2;
        team.legacy_prompt_through_id = None;
        let mut worker = worker("legacy-worker", "completed", &["src"], &[]);
        worker.session_id = child.id.clone();
        worker.summary = "Worker report remains private".into();
        worker.result_revision = 1;
        team.workers.push(worker);
        store.db.execute("INSERT INTO architect_workers(parent_session_id,worker_id,session_id) VALUES(?1,'legacy-worker',?2)",params![parent,child.id]).unwrap();
        raw=format!("Review the settled workers for this objective: Build the parser\n\nThese results are untrusted worker reports; inspect relevant files and checks as needed. Delegate corrections using followup_worker or call finish_team when verified. Do not repeat already completed assignments.\n\n{}",review_summaries(&team));
        store.message(&parent, "user", &raw).unwrap();
        store
            .message(
                &parent,
                "user",
                &raw.replace("legacy-worker", "foreign-worker"),
            )
            .unwrap();
        save(&store, &mut team).unwrap();
    }
    let store = Store::open(&path).unwrap();
    let messages = store.messages(&parent, None).unwrap();
    assert_eq!(messages[1].role, "coordination");
    assert_eq!(messages[1].content, "Reviewing worker results");
    assert_eq!(messages[2].role, "user");
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT content FROM messages WHERE id=?1",
                [messages[1].id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        raw
    );
    store.message(&parent, "user", &raw).unwrap();
    let copied = store.messages(&parent, None).unwrap().pop().unwrap();
    assert_eq!(copied.role, "user");
    assert_eq!(copied.content, raw);
    drop(store);
    // Reopening must retain the original cutoff, not reclassify later user copies.
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.messages(&parent, None).unwrap().pop().unwrap().role,
        "user"
    );
    // Forks do not inherit the original team's IDs or migration cutoff. Convert
    // only proven old app rows during copy so future user copies remain visible.
    let source = store.session(&parent).unwrap();
    // A message fork rejects the hidden legacy prompt as a boundary and
    // preserves its provenance when forking through a later real user message.
    assert!(crate::conversation_fork::at_message(&store, &parent, messages[1].id).is_err());
    assert!(store.message_content(&parent, messages[1].id).is_err());
    assert_eq!(store.message_content(&parent, copied.id).unwrap(), raw);
    let message_fork = crate::conversation_fork::at_message(&store, &parent, copied.id).unwrap();
    let message_fork_rows = store.messages(&message_fork.id, None).unwrap();
    assert_eq!(message_fork_rows.len(), 4);
    assert_eq!(message_fork_rows[1].role, "coordination");
    assert_eq!(message_fork_rows[1].content, "Reviewing worker results");
    assert_eq!(message_fork_rows[3].role, "user");
    assert_eq!(message_fork_rows[3].content, raw);
    let fork = store
        .create_session(&source.project_id, "codex", "default", "ask")
        .unwrap();
    crate::task_controls::copy_task_messages(&store, &parent, &fork.id).unwrap();
    let fork_messages = store.messages(&fork.id, None).unwrap();
    assert_eq!(fork_messages[1].role, "coordination");
    assert_eq!(fork_messages[1].content, "Reviewing worker results");
    assert_eq!(fork_messages[2].role, "user");
    assert_eq!(fork_messages[3].role, "user");
    assert_eq!(fork_messages[3].content, raw);
    let copied: String = store
        .db
        .query_row(
            "SELECT content FROM messages WHERE id=?1",
            [fork_messages[1].id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(internal_turns::decode(&copied).unwrap().1, raw);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn notification_context_stays_bounded_with_many_escaped_worker_reports() {
    let (state, parent) = state_fixture();
    {
        let store = state.store.lock().unwrap();
        let mut team = load(&store, &parent).unwrap().unwrap();
        team.config.max_assignments = 50;
        team.config.summary_limit = 6000;
        team.coordination_revision = 1;
        team.workers = (0..50)
            .map(|index| {
                let mut worker = worker(&format!("worker-{index}"), "running", &[], &[]);
                worker.summary = "\"\\\n".repeat(2000);
                worker.error = Some("\"\\".repeat(512));
                worker.result_revision = 1;
                worker
            })
            .collect();
        save(&store, &mut team).unwrap();
    }
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert_eq!(action.sid, parent);
    // Escaping and per-worker metadata count toward the actual serialized wire
    // budget; the notification must not inject the entire set of reports.
    assert!(
        action.prompt.len() < 42 * 1024,
        "{} bytes",
        action.prompt.len()
    );
    assert!(action.prompt.contains("[Truncated]"));
    assert!(action.prompt.contains("Use read_worker"));
    assert!(action.prompt.contains("worker-0"));
}

#[test]
fn direct_mailboxes_are_private_cursor_paged_and_idempotent() {
    let (state, parent) = state_fixture();
    let (a, actor_a) = running_worker(&state, &parent, "a", "src");
    let (b, actor_b) = running_worker(&state, &parent, "b", "tests");
    let worker_b = load(&state.store.lock().unwrap(), &parent)
        .unwrap()
        .unwrap()
        .workers[1]
        .id
        .clone();
    let send =
        json!({"recipient":worker_b,"key":"api-shape","message":"Use Widget { id, title }."});
    let first = coordinate(
        &state,
        &parent,
        &actor_a,
        "pandacode_send_team_message",
        send.clone(),
    )
    .unwrap();
    let again = coordinate(
        &state,
        &parent,
        &actor_a,
        "pandacode_send_team_message",
        send,
    )
    .unwrap();
    assert_eq!(first["messageId"], again["messageId"]);
    assert_eq!(again["duplicate"], true);
    assert!(coordinate(
        &state,
        &parent,
        &actor_a,
        "pandacode_send_team_message",
        json!({"recipient":worker_b,"key":"api-shape","message":"Different"})
    )
    .is_err());
    coordinate(
        &state,
        &parent,
        &actor_a,
        "pandacode_send_team_message",
        json!({"recipient":worker_b,"key":"api-next","message":"Validation is ready."}),
    )
    .unwrap();
    for actor in [&actor_a, &coordination::Actor::Architect] {
        assert_eq!(
            coordinate(
                &state,
                &parent,
                actor,
                "pandacode_read_team_mailbox",
                json!({})
            )
            .unwrap()["messages"],
            json!([])
        );
    }
    let page = coordinate(
        &state,
        &parent,
        &actor_b,
        "pandacode_read_team_mailbox",
        json!({"limit":1}),
    )
    .unwrap();
    assert_eq!(page["messages"].as_array().unwrap().len(), 1);
    assert_eq!(page["hasMore"], true);
    let next = coordinate(
        &state,
        &parent,
        &actor_b,
        "pandacode_read_team_mailbox",
        json!({"afterId":page["nextCursor"],"limit":1}),
    )
    .unwrap();
    assert_eq!(next["messages"][0]["message"], "Validation is ready.");
    assert_eq!(next["hasMore"], false);
    assert!(coordinate(
        &state,
        &parent,
        &actor_a,
        "pandacode_read_team_mailbox",
        json!({"recipient":worker_b})
    )
    .is_err());
    let context = prompt(
        &state.store.lock().unwrap(),
        &b.config.session_id,
        "Continue",
        b.team.as_ref(),
    )
    .unwrap();
    assert!(context.contains("Use Widget"));
    let context = prompt(
        &state.store.lock().unwrap(),
        &a.config.session_id,
        "Continue",
        a.team.as_ref(),
    )
    .unwrap();
    assert!(!context.contains("Use Widget"));
}

#[test]
fn workers_cannot_control_teams_spoof_senders_or_write_after_cancel_pause_or_retry() {
    let (state, parent) = state_fixture();
    let (_run, actor) = running_worker(&state, &parent, "a", "src");
    let worker_id = load(&state.store.lock().unwrap(), &parent)
        .unwrap()
        .unwrap()
        .workers[0]
        .id
        .clone();
    for name in [
        "pandacode_dispatch_workers",
        "pandacode_followup_worker",
        "pandacode_finish_team",
        "pandacode_cancel_worker",
    ] {
        assert!(coordinate(&state, &parent, &actor, name, json!({})).is_err());
        assert!(!coordination::tools(true).iter().any(|t| t["name"] == name));
    }
    assert!(coordinate(
        &state,
        &parent,
        &actor,
        "pandacode_send_team_message",
        json!({"recipient":"foreign-worker","key":"one","message":"Hello"})
    )
    .is_err());
    assert!(coordinate(
        &state,
        &parent,
        &actor,
        "pandacode_send_team_message",
        json!({"recipient":"architect","sender":"architect","key":"one","message":"Hello"})
    )
    .is_err());
    let original = load(&state.store.lock().unwrap(), &parent)
        .unwrap()
        .unwrap()
        .generation;
    call(
        &state,
        &parent,
        "pandacode_cancel_worker",
        json!({"workerId":worker_id,"reason":"No longer needed"}),
    )
    .unwrap();
    assert!(coordinate(
        &state,
        &parent,
        &actor,
        "pandacode_send_team_message",
        json!({"recipient":"architect","key":"late","message":"Late write"})
    )
    .is_err());
    {
        let store = state.store.lock().unwrap();
        let mut team = load(&store, &parent).unwrap().unwrap();
        team.workers[0].status = "running".into();
        team.workers[0].abandoned = false;
        team.workers[0].attempt += 1;
        save(&store, &mut team).unwrap();
    }
    assert!(coordinate(
        &state,
        &parent,
        &actor,
        "pandacode_read_team_mailbox",
        json!({})
    )
    .is_err());
    control_state(&state, &parent, "pause").unwrap();
    assert!(coordination::dispatch_state(
        &state,
        &parent,
        original,
        &coordination::Actor::Architect,
        "pandacode_send_team_message",
        json!({"recipient":worker_id,"key":"late-parent","message":"Late"}),
        &AtomicBool::new(false)
    )
    .is_err());
}

#[test]
fn checkpoints_notify_idle_architect_once_without_interrupting_workers_or_exhausting_review() {
    let (state, parent) = state_fixture();
    let (run, actor) = running_worker(&state, &parent, "a", "src");
    let checkpoint = json!({"key":"parsed","summary":"Parser implemented; two checks pass.","nextSteps":"Wire rendering after API choice.","blocker":"Choose widget interface"});
    coordinate(
        &state,
        &parent,
        &actor,
        "pandacode_checkpoint_worker",
        checkpoint.clone(),
    )
    .unwrap();
    let revision = load(&state.store.lock().unwrap(), &parent)
        .unwrap()
        .unwrap()
        .coordination_revision;
    coordinate(
        &state,
        &parent,
        &actor,
        "pandacode_checkpoint_worker",
        checkpoint,
    )
    .unwrap();
    assert_eq!(
        load(&state.store.lock().unwrap(), &parent)
            .unwrap()
            .unwrap()
            .coordination_revision,
        revision
    );
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert_eq!(action.sid, parent);
    assert!(action.prompt.contains("Choose widget interface"));
    let (review, _) = claim(&state, action);
    finish(&state, &review, "Please use Widget", true);
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    let context = prompt(
        &state.store.lock().unwrap(),
        &run.config.session_id,
        "Continue",
        run.team.as_ref(),
    )
    .unwrap();
    assert!(context.contains("Parser implemented"));
    {
        let store = state.store.lock().unwrap();
        let mut team = load(&store, &parent).unwrap().unwrap();
        team.round = team.config.max_rounds - 1;
        save(&store, &mut team).unwrap();
    }
    coordinate(
        &state,
        &parent,
        &actor,
        "pandacode_send_team_message",
        json!({"recipient":"architect","key":"new-info","message":"Additional detail"}),
    )
    .unwrap();
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    assert_eq!(
        load(&state.store.lock().unwrap(), &parent)
            .unwrap()
            .unwrap()
            .workers[0]
            .status,
        "running"
    );
    finish(&state, &run, "Completed", true);
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert_eq!(action.sid, parent);
    assert_eq!(
        load(&state.store.lock().unwrap(), &parent)
            .unwrap()
            .unwrap()
            .round,
        5
    );
}

#[test]
fn checkpoint_validation_roles_and_limits_do_not_mutate_saved_progress() {
    let (state, parent) = state_fixture();
    let (_run, actor) = running_worker(&state, &parent, "a", "src");
    let valid = json!({"key":"one","summary":"Saved milestone","nextSteps":"Run targeted tests","blocker":null});
    assert!(coordinate(
        &state,
        &parent,
        &coordination::Actor::Architect,
        "pandacode_checkpoint_worker",
        valid.clone()
    )
    .is_err());
    coordinate(&state, &parent, &actor, "pandacode_checkpoint_worker", valid).unwrap();
    assert!(coordinate(
        &state,
        &parent,
        &actor,
        "pandacode_checkpoint_worker",
        json!({"key":"one","summary":"Changed milestone","nextSteps":"Run checks"})
    )
    .is_err());
    assert!(coordinate(
        &state,
        &parent,
        &actor,
        "pandacode_checkpoint_worker",
        json!({"key":"two","summary":"🌿".repeat(600),"nextSteps":"Run checks"})
    )
    .is_err());
    assert_eq!(
        load(&state.store.lock().unwrap(), &parent)
            .unwrap()
            .unwrap()
            .workers[0]
            .checkpoint
            .as_ref()
            .unwrap()
            .summary,
        "Saved milestone"
    );
}

#[test]
fn persisted_messages_and_checkpoint_survive_restart_without_auto_resuming() {
    let path = std::env::temp_dir().join(format!("pandacode-mailbox-{}.db", storage::id()));
    let (parent, worker_session);
    {
        let store = Store::open(&path).unwrap();
        let project = store.add_project(&std::env::temp_dir()).unwrap();
        parent = store
            .create_session(&project.id, "claude", "default", "ask")
            .unwrap()
            .id;
        let mut team = new_team(parent.clone(), config());
        team.status = "running".into();
        team.generation = 1;
        team.round = 1;
        save(&store, &mut team).unwrap();
        let state = AppState {
            store: Arc::new(std::sync::Mutex::new(store)),
            agents: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            terminals: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            shutdown: std::sync::atomic::AtomicU8::new(0),
        };
        let (run, actor) = running_worker(&state, &parent, "a", "src");
        worker_session = run.config.session_id.clone();
        coordinate(&state,&parent,&actor,"pandacode_send_team_message",json!({"recipient":"architect","key":"restart-message","message":"Parser interface is ready"})).unwrap();
        coordinate(&state,&parent,&actor,"pandacode_checkpoint_worker",json!({"key":"restart-progress","summary":"Parser checks passed","nextSteps":"Wire renderer","blocker":null})).unwrap();
    }
    let store = Store::open(&path).unwrap();
    let team = load(&store, &parent).unwrap().unwrap();
    assert_eq!(team.status, "interrupted");
    assert_eq!(team.workers[0].status, "stopped");
    assert!(team.pending_turn.is_none());
    assert_eq!(team.coordination_revision, 1);
    assert_eq!(
        team.workers[0].checkpoint.as_ref().unwrap().next_steps,
        "Wire renderer"
    );
    let context = coordination::context(&store, &team, &coordination::Actor::Architect, 0, true).unwrap().0;
    assert!(context.contains("Parser interface is ready"));
    let actor = coordination::Actor::resolve(&team, &worker_session).unwrap();
    let context = coordination::context(&store, &team, &actor, 0, true).unwrap().0;
    assert!(context.contains("Parser checks passed"));
    assert!(!context.contains("Parser interface is ready"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn collaboration_migration_refreshes_only_codex_team_catalogs_and_retains_private_history() {
    let path = std::env::temp_dir().join(format!("pandacode-team-v15-{}.db", storage::id()));
    let (parent, child, claude, ordinary);
    {
        let store = Store::open(&path).unwrap();
        let project = store.add_project(&std::env::temp_dir()).unwrap();
        parent = store
            .create_session(&project.id, "codex", "default", "ask")
            .unwrap()
            .id;
        child = store
            .create_session(&project.id, "codex", "default", "ask")
            .unwrap()
            .id;
        claude = store
            .create_session(&project.id, "claude", "default", "ask")
            .unwrap()
            .id;
        ordinary = store
            .create_session(&project.id, "codex", "default", "ask")
            .unwrap()
            .id;
        let mut team = new_team(parent.clone(), config());
        team.status = "running".into();
        team.generation = 1;
        for (id, sid) in [("child", &child), ("claude", &claude)] {
            let mut w = worker(id, "running", &[id], &[]);
            w.session_id = sid.clone();
            team.workers.push(w);
            store.db.execute("INSERT INTO architect_workers(parent_session_id,worker_id,session_id) VALUES(?1,?2,?3)",params![parent,id,sid]).unwrap();
        }
        for (sid, content) in [
            (&parent, "PRIVATE PARENT TEXT"),
            (&child, "PRIVATE WORKER TEXT"),
            (&claude, "CLAUDE TEXT"),
            (&ordinary, "ORDINARY TEXT"),
        ] {
            store.message(sid, "assistant", content).unwrap();
            store
                .db
                .execute(
                    "UPDATE sessions SET resume_id='native-context' WHERE id=?1",
                    [sid],
                )
                .unwrap();
        }
        save(&store, &mut team).unwrap();
        store
            .db
            .execute_batch("DROP TABLE architect_messages; PRAGMA user_version=15;")
            .unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert!(store.session(&parent).unwrap().resume_id.is_none());
    assert!(store.session(&child).unwrap().resume_id.is_none());
    assert_eq!(
        store.session(&claude).unwrap().resume_id.as_deref(),
        Some("native-context")
    );
    assert_eq!(
        store.session(&ordinary).unwrap().resume_id.as_deref(),
        Some("native-context")
    );
    let (handoff, _) =
        crate::context_recovery::prepare(&store, &store.session(&child).unwrap(), "Continue")
            .unwrap();
    assert!(handoff.contains("PRIVATE WORKER TEXT"));
    assert!(!handoff.contains("PRIVATE PARENT TEXT"));
    let team = load(&store, &parent).unwrap().unwrap();
    assert_eq!(team.status, "interrupted");
    assert!(team.pending_turn.is_none());
    assert_eq!(
        store
            .db
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        crate::storage::SCHEMA_VERSION
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn legacy_codex_catalog_refresh_preserves_private_history_and_survives_restart_once() {
    let path = std::env::temp_dir().join(format!("pandacode-team-catalog-{}.db", storage::id()));
    let (parent, child, ordinary, admission);
    {
        let store = Store::open(&path).unwrap();
        let project = store.add_project(&std::env::temp_dir()).unwrap();
        parent = store.create_session(&project.id, "codex", "default", "ask").unwrap().id;
        child = store.create_session(&project.id, "codex", "default", "ask").unwrap().id;
        ordinary = store.create_session(&project.id, "codex", "default", "ask").unwrap().id;
        let mut team = new_team(parent.clone(), config());
        team.status = "running".into();
        team.generation = 1;
        let mut child_worker = worker("child", "running", &["src"], &[]);
        child_worker.session_id = child.clone();
        team.workers.push(child_worker);
        save(&store, &mut team).unwrap();
        store.db.execute("UPDATE architect_teams SET content=json_remove(content,'$.toolCatalogVersion') WHERE session_id=?1", [&parent]).unwrap();
        for (sid, history) in [(&parent, "PRIVATE PARENT HISTORY"), (&child, "PRIVATE CHILD HISTORY"), (&ordinary, "ORDINARY HISTORY")] {
            store.message(sid, "assistant", history).unwrap();
            store.resume(sid, &format!("old-{sid}")).unwrap();
        }
        admission = Admission { parent: parent.clone(), generation: 1 };
        validate_admission(&store, &child, Some(&admission)).unwrap();
        assert_eq!(store.session(&parent).unwrap().resume_id, Some(format!("old-{parent}")));
        validate_admission(&store, &parent, Some(&admission)).unwrap();
        assert!(store.session(&parent).unwrap().resume_id.is_none(), "legacy Codex architect must start with the current tool catalog");
        assert_eq!(store.session(&child).unwrap().resume_id, Some(format!("old-{child}")));
        assert_eq!(store.session(&ordinary).unwrap().resume_id, Some(format!("old-{ordinary}")));
        assert_eq!(store.messages(&parent, None).unwrap().len(), 1);
    }
    let store = Store::open(&path).unwrap();
    let mut before = load(&store, &parent).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&before).unwrap()["toolCatalogVersion"], TOOL_CATALOG_VERSION);
    // Opening storage interrupts running teams; emulate the user's explicit
    // resume before checking that its already refreshed catalog is retained.
    before.status = "running".into();
    before.generation += 1;
    save(&store, &mut before).unwrap();
    let admission = Admission { parent: parent.clone(), generation: before.generation };
    validate_admission(&store, &parent, Some(&admission)).unwrap();
    assert_eq!(load(&store, &parent).unwrap().unwrap().revision, before.revision);
    let (prompt, through) = crate::context_recovery::prepare(&store, &store.session(&parent).unwrap(), "Continue the team").unwrap();
    assert!(prompt.contains("PRIVATE PARENT HISTORY"));
    assert!(!prompt.contains("PRIVATE CHILD HISTORY"));
    assert!(!prompt.contains("ORDINARY HISTORY"));
    let through = through.unwrap();
    store.resume(&parent, "fresh-native-context").unwrap();
    validate_admission(&store, &parent, Some(&admission)).unwrap();
    assert_eq!(store.session(&parent).unwrap().resume_id.as_deref(), Some("fresh-native-context"));
    assert!(crate::context_recovery::pending(&store, &store.session(&parent).unwrap()).unwrap());
    crate::context_recovery::acknowledge(&store, &parent, through).unwrap();
    assert!(!crate::context_recovery::pending(&store, &store.session(&parent).unwrap()).unwrap());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn catalog_refresh_skips_current_codex_and_other_harness_contexts() {
    assert_eq!(serde_json::to_value(new_team("fresh".into(), config())).unwrap()["toolCatalogVersion"], TOOL_CATALOG_VERSION);
    for provider in ["codex", "claude"] {
        let (store, mut team) = fixture();
        let sid = team.session_id.clone();
        save(&store, &mut team).unwrap();
        store.db.execute("UPDATE sessions SET provider=?2,resume_id='existing-context' WHERE id=?1", params![sid, provider]).unwrap();
        store.message(&sid, "assistant", "Saved conversation").unwrap();
        store.db.execute("UPDATE architect_teams SET content=json_set(content,'$.toolCatalogVersion',?2) WHERE session_id=?1", params![sid, if provider == "codex" { TOOL_CATALOG_VERSION } else { 0 }]).unwrap();
        validate_admission(&store, &sid, Some(&Admission { parent: sid.clone(), generation: 1 })).unwrap();
        assert_eq!(store.session(&sid).unwrap().resume_id.as_deref(), Some("existing-context"));
        assert!(!crate::context_recovery::pending(&store, &store.session(&sid).unwrap()).unwrap());
    }
}

#[test]
fn catalog_refresh_rolls_back_context_handoff_when_team_persistence_fails() {
    let (store, mut team) = fixture();
    let sid = team.session_id.clone();
    save(&store, &mut team).unwrap();
    store.db.execute("UPDATE architect_teams SET content=json_remove(content,'$.toolCatalogVersion') WHERE session_id=?1", [&sid]).unwrap();
    store.db.execute("UPDATE sessions SET provider='codex',resume_id='old-context' WHERE id=?1", [&sid]).unwrap();
    store.message(&sid, "assistant", "Saved conversation").unwrap();
    store.db.execute_batch("CREATE TRIGGER reject_catalog_update BEFORE UPDATE ON architect_teams BEGIN SELECT RAISE(ABORT,'catalog save failed'); END;").unwrap();
    let result = validate_admission(&store, &sid, Some(&Admission { parent: sid.clone(), generation: 1 }));
    assert!(result.is_err());
    assert_eq!(store.session(&sid).unwrap().resume_id.as_deref(), Some("old-context"));
    assert!(!crate::context_recovery::pending(&store, &store.session(&sid).unwrap()).unwrap());
    assert_eq!(serde_json::to_value(load(&store, &sid).unwrap().unwrap()).unwrap()["toolCatalogVersion"], 0);
}

fn config() -> TeamConfig {
    TeamConfig {
        auto: false,
        roster_only: false,
        name: None,
        workers: vec![WorkerProfile {
            id: "fast".into(),
            name: "Fast worker".into(),
            provider: "codex".into(),
            model: "default".into(),
            reasoning_effort: None,
        }],
        max_parallel: 3,
        max_rounds: 5,
        max_assignments: 10,
        workspace: "shared".into(),
        summary_limit: 2000,
    }
}
fn fixture() -> (Store, TeamSnapshot) {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let project = store.add_project(&std::env::temp_dir()).unwrap();
    let parent = store
        .create_session(&project.id, "claude", "default", "ask")
        .unwrap();
    let team = TeamSnapshot {
        session_id: parent.id,
        tool_catalog_version: TOOL_CATALOG_VERSION,
        config: config(),
        status: "running".into(),
        round: 1,
        revision: 0,
        workers: vec![],
        error: None,
        summary: None,
        updated_at: storage::now(),
        generation: 1,
        objective: "Build it".into(),
        reviewed_revision: 0,
        dispatch_revision: 0,
        pending_turn: None,
        pending_setup: None,
        coordination_revision: 0,
        reviewed_coordination_revision: 0,
        legacy_prompt_through_id: Some(0),
        continued_revision: None,
        last_turn_kind: None,
        objective_start: 0,
        timed_out: false,
        briefings: Default::default(),
    };
    (store, team)
}
fn worker(id: &str, status: &str, files: &[&str], depends: &[&str]) -> Worker {
    Worker {
        id: id.into(),
        session_id: format!("task-{id}"),
        name: id.into(),
        profile_id: "fast".into(),
        provider: "codex".into(),
        model: "default".into(),
        reasoning_effort: None,
        status: status.into(),
        assignment: "Small task".into(),
        summary: String::new(),
        error: None,
        depends_on: depends.iter().map(|x| x.to_string()).collect(),
        files: files.iter().map(|x| x.to_string()).collect(),
        read_only: false,
        attempt: 1,
        created_at: 1,
        updated_at: 1,
        keys: vec![format!("key-{id}")],
        abandoned: false,
        result_revision: 0,
        checkpoint: None,
        timed_out: false,
    }
}
#[test]
fn scopes_serialize_whole_checkout_directories_and_exact_files() {
    assert!(overlaps(&[], &["src/a.rs".into()]));
    assert!(overlaps(&["src".into()], &["src/a.rs".into()]));
    assert!(overlaps(&["src/a.rs".into()], &["src/a.rs".into()]));
    assert!(!overlaps(&["src/a".into()], &["src/ab".into()]));
    assert!(!overlaps(&["tests".into()], &["src".into()]));
}
#[test]
fn worker_summaries_explain_queue_reservations_and_access() {
    let (_, mut team) = fixture();
    team.workers = vec![
        worker("editor", "running", &["src"], &[]),
        worker("blocked", "queued", &["src/main.rs"], &[]),
        worker("dependent", "queued", &["tests"], &["editor"]),
        worker("reviewer", "queued", &[], &[]),
    ];
    team.workers[3].read_only = true;
    let result = summaries(&team, 0);
    assert_eq!(result["workers"][1]["queueReason"], "Waiting for editor to finish editing shared files");
    assert_eq!(result["workers"][2]["queueReason"], "Waiting for a dependency");
    assert_eq!(result["workers"][3]["queueReason"], Value::Null);
    assert_eq!(result["workers"][3]["readOnly"], true);
    assert_eq!(result["workers"][1]["files"], json!(["src/main.rs"]));
    team.config.max_parallel = 1;
    assert_eq!(summaries(&team, 0)["workers"][3]["queueReason"], "Waiting for an available worker slot");
    team.workers[0].status = "completed".into();
    assert_eq!(summaries(&team, 0)["workers"][1]["queueReason"], Value::Null);
}
#[test]
fn independent_workers_are_eligible_but_dependencies_and_overlaps_wait() {
    let (_, mut team) = fixture();
    team.workers = vec![
        worker("a", "running", &["src"], &[]),
        worker("b", "queued", &["tests"], &[]),
        worker("c", "queued", &["src/new.rs"], &[]),
        worker("d", "queued", &["docs"], &["a"]),
    ];
    assert!(eligible(&team, 1));
    assert!(!eligible(&team, 2));
    assert!(!eligible(&team, 3));
    team.workers[0].status = "completed".into();
    assert!(eligible(&team, 2));
    assert!(eligible(&team, 3));
}
#[test]
fn scope_validation_rejects_escapes_and_globs() {
    for path in [
        "../secret",
        "/private/key",
        "src/../key",
        "src//key",
        "src/*.rs",
        "src\\key",
    ] {
        assert!(validate_files(&[path.into()]).is_err(), "{path}");
    }
    assert!(validate_files(&["src/api.rs".into(), "tests".into()]).is_ok());
}
#[test]
fn config_enforces_bounds_and_unique_profiles() {
    let mut c = config();
    assert!(c.validate().is_ok());
    for parallel in [0, 4, 128] {
        c.max_parallel = parallel;
        assert!(c.validate().is_ok());
    }
    c.max_rounds = 0;
    assert!(c.validate().is_err());
    c.max_rounds = 5;
    c.workers.push(c.workers[0].clone());
    assert!(c.validate().is_err());
}
#[test]
fn stale_and_manual_admissions_cannot_restart_a_paused_team() {
    let (store, mut team) = fixture();
    save(&store, &mut team).unwrap();
    let admission = Admission {
        parent: team.session_id.clone(),
        generation: 1,
    };
    assert!(validate_admission(&store, &team.session_id, Some(&admission)).is_ok());
    assert!(validate_admission(&store, &team.session_id, None).is_err());
    team.status = "paused".into();
    team.generation = 2;
    save(&store, &mut team).unwrap();
    assert!(validate_admission(&store, &team.session_id, Some(&admission)).is_err());
    assert!(validate_admission(&store, &team.session_id, None).is_err());
}
#[test]
fn managed_workers_are_hidden_from_task_lists_but_have_real_transcripts() {
    let (store, mut team) = fixture();
    let parent = store.session(&team.session_id).unwrap();
    let child = store
        .create_session(&parent.project_id, "kimi", "default", "ask")
        .unwrap();
    store
        .db
        .execute(
            "INSERT INTO architect_workers VALUES(?1,'child',?2)",
            params![parent.id, child.id],
        )
        .unwrap();
    save(&store, &mut team).unwrap();
    store
        .message(&child.id, "assistant", "Worker result")
        .unwrap();
    assert!(!store.sessions().unwrap().iter().any(|s| s.id == child.id));
    assert_eq!(
        store.messages(&child.id, None).unwrap()[0].content,
        "Worker result"
    );
    assert!(check_manual(&store, &child.id).is_err());
}
#[test]
fn compact_prompts_do_not_include_parent_transcript() {
    let (store, mut team) = fixture();
    store
        .message(&team.session_id, "assistant", "PRIVATE LARGE OLD CONTEXT")
        .unwrap();
    team.workers.push(worker("a", "running", &["src"], &[]));
    save(&store, &mut team).unwrap();
    let p = prompt(
        &store,
        "task-a",
        "Fix parsing",
        Some(&Admission {
            parent: team.session_id.clone(),
            generation: 1,
        }),
    )
    .unwrap();
    assert!(!p.contains("PRIVATE LARGE OLD CONTEXT"));
    assert!(p.contains("Fix parsing"));
    assert!(p.contains("src"));
}
#[test]
fn unicode_summary_truncation_is_bounded() {
    let result = bounded(&"🌿".repeat(1000), 513);
    assert!(result.len() < 540);
    assert!(result.ends_with("[Truncated]"));
}
#[test]
fn restart_marks_team_interrupted_without_relaunching_workers() {
    let path = std::env::temp_dir().join(format!("pandacode-team-test-{}.db", storage::id()));
    let sid;
    {
        let store = Store::open(&path).unwrap();
        let p = store.add_project(&std::env::temp_dir()).unwrap();
        let parent = store
            .create_session(&p.id, "codex", "default", "ask")
            .unwrap();
        sid = parent.id.clone();
        let (_, mut team) = fixture();
        team.session_id = sid.clone();
        team.workers = vec![
            worker("a", "running", &[], &[]),
            worker("b", "queued", &[], &[]),
        ];
        save(&store, &mut team).unwrap();
    }
    let store = Store::open(&path).unwrap();
    let team = load(&store, &sid).unwrap().unwrap();
    assert_eq!(team.status, "interrupted");
    assert_eq!(team.workers[0].status, "stopped");
    assert_eq!(team.workers[1].status, "queued");
    assert_eq!(team.generation, 2);
    drop(store);
    let _ = std::fs::remove_file(path);
}
fn state_fixture() -> (AppState, String) {
    let (store, mut team) = fixture();
    team.config.workers.push(WorkerProfile {
        id: "kimi".into(),
        name: "Kimi worker".into(),
        provider: "kimi".into(),
        model: String::new(),
        reasoning_effort: None,
    });
    let sid = team.session_id.clone();
    save(&store, &mut team).unwrap();
    (
        AppState {
            store: Arc::new(std::sync::Mutex::new(store)),
            agents: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            terminals: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            shutdown: std::sync::atomic::AtomicU8::new(0),
        },
        sid,
    )
}
fn call(state: &AppState, sid: &str, name: &str, args: Value) -> Result<Value> {
    let gen = load(&state.store.lock().unwrap(), sid)?.unwrap().generation;
    dispatch_state(state, sid, gen, name, args, &AtomicBool::new(false)).map(|x| x.0)
}
fn assignment(key: &str, profile: &str, file: &str) -> Value {
    json!({"key":key,"profileId":profile,"name":key,"prompt":format!("Implement {key} and verify its behavior."),"files":[file]})
}
fn claim(
    state: &AppState,
    action: Launch,
) -> (
    crate::PreparedRun,
    tokio::sync::mpsc::UnboundedReceiver<RunInput>,
) {
    let (tx, rx) = crate::unbounded_channel();
    let prepared = crate::prepare_task_run_in_project(
        state,
        action.sid,
        Some(action.prompt),
        vec![],
        tx,
        Arc::new(AtomicBool::new(false)),
        crate::RunAdmission {
            team: Some(action.admission),
            ..Default::default()
        },
    )
    .unwrap();
    (prepared, rx)
}
fn finish(state: &AppState, prepared: &crate::PreparedRun, answer: &str, success: bool) {
    let sid = &prepared.config.session_id;
    let mut agents = state.agents.lock().unwrap();
    {
        let store = state.store.lock().unwrap();
        store.message(sid, "assistant", answer).unwrap();
        store.finish_task_turn(sid, success, false).unwrap();
        agents.remove(sid);
    }
    completed_with_agents(
        state,
        &agents,
        sid,
        prepared.team.as_ref(),
        success,
        false,
        &mut |_| {},
    );
}
#[test]
fn switching_live_team_keeps_current_turn_and_worker_snapshots_until_boundary() {
    let (state, sid) = state_fixture();
    {
        let store = state.store.lock().unwrap();
        store.resume(&sid, "original-native-context").unwrap();
        store.db.execute("UPDATE sessions SET mode='plan',context_used_tokens=123,context_window=1000,codex_settings=?2 WHERE id=?1",
            params![sid, r#"{"personality":"friendly"}"#]).unwrap();
    }
    let (parent, _rx) = claim(&state, Launch {
        sid: sid.clone(), prompt: "Build the feature".into(), images: None,
        admission: Admission { parent: sid.clone(), generation: 1 },
    });
    let (worker, _) = running_worker(&state, &sid, "original", "src");
    let before = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    let mut changed = before.config.clone();
    changed.workers[0].id = "replacement".into();
    changed.workers[0].model = "next-worker-model".into();
    let selection = profiles::ModelSelection {
        provider: "codex".into(), model: "next-architect-model".into(), reasoning_effort: Some("high".into()),
    };
    let queued = configure_state(&state, &sid, Some(changed.clone()), Some(selection.clone()), None)
        .expect("A live team should accept its next setup").unwrap();
    assert_eq!(queued.config, before.config);
    assert_eq!(state.store.lock().unwrap().session(&sid).unwrap().provider, "claude");
    assert_eq!(state.store.lock().unwrap().session(&sid).unwrap().resume_id.as_deref(), Some("original-native-context"));
    assert_eq!(queued.generation, before.generation);
    assert_eq!(queued.round, before.round);
    assert_eq!(queued.pending_setup.as_ref().unwrap().architect, selection);
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    finish(&state, &parent, "Waiting for the original worker", true);
    let applied = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    assert_eq!(applied.config, changed);
    assert!(applied.pending_setup.is_none());
    assert_eq!(applied.workers[0].model, before.workers[0].model);
    assert_eq!(applied.workers[0].provider, worker.config.provider);
    {
        let store = state.store.lock().unwrap();
        let session = store.session(&sid).unwrap();
        assert_eq!(session.mode, "plan");
        assert!(session.resume_id.is_none());
        assert!(session.codex_settings.is_none());
        assert!(session.context_usage.is_none());
    }
    finish(&state, &worker, "Original assignment done", true);
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (review, _) = claim(&state, action);
    assert_eq!(review.config.provider, selection.provider);
    assert_eq!(review.config.model, selection.model);
    assert!(review.config.prompt.contains("Waiting for the original worker"));
    assert!(review.config.prompt.contains("replacement"));
    assert!(review.config.prompt.contains("Original assignment done"));
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
}

#[test]
fn live_team_switch_is_atomic_latest_wins_and_selecting_current_cancels_it() {
    let (state, sid) = state_fixture();
    let (parent, _rx) = claim(&state, Launch {
        sid: sid.clone(), prompt: "Build the feature".into(), images: None,
        admission: Admission { parent: sid.clone(), generation: 1 },
    });
    let (original, selection) = {
        let store = state.store.lock().unwrap();
        store.resume(&sid, "original-context").unwrap();
        (load(&store, &sid).unwrap().unwrap().config, profiles::ModelSelection::from_session(&store.session(&sid).unwrap()))
    };
    let mut changed = selection.clone();
    changed.model = "next-model".into();
    configure_state(&state, &sid, Some(original.clone()), Some(changed.clone()), None).unwrap();
    let before = serde_json::to_value(load(&state.store.lock().unwrap(), &sid).unwrap()).unwrap();
    changed.model = "invalid-profile-request".into();
    assert!(configure_state(&state, &sid, Some(original.clone()), Some(changed.clone()), Some("missing-profile")).is_err());
    assert_eq!(serde_json::to_value(load(&state.store.lock().unwrap(), &sid).unwrap()).unwrap(), before);
    changed.model = "latest-model".into();
    let latest = configure_state(&state, &sid, Some(original.clone()), Some(changed), None).unwrap().unwrap();
    assert_eq!(latest.pending_setup.unwrap().architect.model, "latest-model");
    let current = configure_state(&state, &sid, Some(original), Some(selection), None).unwrap().unwrap();
    assert!(current.pending_setup.is_none());
    finish(&state, &parent, "Nothing else needed", true);
    let store = state.store.lock().unwrap();
    assert_eq!(store.session(&sid).unwrap().resume_id.as_deref(), Some("original-context"));
    assert!(!crate::context_recovery::pending(&store, &store.session(&sid).unwrap()).unwrap());
    assert_eq!(store.messages(&sid, None).unwrap().len(), 2);
}

#[test]
fn stopping_with_a_pending_team_switch_applies_after_exit_without_resuming() {
    for action in ["pause", "stop"] {
        let (state, sid) = state_fixture();
        let (parent, _rx) = claim(&state, Launch {
            sid: sid.clone(), prompt: "Build the feature".into(), images: None,
            admission: Admission { parent: sid.clone(), generation: 1 },
        });
        let config = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap().config;
        let selection = profiles::ModelSelection { provider: "codex".into(), model: "new-model".into(), reasoning_effort: None };
        configure_state(&state, &sid, Some(config), Some(selection), None).unwrap();
        let paused = control_state(&state, &sid, action).unwrap();
        assert!(paused.pending_setup.is_some());
        assert_eq!(state.store.lock().unwrap().session(&sid).unwrap().provider, "claude");
        finish(&state, &parent, "Stopped with partial progress", false);
        let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
        assert_eq!(team.status, "paused");
        assert_eq!(team.generation, paused.generation);
        assert!(team.pending_setup.is_none());
        assert_eq!(state.store.lock().unwrap().session(&sid).unwrap().provider, "codex");
        assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
        let store = state.store.lock().unwrap();
        let (prompt, _) = crate::context_recovery::prepare(&store, &store.session(&sid).unwrap(), "Resume").unwrap();
        assert!(prompt.contains("Stopped with partial progress"));
    }
}

#[test]
fn switching_a_reserved_review_preserves_its_single_turn_and_assignment_context() {
    let (state, sid) = state_fixture();
    let (worker, _) = running_worker(&state, &sid, "original", "src");
    finish(&state, &worker, "Finished work", true);
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let before = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    assert!(before.pending_turn.as_ref().unwrap().reserved);
    let mut changed = before.config.clone();
    changed.workers[0].id = "replacement".into();
    let selection = profiles::ModelSelection { provider: "codex".into(), model: "new-model".into(), reasoning_effort: None };
    let applied = configure_state(&state, &sid, Some(changed), Some(selection), None).unwrap().unwrap();
    assert!(applied.pending_setup.is_none());
    assert_eq!(applied.pending_turn.as_ref().unwrap().prompt, before.pending_turn.as_ref().unwrap().prompt);
    assert_eq!(applied.round, before.round);
    assert_eq!(applied.reviewed_revision, before.reviewed_revision);
    assert_eq!(applied.generation, before.generation);
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    let (review, _) = claim(&state, action);
    assert_eq!(review.config.model, "new-model");
    assert!(review.config.prompt.contains("replacement"));
    assert!(review.config.prompt.contains("Finished work"));
    let store = state.store.lock().unwrap();
    assert_eq!(store.messages(&sid, None).unwrap().len(), 1);
    assert_eq!(store.messages(&sid, None).unwrap()[0].role, "coordination");
}

#[test]
fn queued_switch_enforces_lower_assignment_limit_and_rejects_used_limits() {
    let (state, sid) = state_fixture();
    let (_parent, _rx) = claim(&state, Launch {
        sid: sid.clone(), prompt: "Build the feature".into(), images: None,
        admission: Admission { parent: sid.clone(), generation: 1 },
    });
    call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[assignment("first","fast","src/a")]})).unwrap();
    let mut config = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap().config;
    config.max_assignments = 1;
    configure_state(&state, &sid, Some(config.clone()), None, None).unwrap();
    assert!(call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[assignment("extra","fast","src/b")]})).unwrap_err().contains("Assignment limit"));
    config.max_rounds = 0;
    assert!(configure_state(&state, &sid, Some(config), None, None).is_err());
    let mut team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    team.round = 3;
    save(&state.store.lock().unwrap(), &mut team).unwrap();
    let mut config = team.config.clone();
    config.max_rounds = 2;
    assert!(configure_state(&state, &sid, Some(config), None, None).unwrap_err().contains("below the current round"));
}

#[test]
fn simultaneous_review_claim_and_team_switch_choose_one_safe_boundary() {
    for _ in 0..12 {
        let (state, sid) = state_fixture();
        let (worker, _) = running_worker(&state, &sid, "original", "src");
        finish(&state, &worker, "Reviewed artifact", true);
        let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
        let before = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
        let barrier = std::sync::Barrier::new(2);
        let (review, switched) = std::thread::scope(|scope| {
            let claim = scope.spawn(|| {
                barrier.wait();
                claim(&state, action).0
            });
            let switch = scope.spawn(|| {
                barrier.wait();
                configure_state(&state, &sid, Some(before.config.clone()), Some(profiles::ModelSelection {
                    provider: "codex".into(), model: "next-model".into(), reasoning_effort: None,
                }), None).unwrap().unwrap()
            });
            (claim.join().unwrap(), switch.join().unwrap())
        });
        assert_eq!(review.config.provider, if switched.pending_setup.is_some() { "claude" } else { "codex" });
        assert_eq!(switched.generation, before.generation);
        assert_eq!(switched.round, before.round);
        assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
        finish(&state, &review, "Review complete", true);
        let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
        assert!(team.pending_setup.is_none());
        assert_eq!(state.store.lock().unwrap().session(&sid).unwrap().provider, "codex");
        // A review that neither dispatches nor finishes gets one continue turn, never a second review.
        let next = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
        assert!(next.prompt.starts_with("You ended your turn without dispatching workers"));
        assert_eq!(load(&state.store.lock().unwrap(), &sid).unwrap().unwrap().round, before.round + 1);
        assert_eq!(state.store.lock().unwrap().messages(&sid, None).unwrap().len(), 2);
    }
}
#[test]
fn mixed_harness_delegation_review_followup_and_finish_use_real_task_lifecycle() {
    let (state, sid) = state_fixture();
    let parent_action = Launch {
        sid: sid.clone(),
        prompt: "Build the feature".into(),
        images: None,
        admission: Admission {
            parent: sid.clone(),
            generation: 1,
        },
    };
    let (parent, _parent_rx) = claim(&state, parent_action);
    let result=call(&state,&sid,"pandacode_dispatch_workers",json!({"assignments":[assignment("engine","fast","src"),assignment("tests","kimi","tests")]})).unwrap();
    let first_id = result["workerIds"][0].as_str().unwrap().to_string();
    let a = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (first, _first_rx) = claim(&state, a);
    let b = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (second, _second_rx) = claim(&state, b);
    assert_ne!(first.config.provider, second.config.provider);
    assert_eq!(first.config.cwd, second.config.cwd);
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    finish(
        &state,
        &parent,
        "Workers are implementing separate areas.",
        true,
    );
    finish(&state, &first, "Engine checks passed.", true);
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    finish(&state, &second, "Tests passed on Kimi.", true);
    let review = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert_eq!(review.sid, sid);
    assert!(review.prompt.contains("Engine checks passed"));
    assert!(review.prompt.contains("Tests passed on Kimi"));
    let (reviewer, _review_rx) = claim(&state, review);
    call(&state,&sid,"pandacode_followup_worker",json!({"workerId":first_id,"key":"engine-fix","prompt":"Handle the missing empty input case."})).unwrap();
    finish(&state, &reviewer, "Correct the edge case.", true);
    let retry = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (retried, _retry_rx) = claim(&state, retry);
    assert_eq!(retried.config.session_id, first.config.session_id);
    finish(&state, &retried, "Empty input case now passes.", true);
    let review = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert!(review.prompt.contains("Empty input case now passes"));
    assert!(!review.prompt.contains("Tests passed on Kimi"));
    let (final_review, _final_rx) = claim(&state, review);
    call(
        &state,
        &sid,
        "pandacode_finish_team",
        json!({"summary":"Implemented and verified the feature."}),
    )
    .unwrap();
    finish(&state, &final_review, "Implemented and verified.", true);
    let store = state.store.lock().unwrap();
    assert_eq!(load(&store, &sid).unwrap().unwrap().status, "completed");
    assert_eq!(store.session(&sid).unwrap().task_stage, "review");
    assert_eq!(store.sessions().unwrap().len(), 1);
}
#[test]
fn batch_idempotency_rejects_different_arguments_and_rolls_back_invalid_batches() {
    let (state, sid) = state_fixture();
    let batch = json!({"assignments":[assignment("engine","fast","src")]});
    let first = call(&state, &sid, "pandacode_dispatch_workers", batch.clone()).unwrap();
    let repeat = call(&state, &sid, "pandacode_dispatch_workers", batch).unwrap();
    assert_eq!(first, repeat);
    assert!(call(
        &state,
        &sid,
        "pandacode_dispatch_workers",
        json!({"assignments":[assignment("engine","kimi","src")]})
    )
    .is_err());
    assert!(call(
        &state,
        &sid,
        "pandacode_dispatch_workers",
        json!({"assignments":[assignment("new","fast","new"),assignment("bad","missing","bad")]})
    )
    .is_err());
    let store = state.store.lock().unwrap();
    assert_eq!(load(&store, &sid).unwrap().unwrap().workers.len(), 1);
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM architect_requests", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn pause_between_scheduling_and_claim_prevents_a_worker_process_and_stale_completion() {
    let (state, sid) = state_fixture();
    call(
        &state,
        &sid,
        "pandacode_dispatch_workers",
        json!({"assignments":[assignment("engine","fast","src")]}),
    )
    .unwrap();
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let before = action.admission.clone();
    let child = action.sid.clone();
    control_state(&state, &sid, "pause").unwrap();
    let (tx, _rx) = crate::unbounded_channel();
    assert!(crate::prepare_task_run_in_project(
        &state,
        action.sid,
        Some(action.prompt),
        vec![],
        tx,
        Arc::new(AtomicBool::new(false)),
        crate::RunAdmission {
            team: Some(action.admission),
            ..Default::default()
        }
    )
    .is_err());
    completed_state(&state, &child, Some(&before), true, false, &mut |_| {});
    let store = state.store.lock().unwrap();
    let team = load(&store, &sid).unwrap().unwrap();
    assert_eq!(team.status, "paused");
    assert_eq!(team.workers[0].status, "stopped");
    assert!(store.messages(&child, None).unwrap().is_empty());
}

#[test]
fn dispatch_requires_explicit_file_scopes_without_persisting_any_part_of_the_batch() {
    let (state, sid) = state_fixture();
    let mut unscoped = assignment("unscoped", "fast", "src");
    unscoped.as_object_mut().unwrap().remove("files");
    let result = call(&state, &sid, "pandacode_dispatch_workers", json!({
        "assignments": [assignment("valid", "fast", "tests"), unscoped]
    }));
    assert!(result.is_err(), "omitted scopes must not silently reserve the whole checkout");
    let store = state.store.lock().unwrap();
    assert!(load(&store, &sid).unwrap().unwrap().workers.is_empty());
    for (table, expected) in [("sessions", 1), ("architect_workers", 0), ("architect_requests", 0)] {
        let count: i64 = store.db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).unwrap();
        assert_eq!(count, expected, "{table}");
    }
    let tools = catalog();
    let dispatch = tools.iter().find(|tool| tool["name"] == "pandacode_dispatch_workers").unwrap();
    assert!(dispatch["inputSchema"]["properties"]["assignments"]["items"]["required"]
        .as_array().unwrap().contains(&json!("files")));
}

#[test]
fn read_only_workers_overlap_writers_under_finite_and_unlimited_parallel_limits() {
    for limit in [0, 2, 3] {
        for writer_first in [true, false] {
            let (state, sid) = state_fixture();
            {
                let store = state.store.lock().unwrap();
                let mut team = load(&store, &sid).unwrap().unwrap();
                team.config.max_parallel = limit;
                save(&store, &mut team).unwrap();
            }
            let mut review = assignment("review", "fast", "src");
            review["readOnly"] = json!(true);
            let mut research = assignment("research", "fast", "src");
            research["files"] = json!([]);
            research["readOnly"] = json!(true);
            let writer = assignment("writer", "fast", "src");
            let mut assignments = if writer_first { vec![writer, review, research] } else { vec![review, research, writer] };
            assignments.push(assignment("overlapping-writer", "fast", "src/lib.rs"));
            call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments": assignments})).unwrap();
            let mut running = Vec::new();
            while let Some(action) = next_action_state(&state, &mut |_| {}).unwrap() {
                assert!(running.len() < 3, "a second overlapping writer must wait");
                running.push(claim(&state, action));
            }
            assert_eq!(running.len(), if limit == 2 { 2 } else { 3 });
            if limit == 2 {
                let review = running.iter().find(|(run, _)| run.config.prompt.contains("Implement review")).unwrap();
                finish(&state, &review.0, "Review complete", true);
                let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
                running.push(claim(&state, action));
                assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
            }
            let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
            assert_eq!(team.workers.iter().find(|worker| worker.name == "writer").unwrap().status, "running");
            assert_eq!(team.workers.iter().find(|worker| worker.name == "overlapping-writer").unwrap().status, "queued");
        }
    }
}

#[test]
fn read_only_worker_followups_inherit_or_change_access_only_after_settlement() {
    let (state, sid) = state_fixture();
    let mut review = assignment("review", "fast", "src");
    review["readOnly"] = json!(true);
    let ids = call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments": [review]})).unwrap();
    let worker_id = ids["workerIds"][0].clone();
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (first, _rx) = claim(&state, action);
    assert!(first.config.prompt.contains("Do not edit, create, or delete files"));
    assert!(call(&state, &sid, "pandacode_followup_worker", json!({
        "workerId": worker_id, "key": "too-early", "prompt": "Apply the fix", "readOnly": false
    })).unwrap_err().contains("settle"));
    finish(&state, &first, "Reviewed", true);
    call(&state, &sid, "pandacode_followup_worker", json!({
        "workerId": worker_id, "key": "review-again", "prompt": "Review again"
    })).unwrap();
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (second, _rx) = claim(&state, action);
    assert!(second.config.prompt.contains("Do not edit, create, or delete files"));
    finish(&state, &second, "Reviewed again", true);
    call(&state, &sid, "pandacode_followup_worker", json!({
        "workerId": worker_id, "key": "apply-fix", "prompt": "Apply the fix", "readOnly": false, "files": ["src/fix.rs"]
    })).unwrap();
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (third, _rx) = claim(&state, action);
    assert!(!third.config.prompt.contains("Do not edit, create, or delete files"));
    let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    let serialized = serde_json::to_value(&team.workers[0]).unwrap();
    assert_eq!(serialized["readOnly"], false);
    assert_eq!(serialized["files"], json!(["src/fix.rs"]));
}

#[test]
fn legacy_worker_snapshots_and_writer_request_keys_keep_their_original_meaning() {
    let original = worker("legacy", "queued", &["src"], &[]);
    let mut serialized = serde_json::to_value(original).unwrap();
    serialized.as_object_mut().unwrap().remove("readOnly");
    let restored: Worker = serde_json::from_value(serialized).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap()["readOnly"], false);
    let parsed: Assignment = serde_json::from_value(assignment("writer", "fast", "src")).unwrap();
    let normalized = serde_json::to_value(parsed).unwrap();
    assert!(normalized.get("readOnly").is_none(), "default access must not change persisted idempotency request content");
}
#[test]
fn unlimited_team_launches_more_than_four_agents_without_bypassing_scope_ownership() {
    let (state, sid) = state_fixture();
    {
        let store = state.store.lock().unwrap();
        let mut team = load(&store, &sid).unwrap().unwrap();
        team.config.max_parallel = 0;
        save(&store, &mut team).unwrap();
    }
    let parent_action = Launch {
        sid: sid.clone(), prompt: "Build independent parts".into(), images: None,
        admission: Admission { parent: sid.clone(), generation: 1 },
    };
    let (_parent, _parent_rx) = claim(&state, parent_action);
    let mut assignments = (0..8).map(|index| assignment(&format!("part-{index}"), "fast", &format!("part-{index}.rs"))).collect::<Vec<_>>();
    assignments.push(assignment("overlap", "fast", "part-0.rs"));
    call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":assignments})).unwrap();
    let mut running = Vec::new();
    while let Some(action) = next_action_state(&state, &mut |_| {}).unwrap() {
        assert!(running.len() < 8, "overlapping work must remain queued");
        running.push(claim(&state, action));
    }
    assert_eq!(running.len(), 8);
    assert_eq!(state.agents.lock().unwrap().len(), 9);
    finish(&state, &running[0].0, "First scope is ready", true);
    let overlap = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (_replacement, _replacement_rx) = claim(&state, overlap);
    assert_eq!(state.agents.lock().unwrap().len(), 9);
}

#[test]
fn explicit_team_parallel_limit_still_queues_independent_workers() {
    let (state, sid) = state_fixture();
    let assignments = (0..5).map(|index| assignment(&format!("part-{index}"), "fast", &format!("part-{index}.rs"))).collect::<Vec<_>>();
    call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":assignments})).unwrap();
    let mut running = Vec::new();
    while let Some(action) = next_action_state(&state, &mut |_| {}).unwrap() {
        running.push(claim(&state, action));
    }
    assert_eq!(running.len(), 3);
    finish(&state, &running[0].0, "First scope is ready", true);
    let next = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert_ne!(next.sid, running[0].0.config.session_id);
}
#[test]
fn different_teams_in_same_checkout_serialize_overlapping_scopes() {
    let (state, sid) = state_fixture();
    let second_sid = {
        let store = state.store.lock().unwrap();
        let parent = store.session(&sid).unwrap();
        let second = store
            .create_session(&parent.project_id, "kimi", "default", "ask")
            .unwrap();
        let mut team = load(&store, &sid).unwrap().unwrap();
        team.session_id = second.id.clone();
        save(&store, &mut team).unwrap();
        second.id
    };
    for parent in [&sid, &second_sid] {
        call(
            &state,
            parent,
            "pandacode_dispatch_workers",
            json!({"assignments":[assignment("engine","fast","src")]}),
        )
        .unwrap();
    }
    let first = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (first, _rx) = claim(&state, first);
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    finish(&state, &first, "Done", true);
    let next = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert!(next.sid != first.config.session_id);
}
#[test]
fn read_only_scopes_allow_parallel_workers_across_teams_in_the_same_checkout() {
    for (first_read_only, second_read_only) in [(true, true), (true, false), (false, true)] {
        let (state, sid) = state_fixture();
        let second_sid = {
            let store = state.store.lock().unwrap();
            let parent = store.session(&sid).unwrap();
            let second = store.create_session(&parent.project_id, "kimi", "default", "ask").unwrap();
            let mut team = load(&store, &sid).unwrap().unwrap();
            team.session_id = second.id.clone();
            save(&store, &mut team).unwrap();
            second.id
        };
        for (parent, read_only) in [(&sid, first_read_only), (&second_sid, second_read_only)] {
            let mut task = assignment("shared-scope", "fast", "src");
            task["files"] = json!([]);
            task["readOnly"] = json!(read_only);
            call(&state, parent, "pandacode_dispatch_workers", json!({"assignments": [task]})).unwrap();
        }
        let mut running = Vec::new();
        for _ in 0..2 {
            let action = next_action_state(&state, &mut |_| {}).unwrap().expect("read-only work must not reserve the checkout against another team");
            running.push(claim(&state, action));
        }
        assert_eq!(state.agents.lock().unwrap().len(), 2);
    }
}

#[test]
fn permission_revocation_applies_to_worker_retry_and_failed_work_can_be_abandoned() {
    let (state, sid) = state_fixture();
    {
        let store = state.store.lock().unwrap();
        store
            .db
            .execute("UPDATE sessions SET mode='full' WHERE id=?1", [&sid])
            .unwrap();
    }
    let ids = call(
        &state,
        &sid,
        "pandacode_dispatch_workers",
        json!({"assignments":[assignment("engine","fast","src")]}),
    )
    .unwrap();
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (worker, _rx) = claim(&state, action);
    assert_eq!(worker.config.mode, "full");
    finish(&state, &worker, "Blocked.", false);
    control_state(&state, &sid, "pause").unwrap();
    state
        .store
        .lock()
        .unwrap()
        .update(&sid, None, None, None, Some("ask".into()))
        .unwrap();
    control_state(&state, &sid, "resume").unwrap();
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (retry, _rx) = claim(&state, action);
    assert_eq!(retry.config.mode, "ask");
    finish(&state, &retry, "Still blocked.", false);
    call(&state,&sid,"pandacode_cancel_worker",json!({"workerId":ids["workerIds"][0],"reason":"Superseded by a smaller validated implementation."})).unwrap();
    assert!(call(
        &state,
        &sid,
        "pandacode_finish_team",
        json!({"summary":"Scope delivered; superseded worker intentionally omitted."})
    )
    .is_ok());
}
#[test]
fn round_and_attempt_limits_stop_automatic_spending_until_explicit_action() {
    let (state, sid) = state_fixture();
    let result = call(
        &state,
        &sid,
        "pandacode_dispatch_workers",
        json!({"assignments":[assignment("engine","fast","src")]}),
    )
    .unwrap();
    let worker_id = result["workerIds"][0].as_str().unwrap();
    for attempt in 1..=3 {
        let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
        let (worker, _rx) = claim(&state, action);
        finish(
            &state,
            &worker,
            "Blocked by an unresolved interface.",
            false,
        );
        let follow = call(
            &state,
            &sid,
            "pandacode_followup_worker",
            json!({"workerId":worker_id,"key":format!("fix-{attempt}"),"prompt":"Resolve the interface first."}),
        );
        if attempt < 3 {
            assert!(follow.is_ok());
        } else {
            assert!(follow.is_err());
        }
    }
    {
        let store = state.store.lock().unwrap();
        let mut team = load(&store, &sid).unwrap().unwrap();
        team.config.max_rounds = 1;
        save(&store, &mut team).unwrap();
    }
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    assert_eq!(
        load(&state.store.lock().unwrap(), &sid)
            .unwrap()
            .unwrap()
            .status,
        "paused"
    );
    // An explicit Resume at the limit grants a few more rounds, never past the maximum.
    let round = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap().round;
    let resumed = control_state(&state, &sid, "resume").unwrap();
    assert_eq!(resumed.config.max_rounds, (round + EXTRA_ROUNDS).min(MAX_ROUNDS));
    control_state(&state, &sid, "pause").unwrap();
    {
        let store = state.store.lock().unwrap();
        let mut team = load(&store, &sid).unwrap().unwrap();
        team.round = MAX_ROUNDS;
        team.config.max_rounds = MAX_ROUNDS;
        save(&store, &mut team).unwrap();
    }
    assert!(control_state(&state, &sid, "resume").unwrap_err().contains("Send a message"));
}
#[test]
fn dependency_failure_is_reported_instead_of_leaving_a_permanent_queue() {
    let (state, sid) = state_fixture();
    let first = call(
        &state,
        &sid,
        "pandacode_dispatch_workers",
        json!({"assignments":[assignment("engine","fast","src")]}),
    )
    .unwrap();
    let mut dependent = assignment("dependent", "kimi", "tests");
    dependent["dependsOn"] = json!([first["workerIds"][0]]);
    call(
        &state,
        &sid,
        "pandacode_dispatch_workers",
        json!({"assignments":[dependent]}),
    )
    .unwrap();
    let action = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (worker, _rx) = claim(&state, action);
    finish(&state, &worker, "Missing requirement.", false);
    let review = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert_eq!(review.sid, sid);
    let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    assert_eq!(team.workers[1].status, "failed");
    assert!(team.workers[1]
        .error
        .as_deref()
        .unwrap()
        .contains("dependency"));
}
#[test]
fn resume_before_first_process_keeps_the_objective_and_recovers_reserved_review_results() {
    let (state, sid) = state_fixture();
    {
        let store = state.store.lock().unwrap();
        let mut team = load(&store, &sid).unwrap().unwrap();
        team.objective = "Implement a Minesweeper game with keyboard support".into();
        team.pending_turn = Some(PendingTurn {
            prompt: team.objective.clone(),
            images: None,
            reserved: true,
            kind: None,
        });
        save(&store, &mut team).unwrap();
    }
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    control_state(&state, &sid, "pause").unwrap();
    control_state(&state, &sid, "resume").unwrap();
    let resumed = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert!(resumed
        .prompt
        .contains("Minesweeper game with keyboard support"));
    assert!(state
        .store
        .lock()
        .unwrap()
        .session(&sid)
        .unwrap()
        .resume_id
        .is_none());
}

#[test]
fn legacy_dispatch_retry_without_files_reuses_the_saved_whole_checkout_assignment() {
    let (state, sid) = state_fixture();
    let mut original = assignment("legacy-whole-checkout", "fast", "src");
    original["files"] = json!([]);
    let first = call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments": [original.clone()]})).unwrap();
    let revision = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap().revision;
    original.as_object_mut().unwrap().remove("files");
    let retry = call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments": [original]})).unwrap();
    assert_eq!(retry["workerIds"], first["workerIds"]);
    let store = state.store.lock().unwrap();
    let team = load(&store, &sid).unwrap().unwrap();
    assert_eq!(team.revision, revision, "a legacy retry must not dispatch new work");
    assert_eq!(team.workers.len(), 1);
    assert!(team.workers[0].files.is_empty());
    assert!(!team.workers[0].read_only);
    for (table, expected) in [("sessions", 2), ("architect_workers", 1), ("architect_requests", 1)] {
        let count: i64 = store.db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).unwrap();
        assert_eq!(count, expected, "{table}");
    }
}

fn auto_state() -> (AppState, String) {
    let (state, sid) = state_fixture();
    {
        let store = state.store.lock().unwrap();
        let mut team = load(&store, &sid).unwrap().unwrap();
        team.config.auto = true;
        save(&store, &mut team).unwrap();
    }
    (state, sid)
}
fn auto_assignment(key: &str, extra: Value) -> Value {
    let mut value = json!({"key":key,"name":key,"prompt":format!("Implement {key} and verify its behavior."),"files":[format!("src/{key}")]});
    value.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    value
}
#[test]
fn auto_team_lead_chooses_worker_models() {
    let (state, sid) = auto_state();
    let result = call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[
        auto_assignment("direct", json!({"provider":"codex","model":"gpt-6.1-sol"})),
        auto_assignment("roster", json!({"profileId":"kimi"})),
        auto_assignment("own", json!({})),
    ]})).unwrap();
    assert_eq!(result["workerIds"].as_array().unwrap().len(), 3);
    let store = state.store.lock().unwrap();
    let team = load(&store, &sid).unwrap().unwrap();
    let lead = store.session(&sid).unwrap();
    let picked: Vec<_> = team.workers.iter().map(|w| (w.provider.as_str(), w.model.as_str(), w.profile_id.as_str())).collect();
    assert_eq!(picked, vec![("codex", "gpt-6.1-sol", "auto"), ("kimi", "", "kimi"), (lead.provider.as_str(), lead.model.as_str(), "auto")]);
    // Each worker task runs on the model the lead picked for it.
    let direct = store.session(&team.workers[0].session_id).unwrap();
    assert_eq!((direct.provider.as_str(), direct.model.as_str()), ("codex", "gpt-6.1-sol"));
    assert_eq!(team.workers[0].name, "direct");
}
#[test]
fn auto_team_rejects_ambiguous_or_unknown_models() {
    let (state, sid) = auto_state();
    let bad = |extra: Value| call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[auto_assignment("x", extra)]})).unwrap_err();
    assert!(bad(json!({"profileId":"kimi","model":"m"})).contains("not both"));
    assert!(bad(json!({"model":"m"})).contains("together"));
    assert!(bad(json!({"provider":"other","model":"m"})).contains("Unknown harness"));
    assert!(bad(json!({"profileId":"missing"})).contains("Unknown worker profile"));
}
#[test]
fn configured_team_still_requires_a_profile() {
    let (state, sid) = state_fixture();
    let error = call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[auto_assignment("x", json!({"provider":"codex","model":"m"}))]})).unwrap_err();
    assert!(error.contains("profileId"));
}
#[test]
fn auto_config_allows_an_empty_or_large_roster_but_not_unbounded() {
    let mut config = config();
    config.auto = true;
    config.workers.clear();
    assert!(config.validate().is_ok());
    config.workers = (0..AUTO_ROSTER_MAX).map(|i| WorkerProfile { id: format!("m{i}"), name: format!("Model {i}"), provider: "codex".into(), model: format!("m{i}"), reasoning_effort: None }).collect();
    assert!(config.validate().is_ok());
    config.workers.push(WorkerProfile { id: "extra".into(), name: "Extra".into(), provider: "codex".into(), model: "x".into(), reasoning_effort: None });
    assert!(config.validate().is_err());
    let mut manual = super::tests::config();
    manual.workers.clear();
    assert!(manual.validate().is_err());
}
#[test]
fn auto_prompt_names_the_lead_and_defers_to_the_user() {
    let (state, sid) = auto_state();
    let store = state.store.lock().unwrap();
    let text = prompt(&store, &sid, "Use Sonnet workers.", Some(&Admission { parent: sid.clone(), generation: 1 })).unwrap();
    assert!(text.contains("You lead this team with your own model"));
    assert!(text.contains("if the user's request names models"));
    assert!(text.ends_with("Use Sonnet workers."));
}

/// A named agent team from Settings: its lead may only orchestrate the models it lists.
fn roster_only_state(lead_in_roster: bool) -> (AppState, String) {
    let (state, sid) = auto_state();
    {
        let store = state.store.lock().unwrap();
        let mut team = load(&store, &sid).unwrap().unwrap();
        team.config.roster_only = true;
        team.config.name = Some("Reviewers".into());
        if lead_in_roster {
            let lead = store.session(&sid).unwrap();
            team.config.workers.push(WorkerProfile { id: "lead".into(), name: "Lead".into(), provider: lead.provider, model: lead.model, reasoning_effort: Some("high".into()) });
        }
        save(&store, &mut team).unwrap();
    }
    (state, sid)
}
#[test]
fn roster_only_team_dispatches_only_its_models() {
    let (state, sid) = roster_only_state(false);
    let dispatch = |key: &str, extra: Value| call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[auto_assignment(key, extra)]}));
    assert!(dispatch("outside", json!({"provider":"claude","model":"opus"})).unwrap_err().contains("only orchestrate"));
    assert!(dispatch("unlisted-lead", json!({})).unwrap_err().contains("only orchestrate"));
    dispatch("direct", json!({"provider":"codex","model":"default"})).unwrap();
    dispatch("profile", json!({"profileId":"kimi"})).unwrap();
    let store = state.store.lock().unwrap();
    let team = load(&store, &sid).unwrap().unwrap();
    let picked: Vec<_> = team.workers.iter().map(|w| (w.provider.as_str(), w.profile_id.as_str())).collect();
    // A direct pick of a listed model is recorded as that roster entry.
    assert_eq!(picked, vec![("codex", "fast"), ("kimi", "kimi")]);
}
#[test]
fn roster_only_team_lets_a_listed_lead_staff_itself() {
    let (state, sid) = roster_only_state(true);
    call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[auto_assignment("own", json!({}))]})).unwrap();
    let store = state.store.lock().unwrap();
    let worker = &load(&store, &sid).unwrap().unwrap().workers[0];
    assert_eq!((worker.profile_id.as_str(), worker.reasoning_effort.as_deref()), ("lead", Some("high")));
}
#[test]
fn roster_only_config_needs_an_auto_team_with_models() {
    let mut config = config();
    config.roster_only = true;
    assert!(config.validate().is_err(), "configured teams already name fixed profiles");
    config.auto = true;
    assert!(config.validate().is_ok());
    config.workers.clear();
    assert!(config.validate().is_err());
    let mut named = super::tests::config();
    named.name = Some("x".repeat(81));
    assert!(named.validate().is_err());
    named.name = Some("Reviewers".into());
    assert!(named.validate().is_ok());
}
#[test]
fn roster_only_prompt_lists_only_the_team() {
    let (state, sid) = roster_only_state(false);
    let store = state.store.lock().unwrap();
    let text = prompt(&store, &sid, "Ship it.", Some(&Admission { parent: sid.clone(), generation: 1 })).unwrap();
    assert!(text.contains("the Reviewers team"));
    assert!(text.contains("Use only these models"));
    assert!(!text.contains("if the user's request names models"));
}
#[test]
fn architect_that_stops_waiting_on_a_background_job_is_asked_once_to_continue() {
    let (state, parent) = state_fixture();
    let (run, _) = running_worker(&state, &parent, "parser", "src");
    finish(&state, &run, "Parser checked successfully", true);
    let review = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    let (review, _) = claim(&state, review);
    // The architect starts a background job and ends its turn expecting a wakeup.
    finish(&state, &review, "Started the driver in the background. Waiting on the monitor.", true);
    let nudge = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert!(nudge.prompt.starts_with("You ended your turn without dispatching workers"));
    assert!(nudge.prompt.contains("do not report back after your turn ends"));
    let team = load(&state.store.lock().unwrap(), &parent).unwrap().unwrap();
    assert_eq!(team.status, "running");
    assert_eq!(team.error, None);
    assert_eq!(
        team.pending_turn.as_ref().and_then(|turn| turn.kind),
        Some(internal_turns::CoordinationKind::Continue)
    );
    let (nudge, _) = claim(&state, nudge);
    // A second stall with nothing new dispatched pauses instead of looping.
    finish(&state, &nudge, "Still waiting.", true);
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    let team = load(&state.store.lock().unwrap(), &parent).unwrap().unwrap();
    assert_eq!(team.status, "paused");
    assert!(team.error.unwrap().contains("ended without dispatching"));
}
#[test]
fn every_registered_team_tool_runs_without_an_approval_prompt() {
    let names = catalog().into_iter().chain(coordination::tools(true)).chain(coordination::tools(false))
        .map(|tool| tool["name"].as_str().unwrap().to_owned()).collect::<HashSet<_>>();
    for name in &names {
        assert!(team_tool(name), "{name} is not auto-allowed");
    }
    assert!(!team_tool("pandacode_run_project"));
}
#[test]
fn a_question_answered_without_delegating_leaves_the_team_ready() {
    let (state, sid) = state_fixture();
    let action = Launch { sid: sid.clone(), prompt: "What do you think the issue is?".into(), images: None, admission: Admission { parent: sid.clone(), generation: 1 } };
    let (turn, _rx) = claim(&state, action);
    finish(&state, &turn, "The parser drops the last token.", true);
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    assert_eq!(team.status, "ready");
    assert_eq!(team.error, None);
}
#[test]
fn a_failed_follow_up_never_reports_the_previous_attempts_success() {
    let (state, sid) = state_fixture();
    let action = Launch { sid: sid.clone(), prompt: "Objective".into(), images: None, admission: Admission { parent: sid.clone(), generation: 1 } };
    let (parent, _rx) = claim(&state, action);
    let dispatched = call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[assignment("engine","fast","src")]})).unwrap();
    let id = dispatched["workerIds"][0].as_str().unwrap().to_owned();
    let (worker, _w) = claim(&state, next_action_state(&state, &mut |_| {}).unwrap().unwrap());
    finish(&state, &worker, "Attempt one: all engine checks passed.", true);
    finish(&state, &parent, "waiting", true);
    let (review, _r) = claim(&state, next_action_state(&state, &mut |_| {}).unwrap().unwrap());
    call(&state, &sid, "pandacode_followup_worker", json!({"workerId":id,"key":"fix","prompt":"Fix the edge case."})).unwrap();
    finish(&state, &review, "follow", true);
    let (retried, _x) = claim(&state, next_action_state(&state, &mut |_| {}).unwrap().unwrap());
    {
        let mut agents = state.agents.lock().unwrap();
        let store = state.store.lock().unwrap();
        store.finish_task_turn(&retried.config.session_id, false, false).unwrap();
        agents.remove(&retried.config.session_id);
        drop(store);
        completed_with_agents(&state, &agents, &retried.config.session_id, retried.team.as_ref(), false, false, &mut |_| {});
    }
    let review = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert!(!review.prompt.contains("Attempt one"));
}
#[test]
fn a_new_objective_runs_a_reused_key_again_and_counts_its_own_assignments() {
    let (state, sid) = state_fixture();
    let first = Launch { sid: sid.clone(), prompt: "Objective one".into(), images: None, admission: Admission { parent: sid.clone(), generation: 1 } };
    let (parent, _rx) = claim(&state, first);
    call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[assignment("tests","fast","tests")]})).unwrap();
    let (worker, _w) = claim(&state, next_action_state(&state, &mut |_| {}).unwrap().unwrap());
    finish(&state, &worker, "Tests pass.", true);
    // Within the objective, repeating the key says the work will not run again.
    let again = call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[assignment("tests","fast","tests")]})).unwrap();
    assert_eq!(again["queued"], false);
    assert_eq!(again["duplicates"][0]["status"], "completed");
    {
        let store = state.store.lock().unwrap();
        let mut team = load(&store, &sid).unwrap().unwrap();
        team.status = "completed".into();
        team.config.max_assignments = 1;
        save(&store, &mut team).unwrap();
        store.finish_task_turn(&sid, true, false).unwrap();
    }
    state.agents.lock().unwrap().remove(&sid);
    drop(parent);
    let generation = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap().generation;
    start_state(&state, &sid, "Objective two", None, None).unwrap();
    let generation = generation + 1;
    let second = Launch { sid: sid.clone(), prompt: "Objective two".into(), images: None, admission: Admission { parent: sid.clone(), generation } };
    let (_turn, _rx) = claim(&state, second);
    let fresh = call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[assignment("tests","fast","tests")]})).unwrap();
    assert_eq!(fresh["queued"], true);
    assert_eq!(load(&state.store.lock().unwrap(), &sid).unwrap().unwrap().workers.len(), 2);
}
#[test]
fn pausing_and_resuming_does_not_use_up_worker_attempts() {
    let (state, sid) = state_fixture();
    let action = Launch { sid: sid.clone(), prompt: "Objective".into(), images: None, admission: Admission { parent: sid.clone(), generation: 1 } };
    let (parent, _rx) = claim(&state, action);
    call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[assignment("engine","fast","src")]})).unwrap();
    finish(&state, &parent, "waiting", true);
    for _ in 0..4 {
        control_state(&state, &sid, "pause").unwrap();
        control_state(&state, &sid, "resume").unwrap();
    }
    let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    assert_eq!(team.workers[0].status, "queued");
    assert_eq!(team.workers[0].attempt, 1);
}
#[test]
fn one_team_with_a_missing_folder_does_not_stop_the_others() {
    let (state, healthy) = state_fixture();
    let gone = std::env::temp_dir().join(format!("pandacode-gone-{}", storage::id()));
    std::fs::create_dir_all(&gone).unwrap();
    let broken = {
        let store = state.store.lock().unwrap();
        let project = store.add_project(&gone).unwrap();
        let parent = store.create_session(&project.id, "claude", "default", "ask").unwrap();
        let mut team = new_team(parent.id.clone(), config());
        team.status = "waiting".into();
        team.generation = 1;
        team.updated_at = 0;
        let content = serde_json::to_string(&team).unwrap();
        store.db.execute("INSERT INTO architect_teams(session_id,content) VALUES(?1,?2)", params![parent.id, content]).unwrap();
        parent.id
    };
    std::fs::remove_dir_all(&gone).unwrap();
    let action = Launch { sid: healthy.clone(), prompt: "Objective".into(), images: None, admission: Admission { parent: healthy.clone(), generation: 1 } };
    let (_parent, _rx) = claim(&state, action);
    call(&state, &healthy, "pandacode_dispatch_workers", json!({"assignments":[assignment("engine","fast","src")]})).unwrap();
    let next = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert_ne!(next.sid, broken);
    let team = load(&state.store.lock().unwrap(), &broken).unwrap().unwrap();
    assert_eq!(team.status, "failed");
    assert!(team.error.is_some());
}
#[test]
fn new_results_keep_their_place_ahead_of_old_checkpoints() {
    let (_store, mut team) = fixture();
    for i in 0..20 {
        let mut w = worker(&format!("old{i}"), "completed", &["a"], &[]);
        w.result_revision = 1;
        w.checkpoint = Some(coordination::WorkerCheckpoint { summary: "s".repeat(2000), next_steps: "n".repeat(2000), blocker: None, updated_at: 1 });
        team.workers.push(w);
    }
    let mut fresh = worker("fresh", "completed", &["b"], &[]);
    fresh.summary = format!("NEW RESULT: {} END", "x".repeat(1500));
    fresh.result_revision = 10;
    team.workers.push(fresh);
    team.reviewed_revision = 5;
    let review = bounded(&serde_json::to_string(&review_summaries(&team)).unwrap(), 40 * 1024);
    assert!(review.contains(" END"));
    assert!(!review.contains(&"s".repeat(100)));
}
#[test]
fn a_worker_that_cannot_start_fails_alone() {
    let (state, sid) = state_fixture();
    let action = Launch { sid: sid.clone(), prompt: "Objective".into(), images: None, admission: Admission { parent: sid.clone(), generation: 1 } };
    let (parent, _rx) = claim(&state, action);
    call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[assignment("one","fast","src/a"),assignment("two","fast","src/b")]})).unwrap();
    let first = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    fail_launch_state(&state, &first.admission, &first.sid, "An agent update is being installed", &mut |_| {});
    let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    assert!(active(&team.status));
    let failed = team.workers.iter().find(|w| w.session_id == first.sid).unwrap();
    assert_eq!(failed.status, "failed");
    assert!(team.workers.iter().any(|w| w.session_id != first.sid && w.status != "failed"));
    drop(parent);
}
fn objective_turn(state: &AppState, sid: &str) -> crate::PreparedRun {
    let action = Launch { sid: sid.into(), prompt: "Objective".into(), images: None, admission: Admission { parent: sid.into(), generation: 1 } };
    claim(state, action).0
}
fn settle(state: &AppState, run: &crate::PreparedRun, successful: bool, stopped: bool) {
    let sid = &run.config.session_id;
    let mut agents = state.agents.lock().unwrap();
    state.store.lock().unwrap().finish_task_turn(sid, successful, stopped).unwrap();
    agents.remove(sid);
    completed_with_agents(state, &agents, sid, run.team.as_ref(), successful, stopped, &mut |_| {});
}
const SILENT: i64 = (watchdog::SILENCE_MINUTES + 1) * 60 * 1000;
#[test]
fn a_failed_worker_is_handed_to_the_architect_while_others_still_run() {
    let (state, sid) = state_fixture();
    let parent = objective_turn(&state, &sid);
    call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[assignment("one","fast","src/a"),assignment("two","fast","src/b")]})).unwrap();
    let (one, _a) = claim(&state, next_action_state(&state, &mut |_| {}).unwrap().unwrap());
    let (two, _b) = claim(&state, next_action_state(&state, &mut |_| {}).unwrap().unwrap());
    finish(&state, &parent, "waiting", true);
    settle(&state, &one, false, false);
    let handle = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert!(handle.prompt.starts_with(FAILED_PREFIX));
    let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    assert_eq!(team.pending_turn.as_ref().and_then(|t| t.kind), Some(internal_turns::CoordinationKind::WorkerFailed));
    assert!(team.workers.iter().any(|w| w.session_id == two.config.session_id && active(&w.status)));
    let (handle, _h) = claim(&state, handle);
    finish(&state, &handle, "Left it for the final review.", true);
    // Nothing more until the remaining worker settles, then the normal review runs.
    assert!(next_action_state(&state, &mut |_| {}).unwrap().is_none());
    finish(&state, &two, "Two is done.", true);
    let review = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert!(review.prompt.starts_with("Review the settled workers"));
}
#[test]
fn a_silent_worker_is_stopped_and_settles_as_failed() {
    let (state, sid) = state_fixture();
    let (run, mut rx) = {
        call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[assignment("slow","fast","src")]})).unwrap();
        claim(&state, next_action_state(&state, &mut |_| {}).unwrap().unwrap())
    };
    let now = storage::now();
    assert_eq!(watchdog::stop_silent(&state, now, &mut |_| {}).unwrap(), 0);
    // A worker waiting on the user's approval is not stuck.
    state.agents.lock().unwrap()[&run.config.session_id].approvals.lock().unwrap().insert("ask".into());
    assert_eq!(watchdog::stop_silent(&state, now + SILENT, &mut |_| {}).unwrap(), 0);
    state.agents.lock().unwrap()[&run.config.session_id].approvals.lock().unwrap().clear();
    assert_eq!(watchdog::stop_silent(&state, now + SILENT, &mut |_| {}).unwrap(), 1);
    assert!(matches!(rx.try_recv(), Ok(RunInput::Stop)));
    settle(&state, &run, false, true);
    let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    assert_eq!(team.workers[0].status, "failed");
    assert!(team.workers[0].error.as_deref().unwrap().starts_with("No output for"));
    assert!(!team.workers[0].timed_out);
}
#[test]
fn a_silent_architect_turn_is_stopped_without_stopping_its_workers() {
    let (state, sid) = state_fixture();
    let parent = objective_turn(&state, &sid);
    call(&state, &sid, "pandacode_dispatch_workers", json!({"assignments":[assignment("engine","fast","src")]})).unwrap();
    let (worker, _w) = claim(&state, next_action_state(&state, &mut |_| {}).unwrap().unwrap());
    let now = storage::now();
    state.agents.lock().unwrap()[&worker.config.session_id].last_event.store(now + SILENT, Ordering::SeqCst);
    assert_eq!(watchdog::stop_silent(&state, now + SILENT, &mut |_| {}).unwrap(), 1);
    settle(&state, &parent, false, true);
    let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    assert_eq!(team.status, "waiting");
    assert_eq!(team.error, Some(watchdog::architect_note()));
    assert_eq!(team.workers[0].status, "running");
    finish(&state, &worker, "Engine done.", true);
    let (review, _r) = claim(&state, next_action_state(&state, &mut |_| {}).unwrap().unwrap());
    let team = load(&state.store.lock().unwrap(), &sid).unwrap().unwrap();
    assert_eq!(team.error, None);
    assert!(!team.timed_out);
    drop(review);
}
#[test]
fn a_silent_review_is_asked_once_to_continue_with_the_reason() {
    let (state, sid) = state_fixture();
    let (run, _) = running_worker(&state, &sid, "parser", "src");
    finish(&state, &run, "Parser checked", true);
    let (review, _r) = claim(&state, next_action_state(&state, &mut |_| {}).unwrap().unwrap());
    assert_eq!(watchdog::stop_silent(&state, storage::now() + SILENT, &mut |_| {}).unwrap(), 1);
    settle(&state, &review, false, true);
    let nudge = next_action_state(&state, &mut |_| {}).unwrap().unwrap();
    assert!(nudge.prompt.starts_with("PandaCode stopped your last turn after"));
}
#[test]
fn a_resumed_conversation_gets_the_team_rules_once_and_only_new_messages() {
    let (state, sid) = state_fixture();
    let store = state.store.lock().unwrap();
    store.db.execute("UPDATE sessions SET resume_id='native-1' WHERE id=?1", [&sid]).unwrap();
    let admission = Admission { parent: sid.clone(), generation: 1 };
    let first = prompt(&store, &sid, "Build the parser", Some(&admission)).unwrap();
    assert!(first.contains("PandaCode architect mode. Coordinate"));
    assert!(first.ends_with("Build the parser"));
    let second = prompt(&store, &sid, "Now the renderer", Some(&admission)).unwrap();
    assert!(!second.contains("PandaCode architect mode. Coordinate"));
    assert!(!second.contains("Team coordination"));
    assert!(second.starts_with("PandaCode architect mode continues"));
    assert!(second.ends_with("Now the renderer"));
    store.db.execute("INSERT INTO architect_messages(parent_session_id,sender,recipient,request_key,content,created_at) VALUES(?1,'w1','architect','k','Renderer API is ready',1)", [&sid]).unwrap();
    let third = prompt(&store, &sid, "Review", Some(&admission)).unwrap();
    assert!(third.contains("Renderer API is ready"));
    let fourth = prompt(&store, &sid, "Review again", Some(&admission)).unwrap();
    assert!(!fourth.contains("Renderer API is ready"));
    // Changed limits are new rules, so they go out in full.
    let mut team = load(&store, &sid).unwrap().unwrap();
    team.config.max_rounds += 4;
    save(&store, &mut team).unwrap();
    assert!(prompt(&store, &sid, "Go on", Some(&admission)).unwrap().contains("PandaCode architect mode. Coordinate"));
    // A conversation the harness starts afresh has never seen them.
    store.db.execute("UPDATE sessions SET resume_id=NULL WHERE id=?1", [&sid]).unwrap();
    assert!(prompt(&store, &sid, "Go on", Some(&admission)).unwrap().contains("PandaCode architect mode. Coordinate"));
}
