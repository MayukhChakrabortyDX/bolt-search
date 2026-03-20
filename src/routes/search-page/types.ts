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

export type TreeNode = {
    name: string;
    path: string;
    isDir: boolean;
    children: TreeNode[];
};

export type MutableTreeNode = {
    name: string;
    path: string;
    isDir: boolean;
    children: Map<string, MutableTreeNode>;
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

export type SearchScopeMode = "all" | "drive" | "folder";
export type SearchKind = "any" | "file" | "folder";

export type SearchFormState = {
    query: string;
    extensionInput: string;
    pathContainsInput: string;
    pathPrefix: string;
    scopeMode: SearchScopeMode;
    scopeDrive: string;
    scopeFolders: string[];
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
