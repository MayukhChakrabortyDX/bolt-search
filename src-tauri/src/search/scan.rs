use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rayon::prelude::*;
use walkdir::WalkDir;

use super::filters::prepare_filters;
use super::matcher::{entry_matches_with_metadata, entry_matches_without_metadata, to_file_entry};
use super::pool::{claim_result_budget, get_thread_pool};
use super::types::{FileEntry, FolderBatchResult, FolderScanResult, SearchQuery};
use super::utils::can_descend_into_dir;

fn scan_folder_once(folder: &Path, filters: &super::types::PreparedFilters) -> FolderScanResult {
    let mut entries = Vec::new();
    let mut next_folders = Vec::new();

    let read_dir = match fs::read_dir(folder) {
        Ok(iter) => iter,
        Err(_) => {
            return FolderScanResult {
                entries,
                next_folders,
            }
        }
    };

    for dir_entry in read_dir.flatten() {
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
            next_folders.push(path.to_string_lossy().to_string());
        }

        if !entry_matches_without_metadata(&path, is_dir, filters) {
            continue;
        }

        let metadata = match dir_entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if entry_matches_with_metadata(&metadata, filters) {
            entries.push(to_file_entry(&path, &metadata));
        }
    }

    FolderScanResult {
        entries,
        next_folders,
    }
}

pub(crate) fn search_folder_batch_impl(
    query: SearchQuery,
    folders: Vec<String>,
    cap: usize,
    workers: usize,
) -> Result<FolderBatchResult, String> {
    let filters = Arc::new(prepare_filters(&query.filters));

    let cleaned_folders: Vec<PathBuf> = folders
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.exists() && p.is_dir())
        .collect();

    if cleaned_folders.is_empty() {
        return Ok(FolderBatchResult {
            entries: Vec::new(),
            next_folders: Vec::new(),
            scanned_folders: 0,
        });
    }

    let pool = get_thread_pool(workers)?;

    let scans: Vec<FolderScanResult> = pool.install(|| {
        cleaned_folders
            .par_iter()
            .map(|folder| scan_folder_once(folder, filters.as_ref()))
            .collect()
    });

    let mut entries = Vec::new();
    let mut next_set = HashSet::new();

    for scan in scans {
        for entry in scan.entries {
            if entries.len() >= cap {
                break;
            }
            entries.push(entry);
        }

        for folder in scan.next_folders {
            next_set.insert(folder);
        }
    }

    let mut next_folders: Vec<String> = next_set.into_iter().collect();
    next_folders.sort_unstable();

    Ok(FolderBatchResult {
        entries,
        next_folders,
        scanned_folders: cleaned_folders.len(),
    })
}

pub(crate) fn search_impl(
    query: SearchQuery,
    roots: Vec<PathBuf>,
    max_results: usize,
) -> Vec<FileEntry> {
    if roots.is_empty() || max_results == 0 {
        return Vec::new();
    }

    let filters = Arc::new(prepare_filters(&query.filters));
    let remaining = Arc::new(AtomicUsize::new(max_results));

    let per_root: Vec<Vec<FileEntry>> = roots
        .into_par_iter()
        .map(|root| {
            let mut root_results = Vec::new();
            let mut local_budget = 0usize;

            for entry in WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| {
                    !entry.file_type().is_dir()
                        || can_descend_into_dir(entry.path(), filters.as_ref())
                })
                .filter_map(|e| e.ok())
            {
                if remaining.load(Ordering::Acquire) == 0 && local_budget == 0 {
                    break;
                }

                let is_dir = entry.file_type().is_dir();
                let path = entry.path();

                if !entry_matches_without_metadata(path, is_dir, filters.as_ref()) {
                    continue;
                }

                let metadata = match entry.metadata() {
                    Ok(meta) => meta,
                    Err(_) => continue,
                };

                if !entry_matches_with_metadata(&metadata, filters.as_ref()) {
                    continue;
                }

                if local_budget == 0 {
                    local_budget = claim_result_budget(remaining.as_ref(), 32);
                    if local_budget == 0 {
                        break;
                    }
                }

                local_budget -= 1;
                root_results.push(to_file_entry(path, &metadata));
            }

            if local_budget > 0 {
                remaining.fetch_add(local_budget, Ordering::AcqRel);
            }

            root_results
        })
        .collect();

    let mut results = Vec::with_capacity(max_results.min(2048));
    for mut chunk in per_root {
        for entry in chunk.drain(..) {
            results.push(entry);
            if results.len() >= max_results {
                return results;
            }
        }
    }

    results
}
