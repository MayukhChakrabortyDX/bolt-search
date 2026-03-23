import type {
    ExplorerLayoutMode,
    FileEntry,
    SearchFormState,
    SearchRunMode,
} from "./page-types";

export type SearchControllerState = {
    searchForm: SearchFormState;
    showAdvanced: boolean;
    enforceFolderScopeValidation: boolean;
    results: FileEntry[];
    searching: boolean;
    searched: boolean;
    searchStatus: string;
    availableRoots: string[];
    openDirectories: Record<string, boolean>;
    driveScanCounts: Record<string, number>;
    displayedDriveScanCounts: Record<string, number>;
    driveScanOrder: string[];
    streamingEnabled: boolean;
    intentEnabled: boolean;
    layoutMode: ExplorerLayoutMode;
    scanningFolders: Record<string, boolean>;
    intentKnownFolders: Record<string, boolean>;
    intentScannedFolders: Record<string, boolean>;
    intentLoadingFolders: Record<string, boolean>;
    intentEmptyFolders: Record<string, boolean>;
    intentFocusedFolder: string | null;
    focusedFolderPath: string | null;
    streamTruncated: boolean;
    activeRunMode: SearchRunMode;
    activeRunId: number;
    searchStartedAtMs: number | null;
    searchElapsedMs: number;
    lastSearchDurationMs: number | null;
};

export type SearchTimerControls = {
    start: () => void;
    stop: () => void;
    clear: () => void;
};

export type SearchRuntimeDeps = {
    streamWorkerRef: { current: Worker | null };
    streamCompletionResolvers: Map<number, () => void>;
    driveCountAnimationCancels: Map<string, () => void>;
};
