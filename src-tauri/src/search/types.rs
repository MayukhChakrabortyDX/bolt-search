use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct SearchQuery {
    pub(crate) filters: Vec<Filter>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct Filter {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) value: Option<String>,
    pub(crate) value2: Option<String>,
    pub(crate) unit: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub(crate) struct FileEntry {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) is_dir: bool,
    pub(crate) size: u64,
    pub(crate) modified: String,
}

#[derive(Serialize, Debug)]
pub(crate) struct FolderBatchResult {
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) next_folders: Vec<String>,
    pub(crate) scanned_folders: usize,
}

#[derive(Debug)]
pub(crate) struct FolderScanResult {
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) next_folders: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum FilterStage {
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
pub(crate) struct PreparedFilters {
    pub(crate) extensions: HashSet<String>,
    pub(crate) name_contains: Vec<String>,
    pub(crate) path_contains: Vec<String>,
    pub(crate) path_prefix: Option<String>,
    pub(crate) size_gt: Option<u64>,
    pub(crate) size_lt: Option<u64>,
    pub(crate) modified_after: Option<i64>,
    pub(crate) modified_before: Option<i64>,
    pub(crate) created_after: Option<i64>,
    pub(crate) created_before: Option<i64>,
    pub(crate) file_only: bool,
    pub(crate) folder_only: bool,
    pub(crate) hidden: bool,
    pub(crate) readonly: bool,
    pub(crate) stage_order: Vec<FilterStage>,
}
