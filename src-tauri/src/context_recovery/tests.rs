use super::*;
use std::path::{Path, PathBuf};

struct Disk(PathBuf);
impl Disk {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pandacode-context-recovery-{}",
            crate::storage::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn database(&self) -> PathBuf {
        self.0.join("state.sqlite")
    }
}
impl Drop for Disk {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn task(store: &Store) -> Session {
    let project = store.add_project(&std::env::temp_dir()).unwrap();
    store
        .create_session(&project.id, "codex", "gpt-test", "ask")
        .unwrap()
}

fn prior_entries(prompt: &str) -> Vec<serde_json::Value> {
    let json = prompt
        .split_once("Earlier conversation (JSON):\n")
        .unwrap()
        .1;
    let json = json.rsplit_once("\n\nCurrent request:\n").unwrap().0;
    assert!(
        json.len() <= HISTORY_BYTES,
        "Historical context exceeded the wire budget: {}",
        json.len()
    );
    serde_json::from_str(json).unwrap()
}

#[test]
fn a_new_task_and_an_ordinary_resumed_task_keep_the_original_prompt() {
    let store = Store::open(Path::new(":memory:")).unwrap();
    let task = task(&store);
    assert!(!pending(&store, &task).unwrap());
    assert_eq!(
        prepare(&store, &task, "First request").unwrap(),
        ("First request".into(), None)
    );
    store
        .message(&task.id, "user", "Keep this prior request")
        .unwrap();
    store
        .message(&task.id, "assistant", "Keep the native answer")
        .unwrap();
    store.resume(&task.id, "existing-native-context").unwrap();
    let resumed = store.session(&task.id).unwrap();
    assert!(!pending(&store, &resumed).unwrap());
    assert_eq!(
        prepare(&store, &resumed, "Continue").unwrap(),
        ("Continue".into(), None)
    );
}

#[test]
fn missing_native_context_recovers_the_exact_previous_code_without_rewriting_the_transcript() {
    let store = Store::open(Path::new(":memory:")).unwrap();
    let task = task(&store);
    let code = "```cpp\n#include <iostream>\nint main() { std::cout << \"Hello, world!\" << std::endl; return 0; }\n```";
    store
        .message(
            &task.id,
            "user",
            "Write C++ in your response, without making a file",
        )
        .unwrap();
    store.message(&task.id, "assistant", code).unwrap();
    store
        .message(&task.id, "tool", "Do not replay this tool output")
        .unwrap();
    store
        .message(&task.id, "system", "Do not replay this system notice")
        .unwrap();
    store
        .message(&task.id, "changes", "Do not replay this change summary")
        .unwrap();
    let before = store.messages(&task.id, None).unwrap().len();
    let (prompt, through) = prepare(&store, &task, "Put this in a file please").unwrap();
    assert!(through.is_some());
    let entries = prior_entries(&prompt);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["role"], "user");
    assert_eq!(entries[1], json!({"role":"assistant", "content":code}));
    assert!(prompt.ends_with("Current request:\nPut this in a file please"));
    assert_eq!(prompt.matches("Put this in a file please").count(), 1);
    assert!(!prompt.contains("Do not replay"));
    assert_eq!(store.messages(&task.id, None).unwrap().len(), before);
}

#[test]
fn receiving_a_new_session_id_does_not_acknowledge_a_failed_or_stopped_handoff() {
    let store = Store::open(Path::new(":memory:")).unwrap();
    let task = task(&store);
    store
        .message(&task.id, "assistant", "Important previous response")
        .unwrap();
    queue(&store, &task.id).unwrap();
    let (_, first_boundary) = prepare(&store, &task, "Continue").unwrap();
    // Providers return a session ID before accepting the prompt; startup can fail next.
    store.resume(&task.id, "new-context-before-prompt").unwrap();
    store.message(&task.id, "user", "Continue").unwrap();
    let restarted = store.session(&task.id).unwrap();
    assert!(pending(&store, &restarted).unwrap());
    let (retry_prompt, boundary) = prepare(&store, &restarted, "Try again").unwrap();
    assert!(boundary.unwrap() > first_boundary.unwrap());
    let entries = prior_entries(&retry_prompt);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1], json!({"role":"user", "content":"Continue"}));
    assert!(retry_prompt.contains("Important previous response"));
    assert_eq!(retry_prompt.matches("Try again").count(), 1);
    acknowledge(&store, &task.id, first_boundary.unwrap()).unwrap();
    assert!(pending(&store, &restarted).unwrap());
    acknowledge(&store, &task.id, boundary.unwrap()).unwrap();
    assert!(!pending(&store, &restarted).unwrap());
    assert_eq!(
        prepare(&store, &restarted, "Another follow-up").unwrap(),
        ("Another follow-up".into(), None)
    );
}

#[test]
fn acknowledgment_cannot_erase_a_newer_queued_handoff_or_another_task() {
    let store = Store::open(Path::new(":memory:")).unwrap();
    let task = task(&store);
    let other = self::task(&store);
    store
        .message(&task.id, "assistant", "Earlier context")
        .unwrap();
    store
        .message(&other.id, "assistant", "Other task context")
        .unwrap();
    queue(&store, &task.id).unwrap();
    queue(&store, &other.id).unwrap();
    let (_, boundary) = prepare(&store, &task, "Continue").unwrap();
    store.resume(&task.id, "native-context").unwrap();
    store
        .message(&task.id, "assistant", "Later context")
        .unwrap();
    queue(&store, &task.id).unwrap();
    let task = store.session(&task.id).unwrap();
    acknowledge(&store, &task.id, boundary.unwrap()).unwrap();
    assert!(pending(&store, &task).unwrap());
    assert!(pending(&store, &other).unwrap());
    let (prompt, newer) = prepare(&store, &task, "Continue again").unwrap();
    assert!(newer.unwrap() > boundary.unwrap());
    assert_eq!(prior_entries(&prompt).len(), 2);
    acknowledge(&store, &task.id, newer.unwrap()).unwrap();
    assert!(!pending(&store, &task).unwrap());
    assert!(pending(&store, &other).unwrap());
}

#[test]
fn large_unicode_and_escaped_history_stays_valid_bounded_and_honest() {
    let store = Store::open(Path::new(":memory:")).unwrap();
    let task = task(&store);
    for index in 0..300 {
        store
            .message(&task.id, "user", &format!("Older message {index}"))
            .unwrap();
    }
    let huge = "竹🌱\"\\\n\t\u{0001}".repeat(30_000);
    store.message(&task.id, "assistant", &huge).unwrap();
    store
        .message(&task.id, "user", "Newest meaningful context")
        .unwrap();
    let (prompt, _) = prepare(&store, &task, "Keep going 🌱").unwrap();
    assert!(prompt.contains("bounded excerpt"));
    assert!(prompt.contains("do not assume omitted details"));
    let entries = prior_entries(&prompt);
    assert!(entries.len() <= HISTORY_MESSAGES);
    assert_eq!(
        entries.last().unwrap()["content"],
        "Newest meaningful context"
    );
    assert!(entries.iter().any(|entry| entry["content"]
        .as_str()
        .unwrap()
        .contains("Message shortened")));
    assert!(prompt.ends_with("Current request:\nKeep going 🌱"));
}

#[test]
fn history_row_limit_keeps_recent_messages_in_chronological_order() {
    let store = Store::open(Path::new(":memory:")).unwrap();
    let task = task(&store);
    for index in 0..250 {
        store
            .message(&task.id, "user", &format!("message-{index}"))
            .unwrap();
    }
    let (prompt, _) = prepare(&store, &task, "Continue").unwrap();
    let entries = prior_entries(&prompt);
    assert_eq!(entries.len(), HISTORY_MESSAGES);
    assert_eq!(entries.first().unwrap()["content"], "message-50");
    assert_eq!(entries.last().unwrap()["content"], "message-249");
    assert!(prompt.contains("bounded excerpt"));
}

#[test]
fn pending_recovery_survives_reopen_and_success_does_not_repeat_after_reopen() {
    let disk = Disk::new();
    let task;
    {
        let store = Store::open(&disk.database()).unwrap();
        task = self::task(&store);
        store
            .message(&task.id, "assistant", "Context before restart")
            .unwrap();
        store.resume(&task.id, "old-context").unwrap();
        queue(&store, &task.id).unwrap();
    }
    {
        let store = Store::open(&disk.database()).unwrap();
        let restarted = store.session(&task.id).unwrap();
        let (prompt, boundary) = prepare(&store, &restarted, "Continue after restart").unwrap();
        assert!(prompt.contains("Context before restart"));
        store.resume(&task.id, "new-context").unwrap();
        acknowledge(&store, &task.id, boundary.unwrap()).unwrap();
    }
    let store = Store::open(&disk.database()).unwrap();
    let restarted = store.session(&task.id).unwrap();
    assert!(!pending(&store, &restarted).unwrap());
    assert_eq!(
        prepare(&store, &restarted, "Continue again").unwrap(),
        ("Continue again".into(), None)
    );
}

#[test]
fn migration_repairs_legacy_architect_resets_once_even_after_a_new_session_was_saved() {
    let disk = Disk::new();
    let (affected, unaffected);
    {
        let store = Store::open(&disk.database()).unwrap();
        affected = task(&store);
        unaffected = task(&store);
        store
            .message(&affected.id, "assistant", "Original C++ code")
            .unwrap();
        store
            .message(&affected.id, "system", LEGACY_NOTICE)
            .unwrap();
        store
            .message(&affected.id, "user", "Put this in a file")
            .unwrap();
        store
            .message(&affected.id, "assistant", "What does this refer to?")
            .unwrap();
        store
            .resume(&affected.id, "context-created-after-loss")
            .unwrap();
        store
            .message(&unaffected.id, "user", LEGACY_NOTICE)
            .unwrap();
        store
            .message(
                &unaffected.id,
                "system",
                &format!("{LEGACY_NOTICE} Different notice"),
            )
            .unwrap();
        store.resume(&unaffected.id, "intact-context").unwrap();
        store
            .db
            .execute_batch("DROP TABLE context_recovery; PRAGMA user_version=13;")
            .unwrap();
    }
    {
        let store = Store::open(&disk.database()).unwrap();
        let affected = store.session(&affected.id).unwrap();
        let (prompt, boundary) = prepare(&store, &affected, "Continue").unwrap();
        let entries = prior_entries(&prompt);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0]["content"], "Original C++ code");
        assert_eq!(entries[2]["content"], "What does this refer to?");
        assert_eq!(
            affected.resume_id.as_deref(),
            Some("context-created-after-loss")
        );
        assert!(!pending(&store, &store.session(&unaffected.id).unwrap()).unwrap());
        acknowledge(&store, &affected.id, boundary.unwrap()).unwrap();
    }
    let store = Store::open(&disk.database()).unwrap();
    assert!(!pending(&store, &store.session(&affected.id).unwrap()).unwrap());
    assert_eq!(store.messages(&affected.id, None).unwrap().len(), 4);
}
