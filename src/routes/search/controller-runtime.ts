import {
    MAX_RESULTS,
    type FileEntry,
    type StreamingCompletedEvent,
    type StreamingProgressEvent,
} from "./page-types";
import { normalizePath } from "./form-utils";
import {
    driveLabelFromPath,
    incrementDriveScanned,
    stopDriveCountAnimations,
} from "./drive-utils";
import type { SearchControllerState, SearchRuntimeDeps } from "./controller-types";

function markFolderScanning(
    state: SearchControllerState,
    folderPath: string,
    active: boolean,
): void {
    const normalized = normalizePath(folderPath);
    if (!normalized) return;

    if (active) {
        state.scanningFolders[normalized] = true;
        return;
    }

    if (!state.scanningFolders[normalized]) {
        return;
    }

    delete state.scanningFolders[normalized];
}

function appendResults(state: SearchControllerState, chunk: FileEntry[]) {
    if (chunk.length === 0 || state.results.length >= MAX_RESULTS) return;
    const remaining = MAX_RESULTS - state.results.length;
    state.results.push(...chunk.slice(0, remaining));
}

export function applyStreamingProgress(
    state: SearchControllerState,
    deps: SearchRuntimeDeps,
    payload: StreamingProgressEvent["data"],
    allowEntries: boolean,
): void {
    if (allowEntries) {
        for (const folder of payload.startedFolders) {
            markFolderScanning(state, folder, true);
        }

        for (const folder of payload.finishedFolders) {
            markFolderScanning(state, folder, false);
        }
    } else if (Object.keys(state.scanningFolders).length > 0) {
        state.scanningFolders = {};
    }

    if (allowEntries) {
        appendResults(state, payload.entries);
    }

    const finishedByDrive = new Map<string, number>();
    for (const folder of payload.finishedFolders) {
        const drive = driveLabelFromPath(folder);
        finishedByDrive.set(drive, (finishedByDrive.get(drive) ?? 0) + 1);
    }

    for (const [drive, count] of finishedByDrive) {
        state.driveScanOrder = incrementDriveScanned(
            drive,
            count,
            state.driveScanOrder,
            state.driveScanCounts,
            state.displayedDriveScanCounts,
            deps.driveCountAnimationCancels,
        );
    }

    const modeLabel = state.activeRunMode === "batch" ? "Batch" : "Streaming";
    state.searchStatus = `${modeLabel}: scanned ${payload.scannedFolders} folders, ${payload.totalResults} result${payload.totalResults === 1 ? "" : "s"}`;
}

export function applyCompleted(
    state: SearchControllerState,
    deps: SearchRuntimeDeps,
    runId: number,
    payload: StreamingCompletedEvent["data"],
): void {
    state.streamTruncated = payload.truncated;
    state.scanningFolders = {};
    state.searchStatus = `Done (${payload.totalResults} result${payload.totalResults === 1 ? "" : "s"}${payload.truncated ? ", max cap reached" : ""})`;

    const resolve = deps.streamCompletionResolvers.get(runId);
    if (resolve) {
        resolve();
        deps.streamCompletionResolvers.delete(runId);
    }
}

export function clearSearchResults(
    state: SearchControllerState,
    deps: SearchRuntimeDeps,
): void {
    if (state.searching) return;

    state.results = [];
    state.searched = false;
    state.searchStatus = "";
    state.openDirectories = {};
    state.driveScanCounts = {};
    state.displayedDriveScanCounts = {};
    state.driveScanOrder = [];
    stopDriveCountAnimations(deps.driveCountAnimationCancels);
    state.scanningFolders = {};
    state.intentKnownFolders = {};
    state.intentScannedFolders = {};
    state.intentLoadingFolders = {};
    state.intentEmptyFolders = {};
    state.intentFocusedFolder = null;
    state.streamTruncated = false;
    state.activeRunMode = null;
    deps.streamCompletionResolvers.clear();
}

export function isFolderScanning(
    state: SearchControllerState,
    folderPath: string,
): boolean {
    return !!state.scanningFolders[folderPath];
}
