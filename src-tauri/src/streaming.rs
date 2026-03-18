use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rayon::prelude::*;
use serde::Serialize;
use tauri::ipc::Channel;

use crate::{
    entry_matches, get_thread_pool, prepare_filters, to_file_entry, FileEntry, PreparedFilters,
    SearchQuery,
};

const FOLDER_CONTEXT_SWITCH_ITEMS: usize = 100;
const CHANNEL_EMIT_INTERVAL_MS: u64 = 200;
const MIN_WORKERS: usize = 2;
const MAX_WORKERS: usize = 16;
const MIN_BATCH_SIZE: usize = 8;
const MAX_BATCH_SIZE: usize = 128;

static ACTIVE_SEARCH_RUN_ID: AtomicU64 = AtomicU64::new(0);
static CANCEL_REQUEST_RUN_ID: AtomicU64 = AtomicU64::new(0);
static INTERNAL_RUN_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn resolve_run_id(run_id: Option<u64>) -> u64 {
    run_id.unwrap_or_else(|| INTERNAL_RUN_ID_COUNTER.fetch_add(1, Ordering::AcqRel))
}

fn mark_run_started(run_id: u64) {
    CANCEL_REQUEST_RUN_ID.store(0, Ordering::Release);
    ACTIVE_SEARCH_RUN_ID.store(run_id, Ordering::Release);
}

fn mark_run_finished(run_id: u64) {
    let _ = ACTIVE_SEARCH_RUN_ID.compare_exchange(run_id, 0, Ordering::AcqRel, Ordering::Acquire);

    if CANCEL_REQUEST_RUN_ID.load(Ordering::Acquire) == run_id {
        CANCEL_REQUEST_RUN_ID.store(0, Ordering::Release);
    }
}

fn is_run_cancelled(run_id: u64) -> bool {
    CANCEL_REQUEST_RUN_ID.load(Ordering::Acquire) == run_id
        || ACTIVE_SEARCH_RUN_ID.load(Ordering::Acquire) != run_id
}

fn scheduler_tuning(root_count: usize) -> (usize, usize) {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(6);

    let workers = cores.clamp(MIN_WORKERS, MAX_WORKERS);

    // Keep a wider round-robin when scanning many roots while preserving bounded memory.
    let root_factor = root_count.clamp(1, 8);
    let batch_size = (workers * root_factor)
        .clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE);

    (workers, batch_size)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "event", content = "data")]
pub enum SearchStreamEvent {
    Progress {
        started_folders: Vec<String>,
        finished_folders: Vec<String>,
        entries: Vec<FileEntry>,
        scanned_folders: usize,
        total_results: usize,
    },
    Completed {
        scanned_folders: usize,
        total_results: usize,
        truncated: bool,
    },
}

#[tauri::command]
pub fn cancel_search(run_id: Option<u64>) -> Result<(), String> {
    let target_run = run_id.unwrap_or_else(|| ACTIVE_SEARCH_RUN_ID.load(Ordering::Acquire));
    if target_run == 0 {
        return Ok(());
    }

    CANCEL_REQUEST_RUN_ID.store(target_run, Ordering::Release);
    Ok(())
}

#[derive(Debug)]
struct FolderScanState {
    path: PathBuf,
    next_index: usize,
    buffered_subfolders: Vec<String>,
}

impl FolderScanState {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            next_index: 0,
            buffered_subfolders: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct FolderChunkResult {
    entries: Vec<FileEntry>,
    discovered_subfolders: Vec<String>,
    processed_items: usize,
    exhausted: bool,
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
    let debounce = Duration::from_millis(CHANNEL_EMIT_INTERVAL_MS);
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
            debounce,
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
    let debounce = Duration::from_millis(CHANNEL_EMIT_INTERVAL_MS);
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
            debounce,
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

fn run_walkdir_search(
    query: SearchQuery,
    roots: Vec<String>,
    run_id: u64,
    cap: usize,
    workers: usize,
    batch_size: usize,
    debounce: Duration,
    emit_entries_in_progress: bool,
    collect_final_entries: bool,
    on_event: Channel<SearchStreamEvent>,
) -> Result<Vec<FileEntry>, String> {
    let filters = Arc::new(prepare_filters(&query.filters));
    let pool = get_thread_pool(workers)?;

    let mut seen_folders: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<FolderScanState> = VecDeque::new();

    for root in roots {
        let root_path = PathBuf::from(root.trim());
        if !root_path.exists() || !root_path.is_dir() {
            continue;
        }

        let root_key = root_path.to_string_lossy().to_string();
        if seen_folders.insert(root_key) {
            queue.push_back(FolderScanState::new(root_path));
        }
    }

    if queue.is_empty() {
        on_event
            .send(SearchStreamEvent::Completed {
                scanned_folders: 0,
                total_results: 0,
                truncated: false,
            })
            .map_err(|e| e.to_string())?;
        return Ok(Vec::new());
    }

    let mut scanned_folders = 0usize;
    let mut total_results = 0usize;
    let mut pending_started_folders: Vec<String> = Vec::new();
    let mut pending_finished_folders: Vec<String> = Vec::new();
    let mut pending_entries: Vec<FileEntry> = Vec::new();
    let mut final_entries: Vec<FileEntry> = Vec::new();
    let mut last_emit = Instant::now();
    let mut cancelled = false;

    while !queue.is_empty() {
        if total_results >= cap {
            break;
        }

        if is_run_cancelled(run_id) {
            cancelled = true;
            break;
        }

        let mut round_states: Vec<FolderScanState> = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            let Some(state) = queue.pop_front() else {
                break;
            };
            round_states.push(state);
        }

        if round_states.is_empty() {
            continue;
        }

        pending_started_folders.extend(
            round_states
                .iter()
                .filter(|state| state.next_index == 0)
                .map(|state| state.path.to_string_lossy().to_string()),
        );

        let chunk_results: Vec<FolderChunkResult> = pool.install(|| {
            round_states
                .par_iter()
                .map(|state| {
                    scan_folder_chunk(
                        &state.path,
                        state.next_index,
                        FOLDER_CONTEXT_SWITCH_ITEMS,
                        filters.as_ref(),
                        run_id,
                    )
                })
                .collect()
        });

        if is_run_cancelled(run_id) {
            cancelled = true;
            break;
        }

        for (mut state, mut chunk) in round_states.into_iter().zip(chunk_results.into_iter()) {
            state.buffered_subfolders.append(&mut chunk.discovered_subfolders);
            state.next_index += chunk.processed_items;

            if total_results < cap && !chunk.entries.is_empty() {
                let remaining = cap.saturating_sub(total_results);
                if chunk.entries.len() > remaining {
                    chunk.entries.truncate(remaining);
                }

                total_results += chunk.entries.len();

                if emit_entries_in_progress {
                    pending_entries.extend(chunk.entries.iter().cloned());
                }
                if collect_final_entries {
                    final_entries.extend(chunk.entries);
                }
            }

            if chunk.exhausted {
                scanned_folders += 1;
                pending_finished_folders.push(state.path.to_string_lossy().to_string());

                for subfolder in state.buffered_subfolders {
                    if seen_folders.insert(subfolder.clone()) {
                        queue.push_back(FolderScanState::new(PathBuf::from(subfolder)));
                    }
                }
            } else {
                queue.push_back(state);
            }
        }

        if last_emit.elapsed() >= debounce {
            on_event
                .send(SearchStreamEvent::Progress {
                    started_folders: std::mem::take(&mut pending_started_folders),
                    finished_folders: std::mem::take(&mut pending_finished_folders),
                    entries: std::mem::take(&mut pending_entries),
                    scanned_folders,
                    total_results,
                })
                .map_err(|e| e.to_string())?;
            last_emit = Instant::now();
        }
    }

    if !pending_started_folders.is_empty()
        || !pending_finished_folders.is_empty()
        || !pending_entries.is_empty()
    {
        on_event
            .send(SearchStreamEvent::Progress {
                started_folders: pending_started_folders,
                finished_folders: pending_finished_folders,
                entries: pending_entries,
                scanned_folders,
                total_results,
            })
            .map_err(|e| e.to_string())?;
    }

    let truncated = !cancelled && total_results >= cap && !queue.is_empty();

    on_event
        .send(SearchStreamEvent::Completed {
            scanned_folders,
            total_results,
            truncated,
        })
        .map_err(|e| e.to_string())?;

    Ok(final_entries)
}

fn scan_folder_chunk(
    folder: &Path,
    start_index: usize,
    chunk_size: usize,
    filters: &PreparedFilters,
    run_id: u64,
) -> FolderChunkResult {
    let mut entries = Vec::new();
    let mut discovered_subfolders = Vec::new();

    let read_dir = match fs::read_dir(folder) {
        Ok(iter) => iter,
        Err(_) => {
            return FolderChunkResult {
                entries,
                discovered_subfolders,
                processed_items: 0,
                exhausted: true,
            };
        }
    };

    let mut iter = read_dir.skip(start_index).peekable();
    let mut processed_items = 0usize;

    while processed_items < chunk_size {
        if is_run_cancelled(run_id) {
            break;
        }

        let Some(next_item) = iter.next() else {
            break;
        };

        processed_items += 1;

        let dir_entry = match next_item {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let file_type = match dir_entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if file_type.is_symlink() {
            continue;
        }

        let path = dir_entry.path();
        let metadata = match dir_entry.metadata() {
            Ok(meta) => meta,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            discovered_subfolders.push(path.to_string_lossy().to_string());
        }

        if entry_matches(&path, &metadata, filters) {
            entries.push(to_file_entry(&path, &metadata));
        }
    }

    let exhausted = iter.peek().is_none();

    FolderChunkResult {
        entries,
        discovered_subfolders,
        processed_items,
        exhausted,
    }
}
