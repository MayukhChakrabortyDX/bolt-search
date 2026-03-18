use std::collections::HashSet;
use std::fs::{self, Metadata};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};
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

#[derive(Debug)]
struct PreparedFilters {
    extensions: HashSet<String>,
    name_contains: Vec<String>,
    path_contains: Vec<String>,
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

fn try_claim_result_slot(remaining: &AtomicUsize) -> bool {
    let mut current = remaining.load(Ordering::Acquire);

    loop {
        if current == 0 {
            return false;
        }

        match remaining.compare_exchange_weak(
            current,
            current - 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return true,
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
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

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

    let file_only   = filters.iter().any(|f| f.kind == "file_only");
    let folder_only = filters.iter().any(|f| f.kind == "folder_only");
    let hidden      = filters.iter().any(|f| f.kind == "hidden");
    let readonly    = filters.iter().any(|f| f.kind == "readonly");

    PreparedFilters {
        extensions,
        name_contains,
        path_contains,
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
    }
}

fn entry_matches(path: &Path, metadata: &Metadata, filters: &PreparedFilters) -> bool {
    let is_dir = metadata.is_dir();

    if filters.file_only && is_dir {
        return false;
    }
    if filters.folder_only && !is_dir {
        return false;
    }

    if filters.hidden {
        let attrs = metadata.file_attributes();
        if attrs & 0x2 == 0 {
            return false;
        }
    }

    if filters.readonly && !metadata.permissions().readonly() {
        return false;
    }

    if !filters.extensions.is_empty() {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_ascii_lowercase()))
            .unwrap_or_default();
        if !filters.extensions.contains(&ext) {
            return false;
        }
    }

    if !filters.name_contains.is_empty() {
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !filters.name_contains.iter().all(|n| name.contains(n)) {
            return false;
        }
    }

    if !filters.path_contains.is_empty() {
        let path_lower = path.to_string_lossy().to_ascii_lowercase();
        if !filters.path_contains.iter().all(|p| path_lower.contains(p)) {
            return false;
        }
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

    let modified_secs = metadata
        .modified()
        .ok()
        .and_then(system_time_to_unix_secs);

    if filters.modified_after.is_some() || filters.modified_before.is_some() {
        let secs = match modified_secs {
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
            let mut root_results = Vec::new();

            for entry in WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if remaining.load(Ordering::Acquire) == 0 {
                    break;
                }

                let path = entry.path();
                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                if !entry_matches(path, &metadata, filters.as_ref()) {
                    continue;
                }

                if !try_claim_result_slot(remaining.as_ref()) {
                    break;
                }

                root_results.push(to_file_entry(path, &metadata));
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