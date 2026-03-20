use std::fs::Metadata;
use std::os::windows::fs::MetadataExt;
use std::path::Path;

use super::types::{FileEntry, FilterStage, PreparedFilters};
use super::utils::{
    normalize_path_for_match, path_starts_with_component, system_time_to_unix_secs,
};

#[allow(dead_code)]
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
                let path_normalized =
                    path_normalized_cache.get_or_insert_with(|| normalize_path_for_match(path));
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
                let path_normalized =
                    path_normalized_cache.get_or_insert_with(|| normalize_path_for_match(path));
                if !filters.path_contains.iter().all(|p| path_normalized.contains(p)) {
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
                        let value = metadata.modified().ok().and_then(system_time_to_unix_secs);
                        modified_secs_cache = Some(value);
                        value
                    }
                };

                let secs = match secs_opt {
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

pub(crate) fn entry_matches_without_metadata(
    path: &Path,
    is_dir: bool,
    filters: &PreparedFilters,
) -> bool {
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

        if !filters.path_contains.iter().all(|p| path_normalized.contains(p)) {
            return false;
        }
    }

    true
}

pub(crate) fn entry_matches_with_metadata(
    metadata: &Metadata,
    filters: &PreparedFilters,
) -> bool {
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

pub(crate) fn to_file_entry(path: &Path, metadata: &Metadata) -> FileEntry {
    let name = path
        .file_name()
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
