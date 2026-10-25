use crate::storage::Store;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VibeSnapshot {
    pub turn_index: usize,
    pub session_id: String,
    pub commit_sha: Option<String>,
    pub tree_sha: Option<String>,
    pub timestamp: i64,
    pub summary: String,
    pub files_copied: Vec<String>,
    pub files_changed: Vec<String>,
    pub additions: usize,
    pub deletions: usize,
    pub message_id: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VibeSnapshotPreview {
    pub snapshot: VibeSnapshot,
    pub preview_active: bool,
}

fn get_snapshots_dir(app: &AppHandle, session_id: &str) -> Result<PathBuf, String> {
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let dir = cache.join("vibe_snapshots").join(session_id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn session_root(store: &Arc<Mutex<Store>>, session_id: &str) -> Result<PathBuf, String> {
    let store = store.lock().map_err(|e| e.to_string())?;
    let task = store.session(session_id)?;
    if let Some(worktree) = task.worktree_path {
        return Ok(PathBuf::from(worktree));
    }
    store.workspace_root(&task.project_id, Some(session_id))
}

fn is_git_repo(root: &Path) -> bool {
    std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn capture_git_working_tree(
    root: &Path,
    temp_index: &Path,
    session_id: &str,
    turn_index: usize,
) -> (Option<String>, Option<String>, Vec<String>, usize, usize) {
    if !is_git_repo(root) {
        return (None, None, Vec::new(), 0, 0);
    }

    // 1. Copy the current index if it exists so we preserve staged/tracked state base,
    // or start fresh with git add -A on temp_index
    let _ = std::fs::remove_file(temp_index);

    // git add -A using GIT_INDEX_FILE
    let add_res = std::process::Command::new("git")
        .args(["add", "-A"])
        .env("GIT_INDEX_FILE", temp_index)
        .current_dir(root)
        .output();

    if add_res.is_err() || !add_res.as_ref().unwrap().status.success() {
        let _ = std::fs::remove_file(temp_index);
        return (None, None, Vec::new(), 0, 0);
    }

    // git write-tree using GIT_INDEX_FILE
    let write_res = std::process::Command::new("git")
        .args(["write-tree"])
        .env("GIT_INDEX_FILE", temp_index)
        .current_dir(root)
        .output();

    let tree_sha = if let Ok(out) = write_res {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                Some(s)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Clean up temporary index
    let _ = std::fs::remove_file(temp_index);

    let Some(tree) = tree_sha else {
        return (None, None, Vec::new(), 0, 0);
    };

    // Compute diff against HEAD
    let mut files_changed = Vec::new();
    let mut additions = 0;
    let mut deletions = 0;

    if let Ok(diff_out) = std::process::Command::new("git")
        .args(["diff", "--numstat", "HEAD", &tree])
        .current_dir(root)
        .output()
    {
        if diff_out.status.success() {
            let text = String::from_utf8_lossy(&diff_out.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let a = parts[0].parse::<usize>().unwrap_or(0);
                    let d = parts[1].parse::<usize>().unwrap_or(0);
                    let f = parts[2..].join(" ");
                    additions += a;
                    deletions += d;
                    files_changed.push(f);
                }
            }
        }
    }

    // git commit-tree
    let mut commit_cmd = std::process::Command::new("git");
    commit_cmd
        .arg("commit-tree")
        .arg(&tree)
        .args(["-m", &format!("vibe-snapshot: turn {turn_index}")])
        .current_dir(root);

    if let Ok(head_out) = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
    {
        if head_out.status.success() {
            let head_sha = String::from_utf8_lossy(&head_out.stdout).trim().to_string();
            if !head_sha.is_empty() {
                commit_cmd.args(["-p", &head_sha]);
            }
        }
    }

    let commit_sha = if let Ok(out) = commit_cmd.output() {
        if out.status.success() {
            let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !sha.is_empty() {
                let ref_name = format!("refs/swarm/snapshots/{session_id}/turn-{turn_index}");
                let _ = std::process::Command::new("git")
                    .args(["update-ref", &ref_name, &sha])
                    .current_dir(root)
                    .output();
                Some(sha)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    (commit_sha, Some(tree), files_changed, additions, deletions)
}

#[tauri::command]
pub(crate) async fn vibe_snapshot_turn(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    turn_index: usize,
    summary: String,
) -> Result<VibeSnapshot, String> {
    let root = session_root(&state.store, &session_id)?;
    let base_dir = get_snapshots_dir(&app, &session_id)?;
    let turn_dir = base_dir.join(format!("turn_{turn_index}"));
    std::fs::create_dir_all(&turn_dir).map_err(|e| e.to_string())?;

    let temp_index = base_dir.join(format!(".tmp_index_{turn_index}"));
    let (commit_sha, tree_sha, files_changed, additions, deletions) =
        capture_git_working_tree(&root, &temp_index, &session_id, turn_index);

    let mut files_copied = Vec::new();

    // Look for state/env files in root: .sqlite, .sqlite3, .db, .env, .env.*
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    let is_db = name.ends_with(".sqlite")
                        || name.ends_with(".sqlite3")
                        || name.ends_with(".db");
                    let is_env = name == ".env" || name.starts_with(".env.");

                    if is_db || is_env {
                        let target = turn_dir.join(name);
                        if std::fs::copy(&path, &target).is_ok() {
                            files_copied.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    let message_id = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store.get_latest_message_id(&session_id)
    };

    let snapshot = VibeSnapshot {
        turn_index,
        session_id: session_id.clone(),
        commit_sha,
        tree_sha,
        timestamp: chrono::Utc::now().timestamp(),
        summary: if summary.is_empty() {
            format!("Turn {turn_index}")
        } else {
            summary
        },
        files_copied,
        files_changed,
        additions,
        deletions,
        message_id,
    };

    let meta_file = turn_dir.join("snapshot.json");
    let json = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
    std::fs::write(&meta_file, json).map_err(|e| e.to_string())?;

    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn vibe_get_turn_snapshots(
    app: AppHandle,
    _state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<VibeSnapshot>, String> {
    let base_dir = get_snapshots_dir(&app, &session_id)?;
    let mut snapshots = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&base_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let meta_file = path.join("snapshot.json");
                if meta_file.exists() {
                    if let Ok(data) = std::fs::read_to_string(&meta_file) {
                        if let Ok(snap) = serde_json::from_str::<VibeSnapshot>(&data) {
                            snapshots.push(snap);
                        }
                    }
                }
            }
        }
    }

    snapshots.sort_by_key(|s| s.turn_index);
    Ok(snapshots)
}

#[tauri::command]
pub(crate) async fn vibe_preview_turn_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    turn_index: usize,
) -> Result<VibeSnapshotPreview, String> {
    let root = session_root(&state.store, &session_id)?;
    let base_dir = get_snapshots_dir(&app, &session_id)?;
    let turn_dir = base_dir.join(format!("turn_{turn_index}"));
    let meta_file = turn_dir.join("snapshot.json");

    if !meta_file.exists() {
        return Err(format!("Snapshot for turn {turn_index} not found"));
    }

    let data = std::fs::read_to_string(&meta_file).map_err(|e| e.to_string())?;
    let snapshot: VibeSnapshot = serde_json::from_str(&data).map_err(|e| e.to_string())?;

    // Check out the historical tree into the working tree
    if is_git_repo(&root) {
        if let Some(tree) = &snapshot.tree_sha {
            let _ = std::process::Command::new("git")
                .args(["read-tree", tree])
                .current_dir(&root)
                .output();
            let _ = std::process::Command::new("git")
                .args(["checkout-index", "-f", "-a"])
                .current_dir(&root)
                .output();
        } else if let Some(sha) = &snapshot.commit_sha {
            let _ = std::process::Command::new("git")
                .args(["checkout", "-f", sha])
                .current_dir(&root)
                .output();
        }
    }

    // Copy snapshot DB/env files to root
    for name in &snapshot.files_copied {
        let src = turn_dir.join(name);
        let dst = root.join(name);
        if src.exists() {
            let _ = std::fs::copy(&src, &dst);
        }
    }

    Ok(VibeSnapshotPreview {
        snapshot,
        preview_active: true,
    })
}

#[tauri::command]
pub(crate) async fn vibe_restore_turn_permanent(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    turn_index: usize,
) -> Result<(), String> {
    let root = session_root(&state.store, &session_id)?;
    let base_dir = get_snapshots_dir(&app, &session_id)?;
    let turn_dir = base_dir.join(format!("turn_{turn_index}"));
    let meta_file = turn_dir.join("snapshot.json");

    if !meta_file.exists() {
        return Err(format!("Snapshot for turn {turn_index} not found"));
    }

    let data = std::fs::read_to_string(&meta_file).map_err(|e| e.to_string())?;
    let snapshot: VibeSnapshot = serde_json::from_str(&data).map_err(|e| e.to_string())?;

    // 1. Physically restore exact working tree from tree_sha / commit
    if is_git_repo(&root) {
        if let Some(tree) = &snapshot.tree_sha {
            let _ = std::process::Command::new("git")
                .args(["read-tree", tree])
                .current_dir(&root)
                .output();
            let _ = std::process::Command::new("git")
                .args(["checkout-index", "-f", "-u", "-a"])
                .current_dir(&root)
                .output();
            let _ = std::process::Command::new("git")
                .args(["clean", "-fd"])
                .current_dir(&root)
                .output();
        } else if let Some(sha) = &snapshot.commit_sha {
            let _ = std::process::Command::new("git")
                .args(["reset", "--hard", sha])
                .current_dir(&root)
                .output();
        }
    }

    // 2. Restore state files
    for name in &snapshot.files_copied {
        let src = turn_dir.join(name);
        let dst = root.join(name);
        if src.exists() {
            let _ = std::fs::copy(&src, &dst);
        }
    }

    // 3. Truncate conversation history in SQLite database
    if let Some(mid) = snapshot.message_id {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let _ = store.truncate_session_messages_after(&session_id, mid);
    }

    // 4. Remove all subsequent turn snapshot folders (> turn_index)
    if let Ok(entries) = std::fs::read_dir(&base_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Some(idx_str) = name.strip_prefix("turn_") {
                        if let Ok(idx) = idx_str.parse::<usize>() {
                            if idx > turn_index {
                                let _ = std::fs::remove_dir_all(&path);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub(crate) async fn vibe_return_to_latest(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let root = session_root(&state.store, &session_id)?;
    let base_dir = get_snapshots_dir(&app, &session_id)?;

    // Find the highest turn_index
    let mut snapshots = vibe_get_turn_snapshots(app, state, session_id.clone()).await?;
    snapshots.sort_by_key(|s| s.turn_index);

    if let Some(latest) = snapshots.last() {
        let turn_dir = base_dir.join(format!("turn_{}", latest.turn_index));
        for name in &latest.files_copied {
            let src = turn_dir.join(name);
            let dst = root.join(name);
            if src.exists() {
                let _ = std::fs::copy(&src, &dst);
            }
        }

        if is_git_repo(&root) {
            if let Some(tree) = &latest.tree_sha {
                let _ = std::process::Command::new("git")
                    .args(["read-tree", tree])
                    .current_dir(&root)
                    .output();
                let _ = std::process::Command::new("git")
                    .args(["checkout-index", "-f", "-u", "-a"])
                    .current_dir(&root)
                    .output();
            } else if let Some(sha) = &latest.commit_sha {
                let _ = std::process::Command::new("git")
                    .args(["checkout", "-f", sha])
                    .current_dir(&root)
                    .output();
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vibe_snapshot_serialization() {
        let snap = VibeSnapshot {
            turn_index: 2,
            session_id: "test-session".into(),
            commit_sha: Some("1234567890abcdef".into()),
            tree_sha: Some("fedcba0987654321".into()),
            timestamp: 1600000000,
            summary: "Added login flow".into(),
            files_copied: vec![".env".into(), "app.db".into()],
            files_changed: vec!["src/login.ts".into()],
            additions: 45,
            deletions: 12,
            message_id: Some(104),
        };
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("turnIndex"));
        assert!(json.contains("filesCopied"));
        assert!(json.contains("filesChanged"));
        assert!(json.contains("additions"));
        assert!(json.contains("messageId"));
        let parsed: VibeSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.turn_index, 2);
        assert_eq!(parsed.files_copied.len(), 2);
        assert_eq!(parsed.files_changed.len(), 1);
        assert_eq!(parsed.additions, 45);
        assert_eq!(parsed.message_id, Some(104));
    }
}
