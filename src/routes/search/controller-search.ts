import { invoke } from "@tauri-apps/api/core";
import {
    MAX_RESULTS,
    createScopedQuery,
    type SearchRunMode,
} from "./page-types";
import { analyzeSearchForm, dedupePaths } from "./form-utils";
import { formToFilters } from "./form-mapping";
import { initializeDriveScanSlots } from "./drive-utils";
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
