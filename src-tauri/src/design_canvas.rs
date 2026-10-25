use crate::{AppState, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignArtboard {
    pub name: String,
    pub filename: String,
    pub path: String,
    pub updated_at: i64,
    pub size_bytes: u64,
}

fn sanitize_filename(filename: &str) -> Result<String> {
    let trimmed = filename.trim();
    if trimmed.is_empty() || trimmed.contains("..") || trimmed.contains('/') || trimmed.contains('\\') {
        return Err("Invalid artboard filename.".into());
    }
    if !trimmed.ends_with(".html") && !trimmed.ends_with(".htm") {
        return Ok(format!("{trimmed}.html"));
    }
    Ok(trimmed.to_string())
}

fn design_directory(state: &State<'_, AppState>, session_id: &str) -> Result<PathBuf> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let task = store.session(session_id)?;
    let root = store.workspace_root(&task.project_id, Some(session_id))?;
    let swarm_dir = root.join(".swarm").join("design");
    if swarm_dir.exists() {
        return Ok(swarm_dir);
    }
    let pandacode_dir = root.join(".pandacode").join("design");
    if pandacode_dir.exists() {
        return Ok(pandacode_dir);
    }
    Ok(swarm_dir)
}

#[tauri::command]
pub(crate) async fn design_list_artboards(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<DesignArtboard>> {
    let dir = design_directory(&state, &session_id)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut artboards = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            if filename.ends_with(".html") || filename.ends_with(".htm") {
                let name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                let metadata = entry.metadata().ok();
                let updated_at = metadata
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

                artboards.push(DesignArtboard {
                    name,
                    filename,
                    path: path.to_string_lossy().to_string(),
                    updated_at,
                    size_bytes,
                });
            }
        }
    }
    artboards.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
    Ok(artboards)
}

#[tauri::command]
pub(crate) async fn design_read_artboard(
    state: State<'_, AppState>,
    session_id: String,
    filename: String,
) -> Result<String> {
    let clean_file = sanitize_filename(&filename)?;
    let dir = design_directory(&state, &session_id)?;
    let file_path = dir.join(&clean_file);
    if !file_path.exists() {
        return Err(format!("Artboard {clean_file} not found."));
    }
    std::fs::read_to_string(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn design_create_artboard(
    state: State<'_, AppState>,
    session_id: String,
    name: String,
    html_content: String,
) -> Result<DesignArtboard> {
    let dir = design_directory(&state, &session_id)?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let filename_base = name
        .trim()
        .to_lowercase()
        .replace([' ', '_', '/'], "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "");
    let filename = if filename_base.is_empty() {
        "artboard.html".to_string()
    } else {
        format!("{filename_base}.html")
    };
    let clean_file = sanitize_filename(&filename)?;
    let file_path = dir.join(&clean_file);

    let content = if html_content.trim().is_empty() {
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{}</title>
  <style>
    body {{
      font-family: system-ui, -apple-system, sans-serif;
      margin: 0;
      padding: 40px;
      background: #f8fafc;
      color: #0f172a;
    }}
    .card {{
      background: #ffffff;
      border-radius: 12px;
      padding: 24px;
      box-shadow: 0 4px 6px -1px rgb(0 0 0 / 0.1);
      max-width: 480px;
      margin: 0 auto;
    }}
  </style>
</head>
<body>
  <div class="card">
    <h2>{}</h2>
    <p>This is a new interactive artboard. Ask your agent to redesign or update this UI.</p>
  </div>
</body>
</html>"#,
            name, name
        )
    } else {
        html_content
    };

    std::fs::write(&file_path, content).map_err(|e| e.to_string())?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let size_bytes = file_path.metadata().map(|m| m.len()).unwrap_or(0);

    Ok(DesignArtboard {
        name,
        filename: clean_file,
        path: file_path.to_string_lossy().to_string(),
        updated_at: now_ms,
        size_bytes,
    })
}

#[tauri::command]
pub(crate) async fn design_delete_artboard(
    state: State<'_, AppState>,
    session_id: String,
    filename: String,
) -> Result<()> {
    let clean_file = sanitize_filename(&filename)?;
    let dir = design_directory(&state, &session_id)?;
    let file_path = dir.join(&clean_file);
    if file_path.exists() {
        std::fs::remove_file(&file_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn design_save_artboard(
    state: State<'_, AppState>,
    session_id: String,
    filename: String,
    html_content: String,
) -> Result<()> {
    let clean_file = sanitize_filename(&filename)?;
    let dir = design_directory(&state, &session_id)?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let file_path = dir.join(&clean_file);
    std::fs::write(&file_path, html_content).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn design_duplicate_artboard(
    state: State<'_, AppState>,
    session_id: String,
    source_filename: String,
    new_name: String,
) -> Result<DesignArtboard> {
    let content = design_read_artboard(state.clone(), session_id.clone(), source_filename).await?;
    design_create_artboard(state, session_id, new_name, content).await
}
