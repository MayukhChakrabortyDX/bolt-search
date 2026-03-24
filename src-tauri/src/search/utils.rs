use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::NaiveDate;

use super::types::{Filter, PreparedFilters};

pub(crate) fn parse_size(value: &str, unit: &str) -> Option<u64> {
    let n = value.trim().parse::<u64>().ok()?;
    let multiplier = match unit.trim().to_ascii_uppercase().as_str() {
        "KB" => 1024,
        "MB" => 1024u64.pow(2),
        "GB" => 1024u64.pow(3),
        _ => 1,
    };
    n.checked_mul(multiplier)
}

pub(crate) fn parse_date(value: &str) -> Option<i64> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc().timestamp())
}

pub(crate) fn parse_date_end_exclusive(value: &str) -> Option<i64> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .ok()
        .and_then(|d| d.succ_opt().or(Some(d)))
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc().timestamp())
}

pub(crate) fn normalize_path_text(input: &str) -> String {
    let mut normalized = input.trim().replace('\\', "/").to_ascii_lowercase();
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    normalized
}

pub(crate) fn normalize_path_for_match(path: &Path) -> String {
    normalize_path_text(&path.to_string_lossy())
}

pub(crate) fn path_starts_with_component(path: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    if path == prefix {
        return true;
    }

    path.strip_prefix(prefix)
        .map(|rest| rest.starts_with('/'))
        .unwrap_or(false)
}

pub(crate) fn can_descend_into_dir(path: &Path, filters: &PreparedFilters) -> bool {
    let dir_path = normalize_path_for_match(path);

    if filters
        .excluded_path_prefixes
        .iter()
        .any(|excluded| path_starts_with_component(&dir_path, excluded))
    {
        return false;
    }

    let Some(prefix) = filters.path_prefix.as_deref() else {
        return true;
    };

    path_starts_with_component(&dir_path, prefix)
        || path_starts_with_component(prefix, &dir_path)
}

pub(crate) fn system_time_to_unix_secs(time: SystemTime) -> Option<i64> {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(dur) => i64::try_from(dur.as_secs()).ok(),
        Err(err) => i64::try_from(err.duration().as_secs())
            .ok()
            .map(|secs| -secs),
    }
}

pub(crate) fn get_drives() -> Vec<PathBuf> {
    ('A'..='Z')
        .map(|c| PathBuf::from(format!("{}:\\", c)))
        .filter(|p| p.exists())
        .collect()
}

pub(crate) fn roots_from_drive_filters(filters: &[Filter]) -> Vec<PathBuf> {
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::Path;

    use crate::search::types::PreparedFilters;

    use super::{can_descend_into_dir, normalize_path_text};

    fn prepared_with_excluded(excluded: &[&str]) -> PreparedFilters {
        PreparedFilters {
            extensions: HashSet::new(),
            name_contains: Vec::new(),
            path_contains: Vec::new(),
            path_prefix: None,
            excluded_path_prefixes: excluded
                .iter()
                .map(|value| normalize_path_text(value))
                .collect(),
            size_gt: None,
            size_lt: None,
            modified_after: None,
            modified_before: None,
            created_after: None,
            created_before: None,
            file_only: false,
            folder_only: false,
            hidden: false,
            readonly: false,
            stage_order: Vec::new(),
        }
    }

    #[test]
    fn excluded_child_does_not_block_root_descent() {
        let filters = prepared_with_excluded(&["C:/Windows"]);
        assert!(can_descend_into_dir(Path::new("C:/"), &filters));
    }

    #[test]
    fn excluded_path_blocks_descend_inside_prefix() {
        let filters = prepared_with_excluded(&["C:/Windows"]);
        assert!(!can_descend_into_dir(Path::new("C:/Windows"), &filters));
        assert!(!can_descend_into_dir(Path::new("C:/Windows/System32"), &filters));
    }
}
