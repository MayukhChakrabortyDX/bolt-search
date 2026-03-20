use std::collections::HashSet;

use super::types::{Filter, FilterStage, PreparedFilters};
use super::utils::{
    normalize_path_text, parse_date, parse_date_end_exclusive, parse_size,
};

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

fn build_filter_stage_order(filters: &PreparedFilters) -> Vec<FilterStage> {
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

pub(crate) fn prepare_filters(filters: &[Filter]) -> PreparedFilters {
    let extensions: HashSet<String> = filters
        .iter()
        .filter(|f| f.kind == "extension")
        .flat_map(|f| {
            f.value.as_deref().unwrap_or("").split(',').filter_map(|s| {
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

    let name_contains: Vec<String> = filters
        .iter()
        .filter(|f| f.kind == "name_contains")
        .filter_map(|f| f.value.as_deref())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let path_contains: Vec<String> = filters
        .iter()
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

    let size_gt = filters
        .iter()
        .find(|f| f.kind == "size_gt")
        .and_then(|f| parse_size(f.value.as_deref().unwrap_or(""), f.unit.as_deref().unwrap_or("B")));

    let size_lt = filters
        .iter()
        .find(|f| f.kind == "size_lt")
        .and_then(|f| parse_size(f.value.as_deref().unwrap_or(""), f.unit.as_deref().unwrap_or("B")));

    let modified_after = filters
        .iter()
        .find(|f| f.kind == "modified_after")
        .and_then(|f| f.value.as_deref())
        .and_then(parse_date);
    let modified_before = filters
        .iter()
        .find(|f| f.kind == "modified_before")
        .and_then(|f| f.value.as_deref())
        .and_then(parse_date);
    let created_after = filters
        .iter()
        .find(|f| f.kind == "created_after")
        .and_then(|f| f.value.as_deref())
        .and_then(parse_date);
    let created_before = filters
        .iter()
        .find(|f| f.kind == "created_before")
        .and_then(|f| f.value.as_deref())
        .and_then(parse_date);

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

    let file_only = filters.iter().any(|f| f.kind == "file_only");
    let folder_only = filters.iter().any(|f| f.kind == "folder_only");
    let hidden = filters.iter().any(|f| f.kind == "hidden");
    let readonly = filters.iter().any(|f| f.kind == "readonly");

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
