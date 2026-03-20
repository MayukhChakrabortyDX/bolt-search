mod progress;
mod state;
mod walk;

use tauri::ipc::Channel;

use crate::search::{FileEntry, SearchQuery};

pub use progress::SearchStreamEvent;

use self::state::{
    active_run_id,
    mark_run_finished,
    mark_run_started,
    request_cancel,
    resolve_run_id,
    scheduler_tuning,
};
use self::walk::run_walkdir_search;

#[tauri::command]
pub fn cancel_search(run_id: Option<u64>) -> Result<(), String> {
    let target_run = run_id.unwrap_or_else(active_run_id);
    if target_run == 0 {
        return Ok(());
    }

    request_cancel(target_run);
    Ok(())
}

#[tauri::command]
pub async fn search_streaming(
    query: SearchQuery,
    roots: Vec<String>,
    limit: Option<usize>,
    run_id: Option<u64>,
    _thread_limit: Option<usize>,
    _folder_batch_size: Option<usize>,
    _debounce_ms: Option<u64>,
    on_event: Channel<SearchStreamEvent>,
) -> Result<(), String> {
    let cap = limit.unwrap_or(10_000).clamp(1, 10_000);
    let (workers, batch_size) = scheduler_tuning(roots.len());
    let run_id = resolve_run_id(run_id);

    mark_run_started(run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let result = run_walkdir_search(
            query,
            roots,
            run_id,
            cap,
            workers,
            batch_size,
            true,
            false,
            on_event,
        )
        .map(|_| ());

        mark_run_finished(run_id);
        result
    })
    .await
    .map_err(|e| format!("Streaming task failed: {e}"))?
}

#[tauri::command]
pub async fn search_with_progress(
    query: SearchQuery,
    roots: Vec<String>,
    limit: Option<usize>,
    run_id: Option<u64>,
    _thread_limit: Option<usize>,
    _folder_batch_size: Option<usize>,
    _debounce_ms: Option<u64>,
    on_event: Channel<SearchStreamEvent>,
) -> Result<Vec<FileEntry>, String> {
    let cap = limit.unwrap_or(10_000).clamp(1, 10_000);
    let (workers, batch_size) = scheduler_tuning(roots.len());
    let run_id = resolve_run_id(run_id);

    mark_run_started(run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let result = run_walkdir_search(
            query,
            roots,
            run_id,
            cap,
            workers,
            batch_size,
            false,
            true,
            on_event,
        );

        mark_run_finished(run_id);
        result
    })
    .await
    .map_err(|e| format!("Batch progress task failed: {e}"))?
}
