import { open } from "@tauri-apps/plugin-dialog";
import { onMount } from "svelte";
import {
    createDefaultSearchForm,
    type SearchRunMode,
} from "./page-types";
import {
    dedupePaths,
    displayPath,
    normalizeExtension,
    parseExtensionTokens,
    resolvePreferredDrive,
} from "./form-utils";
import { rowIndentClass } from "./tree-utils";
import { stopDriveCountAnimations } from "./drive-utils";
import { formToFilters } from "./form-mapping";
import { clearSearchResults, isFolderScanning } from "./controller-runtime";
import {
    onIntentFolderFocus,
    runIntentFolderScan,
    runSearch,
    runStopSearch,
    openInExplorer,
} from "./controller-search";
import { attachControllerLifecycle } from "./controller-lifecycle";
import { saveFilterProfile, loadFilterProfile } from "./filter-persistence";
import type {
    SearchControllerState,
    SearchRuntimeDeps,
    SearchTimerControls,
} from "./controller-types";

function createInitialState(): SearchControllerState {
    return {
        searchForm: createDefaultSearchForm(),
        showAdvanced: true,
        enforceFolderScopeValidation: false,
        results: [],
        searching: false,
        searched: false,
        searchStatus: "",
        availableRoots: [],
        openDirectories: {},
        driveScanCounts: {},
        displayedDriveScanCounts: {},
        driveScanOrder: [],
        streamingEnabled: true,
        intentEnabled: false,
        scanningFolders: {},
        intentKnownFolders: {},
        intentScannedFolders: {},
        intentLoadingFolders: {},
        intentEmptyFolders: {},
        intentFocusedFolder: null,
        streamTruncated: false,
        activeRunMode: null,
        activeRunId: 0,
        searchStartedAtMs: null,
        searchElapsedMs: 0,
        lastSearchDurationMs: null,
    };
}

export function createSearchController() {
    const state = $state(createInitialState());
    const streamWorkerRef = { current: null as Worker | null };
    const streamCompletionResolvers = new Map<number, () => void>();
    const driveCountAnimationCancels = new Map<string, () => void>();
    let searchTimerHandle: ReturnType<typeof setInterval> | null = null;

    const runtime: SearchRuntimeDeps = {
        streamWorkerRef,
        streamCompletionResolvers,
        driveCountAnimationCancels,
    };

    const timer: SearchTimerControls = {
        start: () => {
            timer.clear();
            state.searchStartedAtMs = Date.now();
            state.searchElapsedMs = 0;
            state.lastSearchDurationMs = null;
            searchTimerHandle = setInterval(() => {
                if (state.searchStartedAtMs !== null) {
                    state.searchElapsedMs = Date.now() - state.searchStartedAtMs;
                }
            }, 100);
        },
        stop: () => {
            if (state.searchStartedAtMs !== null) {
                state.searchElapsedMs = Date.now() - state.searchStartedAtMs;
                state.lastSearchDurationMs = state.searchElapsedMs;
            }
            state.searchStartedAtMs = null;
            timer.clear();
        },
        clear: () => {
            if (searchTimerHandle !== null) {
                clearInterval(searchTimerHandle);
                searchTimerHandle = null;
            }
        },
    };

    function ensureDriveScopeSelection() {
        const current = state.searchForm.scopeDrive.trim().toUpperCase();
        if (current && current !== "ALL") return;
        state.searchForm.scopeDrive = resolvePreferredDrive(state.availableRoots);
    }

    function normalizeExtensionInput() {
        state.searchForm.extensionInput = parseExtensionTokens(
            state.searchForm.extensionInput,
        ).join(", ");
    }

    function togglePopularExtension(rawValue: string) {
        const extension = normalizeExtension(rawValue);
        if (!extension) return;

        const current = parseExtensionTokens(state.searchForm.extensionInput);
        const next = current.includes(extension)
            ? current.filter((item) => item !== extension)
            : [...current, extension];

        state.searchForm.extensionInput = next.join(", ");
    }

    function removeExtensionToken(valueToRemove: string) {
        const extension = normalizeExtension(valueToRemove);
        if (!extension) return;

        state.searchForm.extensionInput = parseExtensionTokens(
            state.searchForm.extensionInput,
        )
            .filter((item) => item !== extension)
            .join(", ");
    }

    function removeScopeFolder(pathToRemove: string) {
        state.searchForm.scopeFolders = state.searchForm.scopeFolders.filter(
            (path) => path !== pathToRemove,
        );
    }

    function resetSearchForm() {
        state.searchForm = createDefaultSearchForm();
        state.enforceFolderScopeValidation = false;
    }

    async function pickScopeFolders() {
        const selectedDrive = state.searchForm.scopeDrive.trim();
        const selectedFolders = state.searchForm.scopeFolders;
        const defaultPath =
            selectedFolders[0] ||
            (selectedDrive && selectedDrive !== "ALL" ? selectedDrive : undefined);

        try {
            const selected = await open({
                directory: true,
                multiple: true,
                ...(defaultPath ? { defaultPath } : {}),
            });

            if (Array.isArray(selected)) {
                state.searchForm.scopeFolders = dedupePaths([
                    ...selectedFolders,
                    ...selected,
                ]);
                state.searchForm.scopeMode = "folder";
            } else if (typeof selected === "string") {
                state.searchForm.scopeFolders = dedupePaths([
                    ...selectedFolders,
                    selected,
                ]);
                state.searchForm.scopeMode = "folder";
            }
        } catch (error) {
            console.error("Folder selection failed:", error);
        }
    }

    async function saveFilter() {
        await saveFilterProfile({
            getSearchFilters: () => formToFilters(state.searchForm),
            setStatus: (status) => {
                state.searchStatus = status;
            },
            setSearchForm: (form) => {
                state.searchForm = form;
            },
            clearSearchResults: () => clearSearchResults(state, runtime),
        });
    }

    async function loadFilter() {
        await loadFilterProfile({
            getSearchFilters: () => formToFilters(state.searchForm),
            setStatus: (status) => {
                state.searchStatus = status;
            },
            setSearchForm: (form) => {
                state.searchForm = form;
            },
            clearSearchResults: () => clearSearchResults(state, runtime),
        });
    }

    onMount(() => {
        const cleanupLifecycle = attachControllerLifecycle({
            state,
            runtime,
            saveFilter,
            loadFilter,
        });

        return () => {
            cleanupLifecycle();
            stopDriveCountAnimations(driveCountAnimationCancels);
            timer.clear();
        };
    });

    return {
        state,
        displayPath,
        rowIndentClass,
        ensureDriveScopeSelection,
        normalizeExtensionInput,
        togglePopularExtension,
        removeExtensionToken,
        removeScopeFolder,
        resetSearchForm,
        pickScopeFolders,
        isFolderScanning: (path: string) =>
            isFolderScanning(state, path) || !!state.intentLoadingFolders[path],
        toggleDirectory: async (path: string, depth: number) => {
            if (state.intentEnabled) {
                onIntentFolderFocus(state, path);
            }

            const current = state.openDirectories[path];
            const next = current !== undefined ? !current : depth !== 0;
            state.openDirectories = { ...state.openDirectories, [path]: next };

            if (state.intentEnabled && next) {
                await runIntentFolderScan(state, runtime, path);
            }
        },
        isFolderEmpty: (path: string) => !!state.intentEmptyFolders[path],
        search: () => runSearch({ state, runtime, timer }),
        stopSearch: () => runStopSearch({ state, runtime, timer }),
        clearSearchResults: () => clearSearchResults(state, runtime),
        openInExplorer,
    };
}
