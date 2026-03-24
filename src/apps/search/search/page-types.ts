import type { Filter } from "../filter.svelte";

export type FileEntry = {
    name: string;
    path: string;
    is_dir: boolean;
    size: number;
    modified: string;
};

export type StreamingProgressEvent = {
    event: "progress";
    data: {
        startedFolders: string[];
        finishedFolders: string[];
        entries: FileEntry[];
        scannedFolders: number;
        totalResults: number;
    };
};

export type StreamingCompletedEvent = {
    event: "completed";
    data: {
        scannedFolders: number;
        totalResults: number;
        truncated: boolean;
    };
};

export type StreamingSearchEvent =
    | StreamingProgressEvent
    | StreamingCompletedEvent;

export type StreamWorkerInput =
    | { type: "configure"; payload: { debounceMs: number } }
    | { type: "reset"; runId: number }
    | { type: "flush"; runId: number }
    | {
          type: "progress";
          runId: number;
          payload: StreamingProgressEvent["data"];
      }
    | {
          type: "completed";
          runId: number;
          payload: StreamingCompletedEvent["data"];
      };

export type StreamWorkerOutput =
    | {
          type: "batched-progress";
          runId: number;
          payload: StreamingProgressEvent["data"];
      }
    | {
          type: "completed";
          runId: number;
          payload: StreamingCompletedEvent["data"];
      };

export type ScopedQuery = {
    filters: Array<{
        type: string;
        value?: string;
        value2?: string;
        unit?: string;
    }>;
};

export type SearchRunMode = "streaming" | "batch" | null;
export type ExplorerLayoutMode = "default" | "focus" | "group";

export type TreeNode = {
    name: string;
    path: string;
    isDir: boolean;
    children: TreeNode[];
};

export type TreeRow = {
    node: TreeNode;
    depth: number;
    hasChildren: boolean;
    isOpen: boolean;
};

export type DriveScanRow = {
    label: string;
    scanned: number;
    active: boolean;
};

export type GroupedResultBucket = {
    key: string;
    label: string;
    entries: FileEntry[];
};

export type SearchScopeMode = "all" | "drive" | "folder";
export type SearchKind = "file" | "folder";

export type SearchFormState = {
    query: string;
    extensionInput: string;
    pathContainsInput: string;
    pathPrefix: string;
    scopeMode: SearchScopeMode;
    scopeDrive: string;
    scopeFolders: string[];
    excludedFolders: string[];
    kind: SearchKind;
    includeHidden: boolean;
    readonlyOnly: boolean;
    sizeMin: string;
    sizeMinUnit: string;
    sizeMax: string;
    sizeMaxUnit: string;
    modifiedFrom: string;
    modifiedTo: string;
    createdFrom: string;
    createdTo: string;
};

export type ValidationIssue = { message: string };

export const MAX_RESULTS = 10_000;
export const WORKER_UI_DEBOUNCE_MS = 50;

export const SIZE_UNIT_OPTIONS = [
    { value: "B", label: "B" },
    { value: "KB", label: "KB" },
    { value: "MB", label: "MB" },
    { value: "GB", label: "GB" },
] as const;

export const POPULAR_EXTENSION_OPTIONS = [
    { value: ".pdf", label: "PDF" },
    { value: ".docx", label: "DOCX" },
    { value: ".xlsx", label: "XLSX" },
    { value: ".pptx", label: "PPTX" },
    { value: ".txt", label: "TXT" },
    { value: ".zip", label: "ZIP" },
    { value: ".jpg", label: "JPG" },
    { value: ".png", label: "PNG" },
    { value: ".mp4", label: "MP4" },
    { value: ".mp3", label: "MP3" },
    { value: ".js", label: "JS" },
    { value: ".ts", label: "TS" },
] as const;

export function createDefaultSearchForm(): SearchFormState {
    return {
        query: "",
        extensionInput: "",
        pathContainsInput: "",
        pathPrefix: "",
        scopeMode: "all",
        scopeDrive: "ALL",
        scopeFolders: [],
        excludedFolders: [],
        kind: "file",
        includeHidden: false,
        readonlyOnly: false,
        sizeMin: "",
        sizeMinUnit: "MB",
        sizeMax: "",
        sizeMaxUnit: "MB",
        modifiedFrom: "",
        modifiedTo: "",
        createdFrom: "",
        createdTo: "",
    };
}

export function createScopedQuery(filters: Filter[]): ScopedQuery {
    return {
        filters: filters.filter(
            (filter) => filter.type !== "drive" && filter.type !== "subfolder",
        ),
    };
}
