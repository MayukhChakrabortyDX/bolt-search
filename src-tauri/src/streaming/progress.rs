use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;

use crate::search::FileEntry;

const MAX_PROGRESS_ENTRIES_PER_EVENT: usize = 256;

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
pub(super) struct ProgressQueuePayload {
    pub(super) started_folders: Vec<String>,
    pub(super) finished_folders: Vec<String>,
    pub(super) entries: Vec<FileEntry>,
    pub(super) scanned_folders: usize,
    pub(super) total_results: usize,
}

#[derive(Debug)]
pub(super) enum ProgressQueueMessage {
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

pub(super) fn run_progress_flusher(
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
