use std::collections::HashSet;
use std::fs::{self, Metadata};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};
use rayon::iter::ParallelBridge;
use rayon::prelude::*;
use walkdir::WalkDir;
use chrono::NaiveDate;

mod streaming;

static THREAD_POOL_CACHE: OnceLock<Mutex<Vec<(usize, Arc<rayon::ThreadPool>)>>> = OnceLock::new();

// ── Input types ───────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct SearchQuery {
    filters: Vec<Filter>,
}

#[derive(Deserialize, Debug)]
struct Filter {
    #[serde(rename = "type")]
    kind: String,
    value: Option<String>,
    value2: Option<String>,
    unit: Option<String>,
}

// ── Output types ──────────────────────────────────────────────────────────────

#[derive(Serialize, Debug, Clone)]
struct FileEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    modified: String,
}

#[derive(Serialize, Debug)]
struct FolderBatchResult {
    entries: Vec<FileEntry>,
    next_folders: Vec<String>,
    scanned_folders: usize,
}

#[derive(Debug)]
struct FolderScanResult {
    entries: Vec<FileEntry>,
    next_folders: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
enum FilterStage {
    EntryKind,
    Extension,
    PathPrefix,
    NameContains,
    PathContains,
    Hidden,
    Readonly,
    SizeRange,
    ModifiedRange,
    CreatedRange,
}

#[derive(Debug)]
struct PreparedFilters {
    extensions: HashSet<String>,
    name_contains: Vec<String>,
    path_contains: Vec<String>,
    path_prefix: Option<String>,
    size_gt: Option<u64>,
    size_lt: Option<u64>,
    modified_after: Option<i64>,
    modified_before: Option<i64>,
    created_after: Option<i64>,
    created_before: Option<i64>,
    file_only: bool,
    folder_only: bool,
    hidden: bool,
    readonly: bool,
    stage_order: Vec<FilterStage>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_size(value: &str, unit: &str) -> Option<u64> {
    let n = value.trim().parse::<u64>().ok()?;
    let multiplier = match unit.trim().to_ascii_uppercase().as_str() {
        "KB" => 1024,
        "MB" => 1024u64.pow(2),
        "GB" => 1024u64.pow(3),
        _    => 1, // B or unknown
    };
    n.checked_mul(multiplier)
}

fn parse_date(value: &str) -> Option<i64> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc().timestamp())
}

fn parse_date_end_exclusive(value: &str) -> Option<i64> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .ok()
        .and_then(|d| d.succ_opt().or(Some(d)))
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc().timestamp())
}

fn merge_lower_bound(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn merge_upper_bound(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn normalize_path_text(input: &str) -> String {
    let mut normalized = input.trim().replace('\\', "/").to_ascii_lowercase();
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    normalized
}

fn normalize_path_for_match(path: &Path) -> String {
    normalize_path_text(&path.to_string_lossy())
}

fn path_starts_with_component(path: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    if path == prefix {
        return true;
    }

    path
        .strip_prefix(prefix)
        .map(|rest| rest.starts_with('/'))
        .unwrap_or(false)
}

fn can_descend_into_dir(path: &Path, filters: &PreparedFilters) -> bool {
    let Some(prefix) = filters.path_prefix.as_deref() else {
        return true;
    };

    let dir_path = normalize_path_for_match(path);

    // Continue descending when this folder is inside the prefix OR
    // this folder is still an ancestor on the way to the prefix.
    path_starts_with_component(&dir_path, prefix)
        || path_starts_with_component(prefix, &dir_path)
}

fn system_time_to_unix_secs(time: SystemTime) -> Option<i64> {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(dur) => i64::try_from(dur.as_secs()).ok(),
        Err(err) => i64::try_from(err.duration().as_secs()).ok().map(|secs| -secs),
    }
}

fn get_drives() -> Vec<PathBuf> {
    ('A'..='Z')
        .map(|c| PathBuf::from(format!("{}:\\", c)))
        .filter(|p| p.exists())
        .collect()
}

fn get_thread_pool(workers: usize) -> Result<Arc<rayon::ThreadPool>, String> {
    let cache = THREAD_POOL_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = cache.lock().map_err(|_| "Thread pool cache lock poisoned".to_string())?;

    if let Some((_, pool)) = guard.iter().find(|(w, _)| *w == workers) {
        return Ok(pool.clone());
    }

    let pool = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|e| e.to_string())?,
    );

    guard.push((workers, pool.clone()));
    Ok(pool)
}

fn claim_result_budget(remaining: &AtomicUsize, max_budget: usize) -> usize {
    let mut current = remaining.load(Ordering::Acquire);

    loop {
        if current == 0 {
            return 0;
        }

        let grant = current.min(max_budget.max(1));

        match remaining.compare_exchange_weak(
            current,
            current - grant,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return grant,
            Err(next) => current = next,
        }
    }
}

fn roots_from_drive_filters(filters: &[Filter]) -> Vec<PathBuf> {
    let selected_subfolders: Vec<PathBuf> = filters
        .iter()
        .filter(|f| f.kind == "subfolder")
        .filter_map(|f| f.value.as_deref())
        .flat_map(|v| v.lines())
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists() && p.is_dir())
        .collect();

    if !selected_subfolders.is_empty() {
        return selected_subfolders;
    }

    let selected: Vec<String> = filters
        .iter()
        .filter(|f| f.kind == "drive")
        .filter_map(|f| f.value.as_deref())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();

    if selected.is_empty() || selected.iter().any(|v| v.eq_ignore_ascii_case("ALL")) {
        return get_drives();
    }

    selected
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect()
}

fn build_filter_stage_order(filters: &PreparedFilters) -> Vec<FilterStage> {
    // Run high-pruning checks before expensive timestamp checks so each rejection
    // reduces the amount of work done by later stages.
    let mut stages: Vec<(u8, FilterStage)> = Vec::new();

    if filters.file_only || filters.folder_only {
        stages.push((100, FilterStage::EntryKind));
    }
    if !filters.extensions.is_empty() {
        stages.push((95, FilterStage::Extension));
    }
    if filters.path_prefix.is_some() {
        stages.push((90, FilterStage::PathPrefix));
    }
    if !filters.name_contains.is_empty() {
        stages.push((85, FilterStage::NameContains));
    }
    if !filters.path_contains.is_empty() {
        stages.push((80, FilterStage::PathContains));
    }
    if filters.hidden {
        stages.push((70, FilterStage::Hidden));
    }
    if filters.readonly {
        stages.push((68, FilterStage::Readonly));
    }
    if filters.size_gt.is_some() || filters.size_lt.is_some() {
        stages.push((65, FilterStage::SizeRange));
    }
    if filters.modified_after.is_some() || filters.modified_before.is_some() {
        stages.push((45, FilterStage::ModifiedRange));
    }
    if filters.created_after.is_some() || filters.created_before.is_some() {
        stages.push((40, FilterStage::CreatedRange));
    }

    stages.sort_by(|a, b| b.0.cmp(&a.0));
    stages.into_iter().map(|(_, stage)| stage).collect()
}

fn prepare_filters(filters: &[Filter]) -> PreparedFilters {
    let extensions: HashSet<String> = filters.iter()
        .filter(|f| f.kind == "extension")
        .flat_map(|f| {
            f.value.as_deref().unwrap_or("").split(',')
                .filter_map(|s| {
                    let mut ext = s.trim().to_ascii_lowercase();
                    if ext.is_empty() {
                        return None;
                    }
                    if !ext.starts_with('.') {
                        ext.insert(0, '.');
                    }
                    Some(ext)
                })
        })
        .collect();

    let name_contains: Vec<String> = filters.iter()
        .filter(|f| f.kind == "name_contains")
        .filter_map(|f| f.value.as_deref())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let path_contains: Vec<String> = filters.iter()
        .filter(|f| f.kind == "path_contains")
        .filter_map(|f| f.value.as_deref())
        .map(normalize_path_text)
        .filter(|s| !s.is_empty())
        .collect();

    let path_prefix = filters
        .iter()
        .find(|f| f.kind == "path_prefix")
        .and_then(|f| f.value.as_deref())
        .map(normalize_path_text)
        .filter(|s| !s.is_empty());

    let size_gt = filters.iter().find(|f| f.kind == "size_gt")
        .and_then(|f| parse_size(
            f.value.as_deref().unwrap_or(""),
            f.unit.as_deref().unwrap_or("B"),
        ));

    let size_lt = filters.iter().find(|f| f.kind == "size_lt")
        .and_then(|f| parse_size(
            f.value.as_deref().unwrap_or(""),
            f.unit.as_deref().unwrap_or("B"),
        ));

    let modified_after  = filters.iter().find(|f| f.kind == "modified_after")
        .and_then(|f| f.value.as_deref()).and_then(parse_date);
    let modified_before = filters.iter().find(|f| f.kind == "modified_before")
        .and_then(|f| f.value.as_deref()).and_then(parse_date);
    let created_after   = filters.iter().find(|f| f.kind == "created_after")
        .and_then(|f| f.value.as_deref()).and_then(parse_date);
    let created_before  = filters.iter().find(|f| f.kind == "created_before")
        .and_then(|f| f.value.as_deref()).and_then(parse_date);

    let modified_range = filters.iter().find(|f| f.kind == "modified_range");
    let modified_range_start = modified_range
        .and_then(|f| f.value.as_deref())
        .and_then(parse_date);
    let modified_range_end = modified_range
        .and_then(|f| f.value2.as_deref())
        .and_then(parse_date_end_exclusive);

    let created_range = filters.iter().find(|f| f.kind == "created_range");
    let created_range_start = created_range
        .and_then(|f| f.value.as_deref())
        .and_then(parse_date);
    let created_range_end = created_range
        .and_then(|f| f.value2.as_deref())
        .and_then(parse_date_end_exclusive);

    let modified_after = merge_lower_bound(modified_after, modified_range_start);
    let modified_before = merge_upper_bound(modified_before, modified_range_end);
    let created_after = merge_lower_bound(created_after, created_range_start);
    let created_before = merge_upper_bound(created_before, created_range_end);

    let file_only   = filters.iter().any(|f| f.kind == "file_only");
    let folder_only = filters.iter().any(|f| f.kind == "folder_only");
    let hidden      = filters.iter().any(|f| f.kind == "hidden");
    let readonly    = filters.iter().any(|f| f.kind == "readonly");

    let mut prepared = PreparedFilters {
        extensions,
        name_contains,
        path_contains,
        path_prefix,
        size_gt,
        size_lt,
        modified_after,
        modified_before,
        created_after,
        created_before,
        file_only,
        folder_only,
        hidden,
        readonly,
        stage_order: Vec::new(),
    };

    prepared.stage_order = build_filter_stage_order(&prepared);
    prepared
}

fn entry_matches(path: &Path, metadata: &Metadata, filters: &PreparedFilters) -> bool {
    let is_dir = metadata.is_dir();
    let mut path_normalized_cache: Option<String> = None;
    let mut modified_secs_cache: Option<Option<i64>> = None;
    let mut created_secs_cache: Option<Option<i64>> = None;

    for stage in &filters.stage_order {
        match stage {
            FilterStage::EntryKind => {
                if filters.file_only && is_dir {
                    return false;
                }
                if filters.folder_only && !is_dir {
                    return false;
                }
            }
            FilterStage::Extension => {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| format!(".{}", e.to_ascii_lowercase()))
                    .unwrap_or_default();
                if !filters.extensions.contains(&ext) {
                    return false;
                }
            }
            FilterStage::PathPrefix => {
                let path_normalized = path_normalized_cache
                    .get_or_insert_with(|| normalize_path_for_match(path));
                if let Some(prefix) = filters.path_prefix.as_deref() {
                    if !path_starts_with_component(path_normalized, prefix) {
                        return false;
                    }
                }
            }
            FilterStage::NameContains => {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !filters.name_contains.iter().all(|n| name.contains(n)) {
                    return false;
                }
            }
            FilterStage::PathContains => {
                let path_normalized = path_normalized_cache
                    .get_or_insert_with(|| normalize_path_for_match(path));
                if !filters
                    .path_contains
                    .iter()
                    .all(|p| path_normalized.contains(p))
                {
                    return false;
                }
            }
            FilterStage::Hidden => {
                let attrs = metadata.file_attributes();
                if attrs & 0x2 == 0 {
                    return false;
                }
            }
            FilterStage::Readonly => {
                if !metadata.permissions().readonly() {
                    return false;
                }
            }
            FilterStage::SizeRange => {
                let size = metadata.len();
                if let Some(gt) = filters.size_gt {
                    if size <= gt {
                        return false;
                    }
                }
                if let Some(lt) = filters.size_lt {
                    if size >= lt {
                        return false;
                    }
                }
            }
            FilterStage::ModifiedRange => {
                let secs_opt = match modified_secs_cache {
                    Some(value) => value,
                    None => {
                        let value = metadata
                            .modified()
                            .ok()
                            .and_then(system_time_to_unix_secs);
                        modified_secs_cache = Some(value);
                        value
                    }
                };

                let secs = match secs_opt {
                    Some(secs) => secs,
                    None => return false,
                };

                // Lower-bound check first, then upper-bound check to short-circuit quickly.
                if let Some(after) = filters.modified_after {
                    if secs <= after {
                        return false;
                    }
                }
                if let Some(before) = filters.modified_before {
                    if secs >= before {
                        return false;
                    }
                }
            }
            FilterStage::CreatedRange => {
                let secs_opt = match created_secs_cache {
                    Some(value) => value,
                    None => {
                        let value = metadata.created().ok().and_then(system_time_to_unix_secs);
                        created_secs_cache = Some(value);
                        value
                    }
                };

                let secs = match secs_opt {
                    Some(secs) => secs,
                    None => return false,
                };

                // Lower-bound check first, then upper-bound check to short-circuit quickly.
                if let Some(after) = filters.created_after {
                    if secs <= after {
                        return false;
                    }
                }
                if let Some(before) = filters.created_before {
                    if secs >= before {
                        return false;
                    }
                }
            }
        }
    }

    true
}

fn entry_matches_without_metadata(path: &Path, is_dir: bool, filters: &PreparedFilters) -> bool {
    if filters.file_only && is_dir {
        return false;
    }
    if filters.folder_only && !is_dir {
        return false;
    }

    if !filters.extensions.is_empty() {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_ascii_lowercase()))
            .unwrap_or_default();
        if !filters.extensions.contains(&ext) {
            return false;
        }
    }

    if !filters.name_contains.is_empty() {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !filters.name_contains.iter().all(|n| name.contains(n)) {
            return false;
        }
    }

    if !filters.path_contains.is_empty() || filters.path_prefix.is_some() {
        let path_normalized = normalize_path_for_match(path);

        if let Some(prefix) = filters.path_prefix.as_deref() {
            if !path_starts_with_component(&path_normalized, prefix) {
                return false;
            }
        }

        if !filters
            .path_contains
            .iter()
            .all(|p| path_normalized.contains(p))
        {
            return false;
        }
    }

    true
}

fn entry_matches_with_metadata(metadata: &Metadata, filters: &PreparedFilters) -> bool {
    if filters.hidden {
        let attrs = metadata.file_attributes();
        if attrs & 0x2 == 0 {
            return false;
        }
    }

    if filters.readonly && !metadata.permissions().readonly() {
        return false;
    }

    let size = metadata.len();
    if let Some(gt) = filters.size_gt {
        if size <= gt {
            return false;
        }
    }
    if let Some(lt) = filters.size_lt {
        if size >= lt {
            return false;
        }
    }

    if filters.modified_after.is_some() || filters.modified_before.is_some() {
        let secs = match metadata.modified().ok().and_then(system_time_to_unix_secs) {
            Some(secs) => secs,
            None => return false,
        };

        if let Some(after) = filters.modified_after {
            if secs <= after {
                return false;
            }
        }
        if let Some(before) = filters.modified_before {
            if secs >= before {
                return false;
            }
        }
    }

    if filters.created_after.is_some() || filters.created_before.is_some() {
        let secs = match metadata.created().ok().and_then(system_time_to_unix_secs) {
            Some(secs) => secs,
            None => return false,
        };

        if let Some(after) = filters.created_after {
            if secs <= after {
                return false;
            }
        }
        if let Some(before) = filters.created_before {
            if secs >= before {
                return false;
            }
        }
    }

    true
}

fn to_file_entry(path: &Path, metadata: &Metadata) -> FileEntry {
    let name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let modified = metadata
        .modified()
        .ok()
        .and_then(system_time_to_unix_secs)
        .map(|s| s.to_string())
        .unwrap_or_default();

    FileEntry {
        name,
        path: path.to_string_lossy().to_string(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
        modified,
    }
}

fn scan_folder_once(folder: &Path, filters: &PreparedFilters) -> FolderScanResult {
    let mut entries = Vec::new();
    let mut next_folders = Vec::new();

    let read_dir = match fs::read_dir(folder) {
        Ok(iter) => iter,
        Err(_) => {
            return FolderScanResult { entries, next_folders };
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
        let metadata = match dir_entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            next_folders.push(path.to_string_lossy().to_string());
        }

        if entry_matches(&path, &metadata, filters) {
            entries.push(to_file_entry(&path, &metadata));
        }
    }

    FolderScanResult { entries, next_folders }
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[tauri::command]
async fn search(query: SearchQuery) -> Result<Vec<FileEntry>, String> {
    let roots = roots_from_drive_filters(&query.filters);
    tauri::async_runtime::spawn_blocking(move || {
        std::panic::catch_unwind(|| search_impl(query, roots, 10_000))
            .map_err(|_| "Search failed due to an internal error".to_string())
    })
    .await
    .map_err(|e| format!("Search task failed: {e}"))?
}

#[tauri::command]
fn list_search_roots() -> Vec<String> {
    get_drives()
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

#[tauri::command]
fn list_subfolders(root: String) -> Result<Vec<String>, String> {
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
async fn search_in_root(query: SearchQuery, root: String, limit: Option<usize>) -> Result<Vec<FileEntry>, String> {
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
async fn search_folder_batch(
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

fn search_folder_batch_impl(
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

fn search_impl(query: SearchQuery, roots: Vec<PathBuf>, max_results: usize) -> Vec<FileEntry> {
    if roots.is_empty() || max_results == 0 {
        return Vec::new();
    }

    let filters = Arc::new(prepare_filters(&query.filters));
    let remaining = Arc::new(AtomicUsize::new(max_results));

    let per_root: Vec<Vec<FileEntry>> = roots
        .into_par_iter()
        .map(|root| {
            let walk_iter = WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| {
                    !entry.file_type().is_dir()
                        || can_descend_into_dir(entry.path(), filters.as_ref())
                });

            let root_state = walk_iter
                .par_bridge()
                .fold(
                    || (Vec::new(), 0usize),
                    |mut state, entry_result| {
                        if remaining.load(Ordering::Acquire) == 0 {
                            return state;
                        }

                        let entry = match entry_result {
                            Ok(entry) => entry,
                            Err(_) => return state,
                        };

                        let is_dir = entry.file_type().is_dir();
                        let path = entry.path();

                        if !entry_matches_without_metadata(path, is_dir, filters.as_ref()) {
                            return state;
                        }

                        let metadata = match entry.metadata() {
                            Ok(meta) => meta,
                            Err(_) => return state,
                        };

                        if !entry_matches_with_metadata(&metadata, filters.as_ref()) {
                            return state;
                        }

                        if state.1 == 0 {
                            state.1 = claim_result_budget(remaining.as_ref(), 32);
                            if state.1 == 0 {
                                return state;
                            }
                        }

                        state.1 -= 1;
                        state.0.push(to_file_entry(path, &metadata));
                        state
                    },
                )
                .reduce(
                    || (Vec::new(), 0usize),
                    |mut left, mut right| {
                        left.0.append(&mut right.0);
                        left.1 += right.1;
                        left
                    },
                );

            if root_state.1 > 0 {
                remaining.fetch_add(root_state.1, Ordering::AcqRel);
            }

            root_state.0
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

#[tauri::command]
fn open_in_explorer(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }

    // Explorer expects Windows separators for /select and can misparse
    // combined `/select,<path>` when file names include special characters.
    let cleaned = trimmed.trim_matches('"').replace('/', "\\");
    let input_path = PathBuf::from(cleaned);
    let target = fs::canonicalize(&input_path).unwrap_or(input_path);

    if target.is_file() {
        Command::new("explorer")
            .arg("/select,")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to reveal file in Explorer: {}", e))
    } else if target.is_dir() {
        Command::new("explorer")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to open folder in Explorer: {}", e))
    } else {
        Err(format!("Path does not exist: {}", target.display()))
    }
}

#[tauri::command]
fn save_filter_file(path: String, content: String) -> Result<(), String> {
    let trimmed = path.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }

    let target = PathBuf::from(trimmed);
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create filter directory: {}", e))?;
        }
    }

    fs::write(&target, content)
        .map_err(|e| format!("Failed to save filter file: {}", e))
}

#[tauri::command]
fn load_filter_file(path: String) -> Result<String, String> {
    let trimmed = path.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }

    let target = PathBuf::from(trimmed);
    fs::read_to_string(&target)
        .map_err(|e| format!("Failed to load filter file: {}", e))
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .invoke_handler(tauri::generate_handler![
        search,
        list_search_roots,
        list_subfolders,
        search_in_root,
        search_folder_batch,
        streaming::cancel_search,
        streaming::search_streaming,
        streaming::search_with_progress,
        open_in_explorer,
        save_filter_file,
        load_filter_file
    ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}