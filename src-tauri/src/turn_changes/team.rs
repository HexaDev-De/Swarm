//! Roll up immutable worker captures through the persisted team relationship.
//! The returned source IDs must be saved with the parent summary in the same transaction.
use super::*;

const SOURCES: usize = 200;

pub(super) fn merge(store: &Store, sid: &str, capture: &mut Captured) -> Result<Vec<String>> {
    // A checkout-wide legacy capture cannot become owned by mixing in scoped edits.
    if !capture.summary.attributed {
        return Ok(vec![]);
    }
    let mut query = store
        .db
        .prepare(
            "SELECT child.id,child.session_id,child.summary FROM turn_changes child
         JOIN architect_workers relation ON relation.session_id=child.session_id
         JOIN architect_teams team ON team.session_id=relation.parent_session_id
         JOIN json_each(team.content,'$.workers') worker
           ON json_extract(worker.value,'$.sessionId')=relation.session_id
          AND json_extract(worker.value,'$.id')=relation.worker_id
         WHERE relation.parent_session_id=?1
           AND json_extract(worker.value,'$.status') IN ('completed','failed','stopped')
           AND json_extract(child.summary,'$.attributed')=1
           AND NOT EXISTS (
             SELECT 1 FROM turn_changes parent,json_each(parent.summary,'$.teamSources') source
             WHERE parent.session_id=?1 AND source.value=child.id
           )
         ORDER BY child.message_id DESC LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = query
        .query(params![sid, (SOURCES + 1) as i64])
        .map_err(|e| e.to_string())?;
    let mut files: BTreeMap<_, _> = capture
        .summary
        .files
        .iter()
        .cloned()
        .map(|f| (f.path.clone(), f))
        .collect();
    let mut details: BTreeMap<_, _> = capture
        .details
        .iter()
        .cloned()
        .map(|d| (d.path.clone(), d))
        .collect();
    let mut patch_bytes: usize = details.values().map(|d| d.patch.len()).sum();
    let mut summary_bytes = 0usize;
    let mut sources = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let source: String = row.get(0).map_err(|e| e.to_string())?;
        let worker_sid: String = row.get(1).map_err(|e| e.to_string())?;
        let json: String = row.get(2).map_err(|e| e.to_string())?;
        summary_bytes = summary_bytes.saturating_add(json.len());
        if sources.len() == SOURCES || summary_bytes > TURN_BYTES {
            limited(&mut capture.summary);
            break;
        }
        let worker: TurnChanges = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        sources.push(source.clone());
        capture.summary.truncated |= worker.truncated;
        capture.summary.concurrent |= worker.concurrent;
        if capture.summary.error.is_none() {
            capture.summary.error = worker.error;
        }
        for mut file in worker.files {
            if !safe_relative(&file.path)
                || (!files.contains_key(&file.path) && files.len() >= CHANGE_FILES)
            {
                limited(&mut capture.summary);
                continue;
            }
            let mut previous = detail(store, &worker_sid, &source, &file.path)?;
            let current_bytes = details.get(&file.path).map_or(0, |d| d.patch.len());
            let separator = usize::from(current_bytes > 0 && !previous.patch.is_empty());
            let available = PATCH_BYTES
                .saturating_sub(current_bytes + separator)
                .min(TURN_BYTES.saturating_sub(patch_bytes + separator));
            if previous.patch.len() > available {
                truncate_patch(&mut previous.patch, available);
                previous.truncated = true;
                previous
                    .error
                    .get_or_insert_with(|| "Team diff preview reached its size limit.".into());
            }
            file.truncated |= previous.truncated;
            capture.summary.truncated |= previous.truncated;
            if let Some(current) = files.get_mut(&file.path) {
                // Sources are newest first. Retain the latest status while recording
                // cumulative edits; their patches remain separate immutable hunks.
                if current.status == "modified" {
                    if file.status == "added" {
                        current.status = "added".into();
                    } else if file.status == "renamed" {
                        current.status = "renamed".into();
                        current.old_path = file.old_path.clone();
                    }
                }
                current.additions = sum_counts(current.additions, file.additions);
                current.deletions = sum_counts(current.deletions, file.deletions);
                current.binary |= file.binary;
                current.truncated |= file.truncated;
            } else {
                files.insert(file.path.clone(), file.clone());
            }
            let previous_bytes = previous.patch.len();
            if let Some(current) = details.get_mut(&file.path) {
                if !previous.patch.is_empty() && !current.patch.is_empty() {
                    previous.patch.push('\n');
                }
                let added = previous.patch.len();
                previous.patch.push_str(&current.patch);
                current.patch = previous.patch;
                current.old_path = files[&file.path].old_path.clone();
                current.binary |= previous.binary;
                current.truncated |= previous.truncated;
                if current.error.is_none() {
                    current.error = previous.error;
                }
                patch_bytes = patch_bytes.saturating_add(added);
            } else {
                details.insert(file.path, previous);
                patch_bytes = patch_bytes.saturating_add(previous_bytes);
            }
        }
    }
    capture.summary.files = files.into_values().collect();
    capture.summary.additions = capture
        .summary
        .files
        .iter()
        .filter_map(|f| f.additions)
        .fold(0, usize::saturating_add);
    capture.summary.deletions = capture
        .summary
        .files
        .iter()
        .filter_map(|f| f.deletions)
        .fold(0, usize::saturating_add);
    capture.details = details.into_values().collect();
    Ok(sources)
}

fn sum_counts(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    a.zip(b).map(|(a, b)| a.saturating_add(b))
}

fn limited(summary: &mut TurnChanges) {
    summary.truncated = true;
    summary
        .error
        .get_or_insert_with(|| "Some team changes exceeded this turn's preview limit.".into());
}

fn truncate_patch(patch: &mut String, limit: usize) {
    let mut end = limit.min(patch.len());
    while !patch.is_char_boundary(end) {
        end -= 1;
    }
    patch.truncate(end);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn capture(path: &str, text: &str) -> Captured {
        Captured {
            summary: TurnChanges {
                id: storage::id(),
                files: vec![ChangedFile {
                    path: path.into(),
                    old_path: None,
                    status: "modified".into(),
                    additions: Some(1),
                    deletions: Some(1),
                    binary: false,
                    truncated: false,
                }],
                additions: 1,
                deletions: 1,
                truncated: false,
                concurrent: false,
                attributed: true,
                error: None,
            },
            details: vec![TurnChangeDiff {
                path: path.into(),
                old_path: None,
                patch: text.into(),
                binary: false,
                truncated: false,
                error: None,
            }],
        }
    }

    fn empty() -> Captured {
        let mut capture = capture("unused", "");
        capture.summary.files.clear();
        capture.summary.additions = 0;
        capture.summary.deletions = 0;
        capture.details.clear();
        capture
    }

    struct TeamFixture {
        _files: super::super::tests::Fixture,
        store: Store,
        parent: String,
        worker: String,
        unrelated: String,
    }

    impl TeamFixture {
        fn new() -> Self {
            let files = super::super::tests::Fixture::new(false);
            let store = Store::open(&files.root.join("state.sqlite")).unwrap();
            let project = store.add_project(&files.root).unwrap();
            let parent = store
                .create_session(&project.id, "codex", "default", "ask")
                .unwrap()
                .id;
            let worker = store
                .create_session(&project.id, "codex", "default", "ask")
                .unwrap()
                .id;
            let unrelated = store
                .create_session(&project.id, "codex", "default", "ask")
                .unwrap()
                .id;
            store.db.execute("INSERT INTO architect_workers(parent_session_id,worker_id,session_id) VALUES(?1,'worker',?2)", params![parent,worker]).unwrap();
            let content = json!({"workers":[{"id":"worker","sessionId":worker,"status":"completed","abandoned":false}]}).to_string();
            store
                .db
                .execute(
                    "INSERT INTO architect_teams(session_id,content) VALUES(?1,?2)",
                    params![parent, content],
                )
                .unwrap();
            Self {
                _files: files,
                store,
                parent,
                worker,
                unrelated,
            }
        }
        fn state(&self, status: &str, abandoned: bool) {
            self.store.db.execute("UPDATE architect_teams SET content=json_set(content,'$.workers[0].status',?2,'$.workers[0].abandoned',?3) WHERE session_id=?1", params![self.parent,status,abandoned]).unwrap();
        }
    }

    #[test]
    fn completed_worker_edits_join_parent_without_other_tasks_or_legacy_changes() {
        let f = TeamFixture::new();
        persist(&f.store, &f.worker, capture("worker.txt", "+worker")).unwrap();
        persist(
            &f.store,
            &f.unrelated,
            capture("unrelated.txt", "+unrelated"),
        )
        .unwrap();
        let mut legacy = capture("legacy.txt", "+legacy");
        legacy.summary.attributed = false;
        persist(&f.store, &f.worker, legacy).unwrap();
        let mut parent = capture("parent.txt", "+parent");
        let sources = merge(&f.store, &f.parent, &mut parent).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(
            parent
                .summary
                .files
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            ["parent.txt", "worker.txt"]
        );
        assert_eq!(parent.summary.additions, 2);
        assert_eq!(parent.details[1].patch, "+worker");
        assert!(parent.summary.attributed);
    }

    #[test]
    fn active_workers_wait_while_failed_and_abandoned_workers_keep_their_edits() {
        let f = TeamFixture::new();
        persist(&f.store, &f.worker, capture("worker.txt", "+worker")).unwrap();
        for status in ["queued", "running", "waiting"] {
            f.state(status, false);
            assert!(merge(&f.store, &f.parent, &mut empty()).unwrap().is_empty());
        }
        for (status, abandoned) in [("completed", false), ("failed", false), ("stopped", true)] {
            f.state(status, abandoned);
            assert_eq!(merge(&f.store, &f.parent, &mut empty()).unwrap().len(), 1);
        }
    }

    #[test]
    fn persisted_sources_are_imported_once_and_new_followup_captures_remain_eligible() {
        let f = TeamFixture::new();
        persist(&f.store, &f.worker, capture("worker.txt", "first")).unwrap();
        let parent = empty();
        let id = parent.summary.id.clone();
        persist(&f.store, &f.parent, parent).unwrap();
        assert_eq!(summary(&f.store, &f.parent, &id).unwrap().files.len(), 1);
        let source_count: i64 = f
            .store
            .db
            .query_row(
                "SELECT json_array_length(summary,'$.teamSources') FROM turn_changes WHERE id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(source_count, 1);
        assert!(merge(&f.store, &f.parent, &mut empty()).unwrap().is_empty());
        persist(&f.store, &f.worker, capture("followup.txt", "second")).unwrap();
        let mut next = empty();
        assert_eq!(merge(&f.store, &f.parent, &mut next).unwrap().len(), 1);
        assert_eq!(next.summary.files[0].path, "followup.txt");
    }

    #[test]
    fn shared_path_keeps_chronological_patches_and_one_file_row() {
        let f = TeamFixture::new();
        persist(&f.store, &f.worker, capture("shared.txt", "first")).unwrap();
        persist(&f.store, &f.worker, capture("shared.txt", "second")).unwrap();
        let mut parent = capture("shared.txt", "parent");
        merge(&f.store, &f.parent, &mut parent).unwrap();
        assert_eq!(parent.summary.files.len(), 1);
        assert_eq!(parent.summary.additions, 3);
        assert_eq!(parent.details[0].patch, "first\nsecond\nparent");
    }

    #[test]
    fn worker_rename_is_preserved_when_parent_edits_the_destination() {
        let f = TeamFixture::new();
        let mut worker = capture("renamed.txt", "rename");
        worker.summary.files[0].status = "renamed".into();
        worker.summary.files[0].old_path = Some("original.txt".into());
        worker.details[0].old_path = Some("original.txt".into());
        persist(&f.store, &f.worker, worker).unwrap();
        let mut parent = capture("renamed.txt", "parent");
        merge(&f.store, &f.parent, &mut parent).unwrap();
        assert_eq!(parent.summary.files[0].status, "renamed");
        assert_eq!(
            parent.summary.files[0].old_path.as_deref(),
            Some("original.txt")
        );
        assert_eq!(parent.details[0].old_path.as_deref(), Some("original.txt"));
    }

    #[test]
    fn rollup_bounds_files_and_patch_bytes_and_preserves_own_capture() {
        let f = TeamFixture::new();
        persist(
            &f.store,
            &f.worker,
            capture("worker.txt", &"🦀".repeat(PATCH_BYTES / 4)),
        )
        .unwrap();
        let mut parent = capture("worker.txt", "parent");
        for i in 1..CHANGE_FILES {
            let extra = capture(&format!("own-{i}.txt"), "own");
            parent.summary.files.extend(extra.summary.files);
            parent.details.extend(extra.details);
        }
        persist(&f.store, &f.worker, capture("overflow.txt", "overflow")).unwrap();
        merge(&f.store, &f.parent, &mut parent).unwrap();
        assert_eq!(parent.summary.files.len(), CHANGE_FILES);
        assert!(!parent
            .summary
            .files
            .iter()
            .any(|f| f.path == "overflow.txt"));
        let shared = parent
            .details
            .iter()
            .find(|d| d.path == "worker.txt")
            .unwrap();
        assert!(shared.patch.len() <= PATCH_BYTES);
        assert!(shared.patch.ends_with("parent"));
        assert!(parent.summary.truncated);
        assert!(parent.details.iter().map(|d| d.patch.len()).sum::<usize>() <= TURN_BYTES);
    }

    #[test]
    fn total_patch_budget_is_shared_across_worker_captures() {
        let f = TeamFixture::new();
        for i in 0..10 {
            persist(
                &f.store,
                &f.worker,
                capture(&format!("worker-{i}.txt"), &"x".repeat(PATCH_BYTES)),
            )
            .unwrap();
        }
        let mut parent = empty();
        merge(&f.store, &f.parent, &mut parent).unwrap();
        assert_eq!(parent.summary.files.len(), 10);
        assert_eq!(
            parent.details.iter().map(|d| d.patch.len()).sum::<usize>(),
            TURN_BYTES
        );
        assert!(parent.summary.truncated);
    }
}
