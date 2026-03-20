use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rayon::prelude::*;
use serde::Serialize;
use tauri::ipc::Channel;

use crate::{
    entry_matches_with_metadata, entry_matches_without_metadata, get_thread_pool,
    prepare_filters, to_file_entry, FileEntry, PreparedFilters, SearchQuery,
};

const FOLDER_CONTEXT_SWITCH_ITEMS: usize = 64;
const CHANNEL_EMIT_INTERVAL_MS: u64 = 300;
const MIN_WORKERS: usize = 2;
const MAX_WORKERS: usize = 16;
const MIN_BATCH_SIZE: usize = 8;
const MAX_BATCH_SIZE: usize = 48;
const MAX_PROGRESS_ENTRIES_PER_EVENT: usize = 256;
const PROGRESS_HEARTBEAT_INTERVAL_MS: u64 = 1_000;
const SLOW_ROUND_LOG_MS: u64 = 1_500;

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

#[derive(Debug)]
struct ProgressQueuePayload {
    started_folders: Vec<String>,
    finished_folders: Vec<String>,
    entries: Vec<FileEntry>,
    scanned_folders: usize,
    total_results: usize,
}

#[derive(Debug)]
enum ProgressQueueMessage {
    Delta(ProgressQueuePayload),
    Complete {
        scanned_folders: usize,
        total_results: usize,
        truncated: bool,
    },
}

fn emit_progress_chunked(
    on_event: &Channel<SearchStreamEvent>,
    pending_started_folders: &mut Vec<String>,
    pending_finished_folders: &mut Vec<String>,
    pending_entries: &mut Vec<FileEntry>,
    scanned_folders: usize,
    total_results: usize,
) -> Result<(), String> {
    let mut include_folder_updates = true;

    while pending_entries.len() > MAX_PROGRESS_ENTRIES_PER_EVENT {
        let entries_chunk: Vec<FileEntry> = pending_entries
            .drain(..MAX_PROGRESS_ENTRIES_PER_EVENT)
            .collect();

        on_event
            .send(SearchStreamEvent::Progress {
                started_folders: if include_folder_updates {
                    std::mem::take(pending_started_folders)
                } else {
                    Vec::new()
                },
                finished_folders: if include_folder_updates {
                    std::mem::take(pending_finished_folders)
                } else {
                    Vec::new()
                },
                entries: entries_chunk,
                scanned_folders,
                total_results,
            })
            .map_err(|e| e.to_string())?;

        include_folder_updates = false;
    }

    if include_folder_updates
        || !pending_started_folders.is_empty()
        || !pending_finished_folders.is_empty()
        || !pending_entries.is_empty()
    {
        on_event
            .send(SearchStreamEvent::Progress {
                started_folders: std::mem::take(pending_started_folders),
                finished_folders: std::mem::take(pending_finished_folders),
                entries: std::mem::take(pending_entries),
                scanned_folders,
                total_results,
            })
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn run_progress_flusher(
    on_event: Channel<SearchStreamEvent>,
    receiver: mpsc::Receiver<ProgressQueueMessage>,
    flush_interval: Duration,
) -> Result<(), String> {
    let mut pending_started_folders: Vec<String> = Vec::new();
    let mut pending_finished_folders: Vec<String> = Vec::new();
    let mut pending_entries: Vec<FileEntry> = Vec::new();
    let mut scanned_folders = 0usize;
    let mut total_results = 0usize;
    let mut last_flush = Instant::now();

    loop {
        match receiver.recv_timeout(flush_interval) {
            Ok(ProgressQueueMessage::Delta(delta)) => {
                pending_started_folders.extend(delta.started_folders);
                pending_finished_folders.extend(delta.finished_folders);
                pending_entries.extend(delta.entries);
                scanned_folders = delta.scanned_folders;
                total_results = delta.total_results;

                let folder_updates =
                    pending_started_folders.len() + pending_finished_folders.len();
                let should_flush = pending_entries.len() >= MAX_PROGRESS_ENTRIES_PER_EVENT
                    || folder_updates >= MAX_PROGRESS_ENTRIES_PER_EVENT
                    || last_flush.elapsed() >= flush_interval;

                if should_flush {
                    emit_progress_chunked(
                        &on_event,
                        &mut pending_started_folders,
                        &mut pending_finished_folders,
                        &mut pending_entries,
                        scanned_folders,
                        total_results,
                    )?;
                    last_flush = Instant::now();
                }
            }
            Ok(ProgressQueueMessage::Complete {
                scanned_folders: completed_scanned,
                total_results: completed_total,
                truncated,
            }) => {
                scanned_folders = completed_scanned;
                total_results = completed_total;

                if !pending_started_folders.is_empty()
                    || !pending_finished_folders.is_empty()
                    || !pending_entries.is_empty()
                {
                    emit_progress_chunked(
                        &on_event,
                        &mut pending_started_folders,
                        &mut pending_finished_folders,
                        &mut pending_entries,
                        scanned_folders,
                        total_results,
                    )?;
                }

                on_event
                    .send(SearchStreamEvent::Completed {
                        scanned_folders,
                        total_results,
                        truncated,
                    })
                    .map_err(|e| e.to_string())?;

                return Ok(());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !pending_started_folders.is_empty()
                    || !pending_finished_folders.is_empty()
                    || !pending_entries.is_empty()
                {
                    emit_progress_chunked(
                        &on_event,
                        &mut pending_started_folders,
                        &mut pending_finished_folders,
                        &mut pending_entries,
                        scanned_folders,
                        total_results,
                    )?;
                    last_flush = Instant::now();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !pending_started_folders.is_empty()
                    || !pending_finished_folders.is_empty()
                    || !pending_entries.is_empty()
                {
                    emit_progress_chunked(
                        &on_event,
                        &mut pending_started_folders,
                        &mut pending_finished_folders,
                        &mut pending_entries,
                        scanned_folders,
                        total_results,
                    )?;
                }

                return Ok(());
            }
        }
    }
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
    reader: Option<fs::ReadDir>,
    started: bool,
}

impl FolderScanState {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            reader: None,
            started: false,
        }
    }
}

#[derive(Debug)]
struct FolderChunkResult {
    entries: Vec<FileEntry>,
    discovered_subfolders: Vec<String>,
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

fn run_walkdir_search(
    query: SearchQuery,
    roots: Vec<String>,
    run_id: u64,
    cap: usize,
    workers: usize,
    batch_size: usize,
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
    let mut final_entries: Vec<FileEntry> = Vec::new();
    let mut cancelled = false;
    let mut last_progress_emit = Instant::now();

    let (progress_tx, progress_rx) = mpsc::channel::<ProgressQueueMessage>();
    let flush_interval = Duration::from_millis(CHANNEL_EMIT_INTERVAL_MS);
    let flusher_handle = thread::spawn(move || {
        run_progress_flusher(on_event, progress_rx, flush_interval)
    });

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

        let mut round_started_folders: Vec<String> = Vec::new();
        let mut round_finished_folders: Vec<String> = Vec::new();
        let mut round_entries: Vec<FileEntry> = Vec::new();

        for state in &mut round_states {
            if !state.started {
                round_started_folders.push(state.path.to_string_lossy().to_string());
                state.started = true;
            }
        }

        let round_started_at = Instant::now();
        let chunk_results: Vec<(FolderScanState, FolderChunkResult)> = pool.install(|| {
            round_states
                .into_par_iter()
                .map(|state| {
                    scan_folder_chunk(
                        state,
                        FOLDER_CONTEXT_SWITCH_ITEMS,
                        filters.as_ref(),
                        run_id,
                    )
                })
                .collect()
        });
        let round_elapsed = round_started_at.elapsed();

        if is_run_cancelled(run_id) {
            cancelled = true;
            break;
        }

        for (state, mut chunk) in chunk_results.into_iter() {
            for subfolder in chunk.discovered_subfolders.drain(..) {
                if seen_folders.insert(subfolder.clone()) {
                    queue.push_back(FolderScanState::new(PathBuf::from(subfolder)));
                }
            }

            if total_results < cap && !chunk.entries.is_empty() {
                let remaining = cap.saturating_sub(total_results);
                if chunk.entries.len() > remaining {
                    chunk.entries.truncate(remaining);
                }

                total_results += chunk.entries.len();

                if emit_entries_in_progress && !collect_final_entries {
                    round_entries.append(&mut chunk.entries);
                } else {
                    if emit_entries_in_progress {
                        round_entries.extend(chunk.entries.iter().cloned());
                    }
                    if collect_final_entries {
                        final_entries.extend(chunk.entries);
                    }
                }
            }

            if chunk.exhausted {
                scanned_folders += 1;
                round_finished_folders.push(state.path.to_string_lossy().to_string());
            } else {
                queue.push_back(state);
            }
        }

        if round_elapsed >= Duration::from_millis(SLOW_ROUND_LOG_MS) {
            let sample = if round_started_folders.is_empty() {
                "<none>".to_string()
            } else {
                round_started_folders
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<String>>()
                    .join(" | ")
            };

            eprintln!(
                "[search-stream] slow round run_id={} elapsed_ms={} round_folders={} queue_after={} scanned_folders={} total_results={} sample={}",
                run_id,
                round_elapsed.as_millis(),
                round_started_folders.len(),
                queue.len(),
                scanned_folders,
                total_results,
                sample,
            );
        }

        if !round_started_folders.is_empty()
            || !round_finished_folders.is_empty()
            || !round_entries.is_empty()
        {
            progress_tx
                .send(ProgressQueueMessage::Delta(ProgressQueuePayload {
                    started_folders: round_started_folders,
                    finished_folders: round_finished_folders,
                    entries: round_entries,
                    scanned_folders,
                    total_results,
                }))
                .map_err(|e| e.to_string())?;
            last_progress_emit = Instant::now();
        } else if last_progress_emit.elapsed()
            >= Duration::from_millis(PROGRESS_HEARTBEAT_INTERVAL_MS)
        {
            progress_tx
                .send(ProgressQueueMessage::Delta(ProgressQueuePayload {
                    started_folders: Vec::new(),
                    finished_folders: Vec::new(),
                    entries: Vec::new(),
                    scanned_folders,
                    total_results,
                }))
                .map_err(|e| e.to_string())?;
            last_progress_emit = Instant::now();
        }
    }

    let truncated = !cancelled && total_results >= cap && !queue.is_empty();

    progress_tx
        .send(ProgressQueueMessage::Complete {
            scanned_folders,
            total_results,
            truncated,
        })
        .map_err(|e| e.to_string())?;

    flusher_handle
        .join()
        .map_err(|_| "Progress flusher thread panicked".to_string())??;

    Ok(final_entries)
}

fn scan_folder_chunk(
    mut state: FolderScanState,
    chunk_size: usize,
    filters: &PreparedFilters,
    run_id: u64,
) -> (FolderScanState, FolderChunkResult) {
    let mut entries = Vec::new();
    let mut discovered_subfolders = Vec::new();
    let mut exhausted = false;

    if state.reader.is_none() {
        state.reader = match fs::read_dir(&state.path) {
            Ok(iter) => Some(iter),
            Err(_) => {
                exhausted = true;
                None
            }
        };
    }

    let mut processed_items = 0usize;

    if let Some(reader) = state.reader.as_mut() {
        while processed_items < chunk_size {
            if is_run_cancelled(run_id) {
                break;
            }

            let Some(next_item) = reader.next() else {
                exhausted = true;
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
            let is_dir = file_type.is_dir();

            if is_dir {
                discovered_subfolders.push(path.to_string_lossy().to_string());
            }

            if !entry_matches_without_metadata(&path, is_dir, filters) {
                continue;
            }

            let metadata = match dir_entry.metadata() {
                Ok(meta) => meta,
                Err(_) => continue,
            };

            if entry_matches_with_metadata(&metadata, filters) {
                entries.push(to_file_entry(&path, &metadata));
            }
        }
    }

    if exhausted {
        state.reader = None;
    }

    (
        state,
        FolderChunkResult {
            entries,
            discovered_subfolders,
            exhausted,
        },
    )
}
