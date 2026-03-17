# Bolt Search - Agent Documentation

## Project Overview

Bolt Search is a Windows file discovery app built with Tauri 2 + Svelte 5 + Rust.
Users build filter pipelines in the left sidebar and receive progressive results in a tree view on the right.

The app now supports:
- Drive-scoped search (single drive) or global search (all drives)
- Subfolder-scoped search via native folder picker
- Two-phase progressive scanning
- Bounded-thread folder batch traversal
- IntelliJ-style hierarchical results rendering
- Custom frameless title bar with window controls

---

## Project Structure

```
bolt-search/
├── src/
│   └── routes/
│       └── +page.svelte        <- UI state, filters, contradictions, progressive orchestration, tree view
├── src-tauri/
│   ├── Cargo.toml              <- Rust dependencies (includes rayon)
│   └── src/
│       ├── main.rs             <- Thin entry point, calls lib::run()
│       └── lib.rs              <- Search/filter engine + Tauri commands
```

---

## Frontend <-> Backend Contract

### Shared Types

Rust receives:

```rust
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
```

Rust returns entries:

```rust
#[derive(Serialize, Debug)]
struct FileEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    modified: String,
}
```

TypeScript consumes:

```ts
type FileEntry = {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: string;
};
```

### Commands

1. `search(query)`
- Purpose: full search entry point (still available)
- Returns: `Result<Vec<FileEntry>, String>`
- Scope behavior: honors `drive` filter via backend root selection

2. `list_search_roots()`
- Purpose: enumerate available drive roots (A:\ through Z:\ that exist)
- Returns: `Vec<String>`

3. `search_in_root(query, root, limit)`
- Purpose: scan a specific root with recursive traversal (legacy progressive command)
- Returns: `Result<Vec<FileEntry>, String>`

4. `search_folder_batch(query, folders, limit, thread_limit)`
- Purpose: non-recursive batch scan of folder list (core of two-phase loader)
- Returns:

```rust
#[derive(Serialize, Debug)]
struct FolderBatchResult {
    entries: Vec<FileEntry>,
    next_folders: Vec<String>,
    scanned_folders: usize,
}
```

5. `open_in_explorer(path)`
- Purpose: open Windows Explorer and select item
- Returns: `Result<(), String>`

6. `list_subfolders(root)`
- Purpose: enumerate first-level folders under a root (legacy helper for subfolder filter evolution)
- Returns: `Result<Vec<String>, String>`

---

## Filter Types and Semantics

Supported filter types:
- `extension`
- `name_contains`
- `path_contains`
- `subfolder`
- `size_gt`
- `size_lt`
- `modified_after`
- `modified_before`
- `created_after`
- `created_before`
- `drive`
- `hidden`
- `readonly`
- `file_only`
- `folder_only`

Rules:
- All filters are AND-combined.
- Stackable filters are only `extension` and `path_contains`.
- `drive` filter supports:
  - `ALL` => global scan across all detected drives
  - specific root (for example `C:\`) => scoped scan
- `subfolder` filter supports:
    - user-picked absolute directory path from native folder chooser
    - when present, it overrides drive scope and becomes the root scan target

---

## Contradiction Analysis (Frontend)

`analyzeContradictions(filters)` blocks search if contradiction exists.

Current rules:
- Any non-stackable filter repeated more than once
- `size_gt >= size_lt`
- `modified_after >= modified_before`
- `created_after >= created_before`
- `file_only` and `folder_only` both present

---

## Two-Phase Progressive Loading

Current progressive algorithm in `+page.svelte`:

1. Resolve roots (`list_search_roots`) and apply `drive` scope.
2. For each selected root:
   - Phase 1: run `search_folder_batch` with `[root]` (non-recursive) and render immediately.
   - Collect `next_folders` into queue.
   - Phase 2: repeatedly process queued folders in batches (size controlled by frontend constant), still non-recursive per call.
3. Continue until queue exhausted or max result cap reached.

Frontend tuning constants:
- `MAX_RESULTS = 10_000`
- `SEARCH_THREAD_LIMIT = 6`
- `FOLDER_BATCH_SIZE = 24`

---

## Threading Model

Backend `search_folder_batch` uses a bounded Rayon thread pool:
- `thread_limit` is clamped to `1..=16`
- each folder in current batch is scanned in parallel
- each folder scan is non-recursive (`std::fs::read_dir` one level only)

This prevents unbounded thread growth while keeping phase-2 traversal responsive.

---

## UI Rendering Model

Results panel is now tree-based, not card-grid based.

Implemented in `+page.svelte`:
- Build tree from `results` paths
- Expand/collapse directory nodes
- Full path displayed for every row
- File row click still triggers `open_in_explorer`
- Icons migrated to Lucide for actions/tree affordances

Window chrome is custom:
- Frameless Tauri window (`decorations: false`)
- Draggable top bar with native-like minimize/maximize/close buttons
- App starts maximized by default

Behavior matches IntelliJ-like hierarchical navigation expectations.

---

## Safety and Robustness Improvements

Implemented backend hardening:
- `parse_size` now uses `checked_mul` to avoid overflow panics
- date parsing/comparison uses signed unix seconds (`i64`)
- command-level panic guards via `catch_unwind`
- `open_in_explorer` now returns `Result` instead of silently swallowing errors
- `open_in_explorer` now passes args safely (`/select,` + path) and canonicalizes paths to avoid special-character misrouting
- shared filter matching logic extracted (`prepare_filters`, `entry_matches`) for consistency

---

## Tauri Commands Registered

Current invoke handler in `lib.rs`:

```rust
.invoke_handler(tauri::generate_handler![
    search,
    list_search_roots,
    list_subfolders,
    search_in_root,
    search_folder_batch,
    open_in_explorer
])
```

---

## Dependencies

Current relevant Rust dependencies:

```toml
[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
tauri-plugin-dialog = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
walkdir = "2"
chrono = { version = "0.4", features = ["serde"] }
rayon = "1.10"
```

---

## Known Issue

Observed during dev runs:
- app compiles but `bolt-search.exe` exits immediately with code `0xcfffffff` before normal runtime interaction
- issue appears startup/runtime related, not search-algorithm related

---

## Session Change Log (2026-03-18)

This session implemented all of the following:

1. Reproduced startup crash and confirmed it happens before command usage.
2. Hardened backend filtering and parsing paths to reduce panic risk.
3. Added progressive root commands (`list_search_roots`, `search_in_root`).
4. Added `drive` filter end-to-end (UI + backend semantics).
5. Reworked loading into two phases (root-first, then queued subfolders).
6. Added bounded thread pool parallelism for folder batches via Rayon.
7. Refactored backend filtering into shared helpers (`prepare_filters`, `entry_matches`).
8. Replaced right-side result grid with IntelliJ-style tree view including full paths.
9. Switched to a custom frameless title bar and modernized UI icons with Lucide.
10. Fixed intermittent Explorer misrouting by hardening path handling for special characters.
11. Added subfolder scope to filtering and updated search root resolution accordingly.
12. Converted subfolder filter to a native folder selector via Tauri Dialog plugin.
13. Fixed reactive filter-state loop that could break "Add Filter" interactions.