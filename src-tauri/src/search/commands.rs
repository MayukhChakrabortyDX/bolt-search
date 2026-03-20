use std::fs;
use std::path::PathBuf;

use super::scan::{search_folder_batch_impl, search_impl};
use super::types::{FileEntry, FolderBatchResult, SearchQuery};
use super::utils::{get_drives, roots_from_drive_filters};

#[tauri::command]
pub async fn search(query: SearchQuery) -> Result<Vec<FileEntry>, String> {
    let roots = roots_from_drive_filters(&query.filters);
    tauri::async_runtime::spawn_blocking(move || {
        std::panic::catch_unwind(|| search_impl(query, roots, 10_000))
            .map_err(|_| "Search failed due to an internal error".to_string())
    })
    .await
    .map_err(|e| format!("Search task failed: {e}"))?
}

#[tauri::command]
pub fn list_search_roots() -> Vec<String> {
    get_drives()
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

#[tauri::command]
pub fn list_subfolders(root: String) -> Result<Vec<String>, String> {
    let root_path = PathBuf::from(root.trim());
    if !root_path.exists() || !root_path.is_dir() {
        return Ok(Vec::new());
    }

    let read_dir = fs::read_dir(&root_path).map_err(|e| e.to_string())?;
    let mut folders: Vec<String> = read_dir
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            Some(path.to_string_lossy().to_string())
        })
        .collect();

    folders.sort_unstable();
    Ok(folders)
}

#[tauri::command]
pub async fn search_in_root(
    query: SearchQuery,
    root: String,
    limit: Option<usize>,
) -> Result<Vec<FileEntry>, String> {
    let cap = limit.unwrap_or(10_000).clamp(1, 10_000);
    tauri::async_runtime::spawn_blocking(move || {
        std::panic::catch_unwind(|| {
            let root_path = PathBuf::from(root);
            if !root_path.exists() {
                return Vec::new();
            }
            search_impl(query, vec![root_path], cap)
        })
        .map_err(|_| "Progressive search failed due to an internal error".to_string())
    })
    .await
    .map_err(|e| format!("Progressive search task failed: {e}"))?
}

#[tauri::command]
pub async fn search_folder_batch(
    query: SearchQuery,
    folders: Vec<String>,
    limit: Option<usize>,
    thread_limit: Option<usize>,
) -> Result<FolderBatchResult, String> {
    let cap = limit.unwrap_or(1_000).clamp(1, 10_000);
    let workers = thread_limit.unwrap_or(4).clamp(1, 16);

    tauri::async_runtime::spawn_blocking(move || {
        std::panic::catch_unwind(|| search_folder_batch_impl(query, folders, cap, workers))
            .map_err(|_| "Folder batch search failed due to an internal error".to_string())?
    })
    .await
    .map_err(|e| format!("Folder batch task failed: {e}"))?
}
