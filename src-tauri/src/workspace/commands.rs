use serde::Serialize;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Serialize, Debug)]
pub struct WorkspaceEntry {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

fn normalize_relative_path(raw: &str) -> String {
    raw.trim().replace('\\', "/").trim_matches('/').to_string()
}

fn canonical_workspace_root(root: &str) -> Result<PathBuf, String> {
    let trimmed = root.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("Workspace root is required".to_string());
    }

    let root_path = PathBuf::from(trimmed);
    let canonical = fs::canonicalize(&root_path)
        .map_err(|e| format!("Failed to access workspace root: {}", e))?;

    if !canonical.is_dir() {
        return Err("Workspace root must be an existing folder".to_string());
    }

    Ok(canonical)
}

fn safe_workspace_path(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let normalized = normalize_relative_path(relative_path);
    if normalized.is_empty() {
        return Ok(root.to_path_buf());
    }

    let relative = Path::new(&normalized);
    if relative.is_absolute() {
        return Err("Path must be relative to workspace".to_string());
    }

    for component in relative.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("Path cannot escape workspace root".to_string())
            }
        }
    }

    Ok(root.join(relative))
}

fn collect_entries(root: &Path, dir: &Path, entries: &mut Vec<WorkspaceEntry>) -> Result<(), String> {
    let read_dir = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read workspace directory {}: {}", dir.display(), e))?;

    for dir_entry in read_dir {
        let dir_entry = dir_entry
            .map_err(|e| format!("Failed to read workspace entry in {}: {}", dir.display(), e))?;
        let path = dir_entry.path();
        let metadata = fs::metadata(&path)
            .map_err(|e| format!("Failed to read metadata for {}: {}", path.display(), e))?;

        let relative = path
            .strip_prefix(root)
            .map_err(|e| format!("Failed to resolve relative path for {}: {}", path.display(), e))?
            .to_string_lossy()
            .replace('\\', "/");

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|| "0".to_string());

        entries.push(WorkspaceEntry {
            name: dir_entry.file_name().to_string_lossy().to_string(),
            relative_path: relative,
            is_dir: metadata.is_dir(),
            size: if metadata.is_file() { metadata.len() } else { 0 },
            modified,
        });

        if metadata.is_dir() {
            collect_entries(root, &path, entries)?;
        }
    }

    Ok(())
}

#[tauri::command]
pub fn list_workspace_entries(root: String) -> Result<Vec<WorkspaceEntry>, String> {
    let canonical_root = canonical_workspace_root(&root)?;
    let mut entries: Vec<WorkspaceEntry> = Vec::new();
    collect_entries(&canonical_root, &canonical_root, &mut entries)?;

    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }

        a.relative_path
            .to_ascii_lowercase()
            .cmp(&b.relative_path.to_ascii_lowercase())
    });

    Ok(entries)
}

#[tauri::command]
pub fn read_workspace_file(root: String, relative_path: String) -> Result<String, String> {
    let canonical_root = canonical_workspace_root(&root)?;
    let target = safe_workspace_path(&canonical_root, &relative_path)?;

    if !target.exists() {
        return Err(format!("File does not exist: {}", relative_path));
    }

    if target.is_dir() {
        return Err("Cannot read directory contents as a file".to_string());
    }

    fs::read_to_string(&target).map_err(|e| format!("Failed to read file: {}", e))
}

#[tauri::command]
pub fn write_workspace_file(root: String, relative_path: String, content: String) -> Result<(), String> {
    let canonical_root = canonical_workspace_root(&root)?;
    let target = safe_workspace_path(&canonical_root, &relative_path)?;

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    fs::write(&target, content).map_err(|e| format!("Failed to write file: {}", e))
}

#[tauri::command]
pub fn create_workspace_entry(
    root: String,
    relative_path: String,
    is_dir: bool,
) -> Result<(), String> {
    let canonical_root = canonical_workspace_root(&root)?;
    let target = safe_workspace_path(&canonical_root, &relative_path)?;

    if target.exists() {
        return Err("Entry already exists".to_string());
    }

    if is_dir {
        fs::create_dir_all(&target).map_err(|e| format!("Failed to create folder: {}", e))
    } else {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent folder: {}", e))?;
        }
        fs::write(&target, "").map_err(|e| format!("Failed to create file: {}", e))
    }
}

#[tauri::command]
pub fn delete_workspace_entry(root: String, relative_path: String) -> Result<(), String> {
    let canonical_root = canonical_workspace_root(&root)?;
    let target = safe_workspace_path(&canonical_root, &relative_path)?;

    if !target.exists() {
        return Err("Entry does not exist".to_string());
    }

    if target.is_dir() {
        fs::remove_dir_all(&target).map_err(|e| format!("Failed to delete folder: {}", e))
    } else {
        fs::remove_file(&target).map_err(|e| format!("Failed to delete file: {}", e))
    }
}
