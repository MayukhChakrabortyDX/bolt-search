use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rayon::prelude::*;
use tauri::ipc::Channel;

use crate::search::{
    entry_matches_with_metadata,
    entry_matches_without_metadata,
    get_thread_pool,
    prepare_filters,
    to_file_entry,
    FileEntry,
    PreparedFilters,
    SearchQuery,
};

use super::progress::{
    run_progress_flusher,
    ProgressQueueMessage,
    ProgressQueuePayload,
    SearchStreamEvent,
};
use super::state::is_run_cancelled;

const FOLDER_CONTEXT_SWITCH_ITEMS: usize = 64;
const CHANNEL_EMIT_INTERVAL_MS: u64 = 300;
const PROGRESS_HEARTBEAT_INTERVAL_MS: u64 = 1_000;
const SLOW_ROUND_LOG_MS: u64 = 1_500;

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

pub(super) fn run_walkdir_search(
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
    let flusher_handle = thread::spawn(move || run_progress_flusher(on_event, progress_rx, flush_interval));

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
                .map(|state| scan_folder_chunk(state, FOLDER_CONTEXT_SWITCH_ITEMS, filters.as_ref(), run_id))
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
        } else if last_progress_emit.elapsed() >= Duration::from_millis(PROGRESS_HEARTBEAT_INTERVAL_MS) {
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
