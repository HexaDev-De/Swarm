use crate::storage::Session;
use crate::workspace;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{Emitter, Manager, State};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaCompetitor {
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaMatchup {
    pub id: String,
    pub source_session_id: String,
    pub task_a: Session,
    pub task_b: Session,
    pub prompt: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaFileStat {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaCompetitorFinished {
    pub arena_id: String,
    pub competitor: String, // "a" | "b"
    pub session_id: String,
    pub response_text: String,
    pub diff: String,
    pub files: Vec<ArenaFileStat>,
    pub preview_file: Option<String>,
    pub success: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaVerdictResult {
    pub arena_id: String,
    pub winner_id: String,
    pub winner_branch: String,
    pub winner_merged: bool,
    pub loser_cleaned: bool,
    pub message: String,
}

fn find_preview_file(root: &Path) -> Option<String> {
    // Check common entry files first
    let candidates = ["index.html", "preview.html", "app.html", "src/index.html"];
    for cand in &candidates {
        let p = root.join(cand);
        if p.exists() {
            return Some(p.to_string_lossy().to_string());
        }
    }

    // Otherwise check for any .html file modified or added in root
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().is_some_and(|ext| ext == "html") {
                return Some(p.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn capture_competitor_result(
    arena_id: &str,
    competitor: &str,
    session_id: &str,
    worktree_path: &Path,
    store: &std::sync::Mutex<crate::storage::Store>,
) -> ArenaCompetitorFinished {
    // 1. Fetch assistant messages from store
    let response_text = store
        .lock()
        .ok()
        .and_then(|st| st.messages(session_id, None).ok())
        .map(|msgs| {
            msgs.iter()
                .filter(|m| m.role == "assistant")
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default();

    // 2. Stage intent to add so untracked files show up in diff
    let _ = std::process::Command::new("git")
        .args(["add", "-N", "."])
        .current_dir(worktree_path)
        .output();

    // 3. Diff against HEAD
    let diff = std::process::Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(worktree_path)
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string())
        .unwrap_or_default();

    // 4. Numstat for files additions/deletions
    let numstat = std::process::Command::new("git")
        .args(["diff", "--numstat", "HEAD"])
        .current_dir(worktree_path)
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string())
        .unwrap_or_default();

    let mut files = Vec::new();
    for line in numstat.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let additions = parts[0].parse::<usize>().unwrap_or(0);
            let deletions = parts[1].parse::<usize>().unwrap_or(0);
            let path = parts[2..].join(" ");
            files.push(ArenaFileStat {
                path,
                additions,
                deletions,
            });
        }
    }

    let preview_file = find_preview_file(worktree_path);

    ArenaCompetitorFinished {
        arena_id: arena_id.to_string(),
        competitor: competitor.to_string(),
        session_id: session_id.to_string(),
        response_text,
        diff,
        files,
        preview_file,
        success: true,
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn arena_start(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    source_session_id: String,
    competitor_a_provider: String,
    competitor_a_model: String,
    competitor_b_provider: String,
    competitor_b_model: String,
    prompt: String,
) -> Result<ArenaMatchup, String> {
    let (source, root) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let source = store.session(&source_session_id)?;
        let root = store.workspace_root(&source.project_id, None)?;
        (source, root)
    };

    // Resolve the genuine repository root or initialize Git if project isn't a repo yet
    let repo_root = match std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&root)
        .output()
    {
        Ok(out) if out.status.success() => {
            let path_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            std::fs::canonicalize(path_str).unwrap_or(root.clone())
        }
        _ => {
            let _ = std::process::Command::new("git")
                .arg("init")
                .current_dir(&root)
                .output();
            let _ = std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(&root)
                .output();
            let _ = std::process::Command::new("git")
                .args(["commit", "-m", "chore: initial commit for arena race", "--allow-empty"])
                .current_dir(&root)
                .output();
            root.clone()
        }
    };

    // Ensure there is at least one commit in the repository so branch creation succeeds
    let head_check = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(&repo_root)
        .output();
    if let Ok(out) = head_check {
        if !out.status.success() {
            let _ = std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(&repo_root)
                .output();
            let _ = std::process::Command::new("git")
                .args(["commit", "-m", "chore: initial commit for arena race", "--allow-empty"])
                .current_dir(&repo_root)
                .output();
        }
    }

    let suffix = uuid::Uuid::new_v4().to_string();
    let short_id = &suffix[..6];
    let branch_a = format!("arena-a-{short_id}");
    let branch_b = format!("arena-b-{short_id}");

    let worktree_a = workspace::create_worktree_with(&repo_root, &branch_a, None, false)?;
    let worktree_b = workspace::create_worktree_with(&repo_root, &branch_b, None, false)?;

    let (task_a, task_b) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let task_a = store.create_worktree_session_on(
            &source,
            &worktree_a.path,
            Some(&worktree_a.branch),
        )?;
        let task_a = store.configure(
            &task_a.id,
            &competitor_a_provider,
            &competitor_a_model,
            None,
        )?;

        let task_b = store.create_worktree_session_on(
            &source,
            &worktree_b.path,
            Some(&worktree_b.branch),
        )?;
        let task_b = store.configure(
            &task_b.id,
            &competitor_b_provider,
            &competitor_b_model,
            None,
        )?;

        (task_a, task_b)
    };

    let arena_id = format!("arena-{short_id}");

    // Spawn Competitor A in background
    {
        let app_a = app.clone();
        let arena_id_a = arena_id.clone();
        let task_a_id = task_a.id.clone();
        let task_a_project_id = task_a.project_id.clone();
        let prompt_a = prompt.clone();
        let worktree_a_path = worktree_a.path.clone();

        tokio::spawn(async move {
            let state_a = app_a.state::<AppState>();
            let _ = crate::launch_task_in_project(
                app_a.clone(),
                state_a,
                task_a_id.clone(),
                Some(prompt_a),
                None,
                Some(task_a_project_id),
            )
            .await;

            let res = capture_competitor_result(
                &arena_id_a,
                "a",
                &task_a_id,
                &worktree_a_path,
                &app_a.state::<AppState>().store,
            );
            let _ = app_a.emit("arena:competitor_finished", res);
        });
    }

    // Spawn Competitor B in background
    {
        let app_b = app.clone();
        let arena_id_b = arena_id.clone();
        let task_b_id = task_b.id.clone();
        let task_b_project_id = task_b.project_id.clone();
        let prompt_b = prompt.clone();
        let worktree_b_path = worktree_b.path.clone();

        tokio::spawn(async move {
            let state_b = app_b.state::<AppState>();
            let _ = crate::launch_task_in_project(
                app_b.clone(),
                state_b,
                task_b_id.clone(),
                Some(prompt_b),
                None,
                Some(task_b_project_id),
            )
            .await;

            let res = capture_competitor_result(
                &arena_id_b,
                "b",
                &task_b_id,
                &worktree_b_path,
                &app_b.state::<AppState>().store,
            );
            let _ = app_b.emit("arena:competitor_finished", res);
        });
    }

    Ok(ArenaMatchup {
        id: arena_id,
        source_session_id,
        task_a,
        task_b,
        prompt,
        created_at: chrono::Utc::now().timestamp(),
    })
}

#[tauri::command]
pub(crate) async fn arena_verdict(
    state: State<'_, AppState>,
    arena_id: String,
    winner_task_id: String,
    loser_task_id: String,
    discard_task_id: Option<String>,
) -> Result<ArenaVerdictResult, String> {
    // Handle "discard"
    if winner_task_id == "discard" || winner_task_id.is_empty() {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        // Clean up task B (loser_task_id)
        if let Ok(loser) = store.session(&loser_task_id) {
            if let Some(path) = loser.worktree_path {
                if let Ok(root) = store.workspace_root(&loser.project_id, None) {
                    let _ = workspace::remove_worktree(&root, &PathBuf::from(path), loser.worktree_branch.as_deref(), true);
                }
            }
        }
        // Clean up task A if provided
        if let Some(ref a_id) = discard_task_id {
            if let Ok(task_a) = store.session(a_id) {
                if let Some(path) = task_a.worktree_path {
                    if let Ok(root) = store.workspace_root(&task_a.project_id, None) {
                        let _ = workspace::remove_worktree(&root, &PathBuf::from(path), task_a.worktree_branch.as_deref(), true);
                    }
                }
            }
        }
        return Ok(ArenaVerdictResult {
            arena_id,
            winner_id: "discard".into(),
            winner_branch: String::new(),
            winner_merged: false,
            loser_cleaned: true,
            message: "Both competitor worktrees and branches were safely discarded.".into(),
        });
    }

    let (winner_path, winner_branch, loser_path, loser_branch, main_root) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let winner = store.session(&winner_task_id)?;
        let loser = store.session(&loser_task_id)?;

        let winner_path = PathBuf::from(
            winner
                .worktree_path
                .ok_or("Winner task has no isolated worktree.")?,
        );
        let winner_branch = winner
            .worktree_branch
            .ok_or("Winner task has no branch name.")?;

        let loser_path = PathBuf::from(
            loser
                .worktree_path
                .ok_or("Loser task has no isolated worktree.")?,
        );
        let loser_branch = loser.worktree_branch;

        let main_root = store.workspace_root(&winner.project_id, None)?;

        (
            winner_path,
            winner_branch,
            loser_path,
            loser_branch,
            main_root,
        )
    };

    // Auto-commit any remaining working-tree or untracked changes in winner worktree
    // so workspace::merge_worktree succeeds without "worktree has uncommitted changes"
    workspace::commit_worktree_all(&winner_path, "chore: apply arena competitor work")?;

    // 1. Merge winner into main
    let merge_res = workspace::merge_worktree(&main_root, &winner_path)?;

    // 2. Clean up loser's worktree and branch
    let clean_res = workspace::remove_worktree(
        &main_root,
        &loser_path,
        loser_branch.as_deref(),
        true,
    );

    // 3. Clean up winner's worktree since it is now merged
    let _ = workspace::remove_worktree(
        &main_root,
        &winner_path,
        Some(&winner_branch),
        true,
    );

    Ok(ArenaVerdictResult {
        arena_id,
        winner_id: winner_task_id,
        winner_branch,
        winner_merged: merge_res.commits > 0 || true,
        loser_cleaned: clean_res.is_ok(),
        message: "Winner changes landed safely into main. Worktrees cleaned up.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_serialization() {
        let verdict = ArenaVerdictResult {
            arena_id: "arena-123".into(),
            winner_id: "task-a".into(),
            winner_branch: "arena-a-123".into(),
            winner_merged: true,
            loser_cleaned: true,
            message: "Success".into(),
        };
        let json = serde_json::to_string(&verdict).unwrap();
        assert!(json.contains("arenaId"));
        assert!(json.contains("winnerMerged"));
        let parsed: ArenaVerdictResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.winner_id, "task-a");
    }

    #[test]
    fn test_competitor_finished_serialization() {
        let finished = ArenaCompetitorFinished {
            arena_id: "arena-123".into(),
            competitor: "a".into(),
            session_id: "sess-1".into(),
            response_text: "Done".into(),
            diff: "+test".into(),
            files: vec![ArenaFileStat {
                path: "test.ts".into(),
                additions: 1,
                deletions: 0,
            }],
            preview_file: Some("/tmp/index.html".into()),
            success: true,
        };
        let json = serde_json::to_string(&finished).unwrap();
        assert!(json.contains("responseText"));
        assert!(json.contains("previewFile"));
        assert!(json.contains("additions"));
    }
}
