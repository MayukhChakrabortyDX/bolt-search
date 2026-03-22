import { invoke } from "@tauri-apps/api/core";
import {
    MAX_RESULTS,
    createScopedQuery,
    type SearchRunMode,
} from "./page-types";
import { analyzeSearchForm, dedupePaths, normalizePath } from "./form-utils";
import { formToFilters } from "./form-mapping";
import { incrementDriveScanned, initializeDriveScanSlots } from "./drive-utils";
import { scanRootsBatchWithProgress, scanRootsStreaming } from "./runtime";
import type {
    SearchControllerState,
    SearchRuntimeDeps,
    SearchTimerControls,
} from "./controller-types";
import {
    applyCompleted,
    applyStreamingProgress,
} from "./controller-runtime";

type SearchRunnerDeps = {
    state: SearchControllerState;
    runtime: SearchRuntimeDeps;
    timer: SearchTimerControls;
};

type FolderBatchResult = {
    entries: SearchControllerState["results"];
    next_folders: string[];
    scanned_folders: number;
};

function markKnownFolders(
    state: SearchControllerState,
    folders: string[],
): void {
    if (folders.length === 0) return;

    const next = { ...state.intentKnownFolders };
    for (const folder of folders) {
        const normalized = normalizePath(folder);
        if (!normalized) continue;
        next[normalized] = true;
    }
    state.intentKnownFolders = next;
}

function markScannedFolder(state: SearchControllerState, folder: string): void {
    const normalized = normalizePath(folder);
    if (!normalized) return;

    state.intentScannedFolders = {
        ...state.intentScannedFolders,
        [normalized]: true,
    };
}

function setFolderEmptyState(
    state: SearchControllerState,
    folder: string,
    empty: boolean,
): void {
    const normalized = normalizePath(folder);
    if (!normalized) return;

    if (empty) {
        state.intentEmptyFolders = {
            ...state.intentEmptyFolders,
            [normalized]: true,
        };
        return;
    }

    if (!state.intentEmptyFolders[normalized]) return;
    const next = { ...state.intentEmptyFolders };
    delete next[normalized];
    state.intentEmptyFolders = next;
}

export function onIntentFolderFocus(
    state: SearchControllerState,
    focusedFolderPath: string,
): void {
    if (!state.intentEnabled) return;

    const focused = normalizePath(focusedFolderPath);
    if (!focused) return;

    const previousFocused = state.intentFocusedFolder;
    state.intentFocusedFolder = focused;

    if (!previousFocused || previousFocused === focused) {
        return;
    }

    if (!state.intentEmptyFolders[previousFocused]) {
        return;
    }

    if (state.intentKnownFolders[previousFocused]) {
        const nextKnown = { ...state.intentKnownFolders };
        delete nextKnown[previousFocused];
        state.intentKnownFolders = nextKnown;
    }

    if (state.openDirectories[previousFocused] !== undefined) {
        const nextOpen = { ...state.openDirectories };
        delete nextOpen[previousFocused];
        state.openDirectories = nextOpen;
    }

    if (state.intentScannedFolders[previousFocused]) {
        const nextScanned = { ...state.intentScannedFolders };
        delete nextScanned[previousFocused];
        state.intentScannedFolders = nextScanned;
    }

    if (state.intentLoadingFolders[previousFocused]) {
        const nextLoading = { ...state.intentLoadingFolders };
        delete nextLoading[previousFocused];
        state.intentLoadingFolders = nextLoading;
    }

    const nextEmpty = { ...state.intentEmptyFolders };
    delete nextEmpty[previousFocused];
    state.intentEmptyFolders = nextEmpty;
}

function appendIntentResults(
    state: SearchControllerState,
    entries: SearchControllerState["results"],
): void {
    if (entries.length === 0 || state.results.length >= MAX_RESULTS) return;
    const remaining = MAX_RESULTS - state.results.length;
    state.results.push(...entries.slice(0, remaining));
}

export async function runSearch({
    state,
    runtime,
    timer,
}: SearchRunnerDeps): Promise<void> {
    if (state.searching) return;

    state.enforceFolderScopeValidation = true;
    const submitIssues = analyzeSearchForm(state.searchForm, {
        enforceFolderScopeSelection: true,
    });
    if (submitIssues.length > 0) {
        state.searchStatus = submitIssues[0].message;
        return;
    }

    timer.start();
    const runId = state.activeRunId + 1;
    state.activeRunId = runId;
    state.searching = true;
    state.searched = true;
    state.searchStatus = "Preparing roots...";
    state.results = [];
    state.openDirectories = {};
    state.driveScanCounts = {};
    state.displayedDriveScanCounts = {};
    state.driveScanOrder = [];
    state.scanningFolders = {};
    state.intentKnownFolders = {};
    state.intentScannedFolders = {};
    state.intentLoadingFolders = {};
    state.intentEmptyFolders = {};
    state.intentFocusedFolder = null;
    state.focusedFolderPath = null;
    state.streamTruncated = false;

    try {
        const roots =
            state.availableRoots.length > 0
                ? state.availableRoots
                : await invoke<string[]>("list_search_roots");

        if (runId !== state.activeRunId) {
            return;
        }

        const selectedDrive =
            state.searchForm.scopeMode === "drive"
                ? state.searchForm.scopeDrive.trim()
                : "ALL";
        const selectedSubfolders =
            state.searchForm.scopeMode === "folder"
                ? dedupePaths(state.searchForm.scopeFolders)
                : [];

        const rootsToScan =
            selectedSubfolders.length > 0
                ? selectedSubfolders
                : selectedDrive && selectedDrive !== "ALL"
                  ? roots.filter((root) => root === selectedDrive)
                  : roots;

        if (rootsToScan.length === 0) {
            state.searchStatus = "No roots found";
            return;
        }

        const slots = initializeDriveScanSlots(rootsToScan);
        state.driveScanOrder = slots.driveScanOrder;
        state.driveScanCounts = slots.driveScanCounts;
        state.displayedDriveScanCounts = slots.displayedDriveScanCounts;

        if (state.intentEnabled) {
            const scopedQuery = createScopedQuery(formToFilters(state.searchForm));
            const batch = await invoke<FolderBatchResult>("search_folder_batch", {
                query: scopedQuery,
                folders: rootsToScan,
                limit: MAX_RESULTS,
                threadLimit: 6,
            });

            if (runId !== state.activeRunId) {
                return;
            }

            markKnownFolders(state, rootsToScan);
            markKnownFolders(state, batch.next_folders);
            for (const root of rootsToScan) {
                markScannedFolder(state, root);
                state.driveScanOrder = incrementDriveScanned(
                    root,
                    1,
                    state.driveScanOrder,
                    state.driveScanCounts,
                    state.displayedDriveScanCounts,
                    runtime.driveCountAnimationCancels,
                );
            }

            state.results = [];
            appendIntentResults(state, batch.entries);
            for (const root of rootsToScan) {
                setFolderEmptyState(state, root, false);
            }
            state.searchStatus = `Intent ready (${state.results.length} result${state.results.length === 1 ? "" : "s"}). Expand folders to continue.`;
            return;
        }

        const runMode: Exclude<SearchRunMode, null> = state.streamingEnabled
            ? "streaming"
            : "batch";
        state.activeRunMode = runMode;

        const scopedQuery = createScopedQuery(formToFilters(state.searchForm));
        const context = {
            getActiveRunId: () => state.activeRunId,
            getActiveRunMode: () => state.activeRunMode,
            getStreamWorker: () => runtime.streamWorkerRef.current,
            streamCompletionResolvers: runtime.streamCompletionResolvers,
            applyStreamingProgress: (payload: Parameters<typeof applyStreamingProgress>[2], allowEntries: boolean) =>
                applyStreamingProgress(state, runtime, payload, allowEntries),
            applyCompleted: (payload: Parameters<typeof applyCompleted>[3]) =>
                applyCompleted(state, runtime, runId, payload),
            maxResults: MAX_RESULTS,
        };

        if (runMode === "streaming") {
            await scanRootsStreaming(rootsToScan, scopedQuery, runId, context);
        } else {
            state.searchStatus = "Batch mode: scanning...";
            const fullResults = await scanRootsBatchWithProgress(
                rootsToScan,
                scopedQuery,
                runId,
                context,
            );

            if (runId !== state.activeRunId) {
                return;
            }

            state.results = fullResults.slice(0, MAX_RESULTS);
            state.searchStatus = `Done (${state.results.length} result${state.results.length === 1 ? "" : "s"})`;
        }

        if (
            runId === state.activeRunId &&
            runMode === "streaming" &&
            !state.searchStatus.startsWith("Done")
        ) {
            state.searchStatus = `Done (${state.results.length} result${state.results.length === 1 ? "" : "s"}${state.streamTruncated ? ", max cap reached" : ""})`;
        }
    } catch (error) {
        if (runId !== state.activeRunId) {
            return;
        }

        console.error("Search failed:", error);
        state.results = [];
        state.scanningFolders = {};
        state.intentLoadingFolders = {};
        state.intentEmptyFolders = {};
        state.intentFocusedFolder = null;
        state.focusedFolderPath = null;
        state.searchStatus = "Search failed";
    } finally {
        if (runId === state.activeRunId) {
            timer.stop();
            state.searching = false;
            state.activeRunMode = null;
        }
    }
}

export async function runStopSearch({
    state,
    runtime,
    timer,
}: SearchRunnerDeps): Promise<void> {
    if (!state.searching) return;

    const runIdToStop = state.activeRunId;
    state.activeRunId = runIdToStop + 1;
    state.activeRunMode = null;
    timer.stop();
    state.searching = false;
    state.scanningFolders = {};
    state.intentLoadingFolders = {};
    state.intentFocusedFolder = null;
    state.focusedFolderPath = null;
    state.streamTruncated = false;
    state.searchStatus = "Stopping search...";

    const resolve = runtime.streamCompletionResolvers.get(runIdToStop);
    if (resolve) {
        resolve();
        runtime.streamCompletionResolvers.delete(runIdToStop);
    }

    if (runtime.streamWorkerRef.current) {
        runtime.streamWorkerRef.current.postMessage({
            type: "reset",
            runId: runIdToStop,
        });
    }

    if (runIdToStop <= 0) {
        state.searchStatus = "Search stopped";
        return;
    }

    try {
        await invoke("cancel_search", { runId: runIdToStop });
        state.searchStatus = "Search stopped";
    } catch (error) {
        console.error("Stop search failed:", error);
        state.searchStatus = "Stop request failed";
    }
}

export async function openInExplorer(path: string): Promise<void> {
    await invoke("open_in_explorer", { path });
}

export async function runIntentFolderScan(
    state: SearchControllerState,
    runtime: SearchRuntimeDeps,
    folderPath: string,
): Promise<void> {
    if (!state.intentEnabled) return;
    const normalizedFolder = normalizePath(folderPath);
    if (!normalizedFolder) return;
    if (state.intentScannedFolders[normalizedFolder]) return;
    if (state.intentLoadingFolders[normalizedFolder]) return;

    const runId = state.activeRunId;
    const remaining = MAX_RESULTS - state.results.length;
    if (remaining <= 0) {
        state.searchStatus = "Done (max cap reached)";
        return;
    }

    state.intentLoadingFolders = {
        ...state.intentLoadingFolders,
        [normalizedFolder]: true,
    };
    state.searchStatus = "Intent: scanning selected folder...";

    try {
        const scopedQuery = createScopedQuery(formToFilters(state.searchForm));
        const batch = await invoke<FolderBatchResult>("search_folder_batch", {
            query: scopedQuery,
            folders: [normalizedFolder],
            limit: remaining,
            threadLimit: 6,
        });

        if (runId !== state.activeRunId) {
            return;
        }

        appendIntentResults(state, batch.entries);
        markKnownFolders(state, batch.next_folders);
        markScannedFolder(state, normalizedFolder);
        setFolderEmptyState(
            state,
            normalizedFolder,
            batch.entries.length === 0 && batch.next_folders.length === 0,
        );
        state.driveScanOrder = incrementDriveScanned(
            normalizedFolder,
            batch.scanned_folders,
            state.driveScanOrder,
            state.driveScanCounts,
            state.displayedDriveScanCounts,
            runtime.driveCountAnimationCancels,
        );

        const reachedCap = state.results.length >= MAX_RESULTS;
        state.searchStatus = reachedCap
            ? `Intent ready (${state.results.length} results, max cap reached).`
            : `Intent ready (${state.results.length} result${state.results.length === 1 ? "" : "s"}). Expand folders to continue.`;
    } catch (error) {
        if (runId !== state.activeRunId) {
            return;
        }
        console.error("Intent folder scan failed:", error);
        state.searchStatus = "Intent scan failed";
    } finally {
        if (runId === state.activeRunId) {
            if (state.intentLoadingFolders[normalizedFolder]) {
                const nextLoading = { ...state.intentLoadingFolders };
                delete nextLoading[normalizedFolder];
                state.intentLoadingFolders = nextLoading;
            }
        }
    }
}
