<script lang="ts">
    import { Channel, invoke } from "@tauri-apps/api/core";
    import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
    import { onMount } from "svelte";
    import { FilterModel } from "./filter.svelte";
    import SearchResultsPanel from "./components/SearchResultsPanel.svelte";
    import SearchSidebar from "./components/SearchSidebar.svelte";
    import { MAX_RESULTS, WORKER_UI_DEBOUNCE_MS } from "./search-page/constants";
    import {
        analyzeSearchForm,
        createDefaultSearchForm,
        dedupePaths,
        formFromFilters,
        formToFilters,
        resolvePreferredDrive,
    } from "./search-page/form-utils";
    import {
        buildResultTree,
        displayPath,
        driveLabelFromPath,
        flattenVisibleRows,
        normalizePath,
        rowIndentClass,
    } from "./search-page/tree-utils";
    import type {
        DriveScanRow,
        FileEntry,
        ScopedQuery,
        SearchFormState,
        SearchRunMode,
        StreamWorkerInput,
        StreamWorkerOutput,
        StreamingProgressEvent,
        StreamingSearchEvent,
    } from "./search-page/types";

    function ensureDriveScopeSelection() {
        const current = searchForm.scopeDrive.trim().toUpperCase();
        if (current && current !== "ALL") {
            return;
        }

        searchForm.scopeDrive = resolvePreferredDrive(availableRoots);
    }

    let searchForm = $state<SearchFormState>(createDefaultSearchForm());
    let showAdvanced = $state(true);
    let enforceFolderScopeValidation = $state(false);
    let results = $state<FileEntry[]>([]);
    let searching = $state(false);
    let searched = $state(false);
    let searchStatus = $state("");
    let availableRoots = $state<string[]>([]);
    let openDirectories = $state<Record<string, boolean>>({});
    let driveScanCounts = $state<Record<string, number>>({});
    let displayedDriveScanCounts = $state<Record<string, number>>({});
    let driveScanOrder = $state<string[]>([]);
    let streamingEnabled = $state(true);
    let scanningFolders = $state<Record<string, boolean>>({});
    let streamTruncated = $state(false);
    let activeRunMode = $state<SearchRunMode>(null);
    let activeRunId = $state(0);
    let searchStartedAtMs = $state<number | null>(null);
    let searchElapsedMs = $state(0);
    let lastSearchDurationMs = $state<number | null>(null);
    let streamWorker: Worker | null = null;
    let searchTimerHandle: ReturnType<typeof setInterval> | null = null;
    const streamCompletionResolvers = new Map<number, () => void>();
    const driveCountAnimationCancels = new Map<string, () => void>();

    const searchFilters = $derived(formToFilters(searchForm));
    const query = $derived(FilterModel.toQuery(searchFilters));
    const driveOptions = $derived([
        { value: "ALL", label: "Global (all drives)" },
        ...availableRoots.map((root) => ({ value: root, label: root })),
    ]);

    const validationIssues = $derived(
        analyzeSearchForm(searchForm, {
            enforceFolderScopeSelection: enforceFolderScopeValidation,
        }),
    );
    const hasContradiction = $derived(validationIssues.length > 0);

    const activeScanningFolders = $derived(
        activeRunMode === "streaming"
            ? Object.keys(scanningFolders).filter(
                  (path) => scanningFolders[path],
              )
            : [],
    );
    const resultTree = $derived(
        buildResultTree(results, activeScanningFolders),
    );
    const treeRows = $derived(flattenVisibleRows(resultTree, isDirectoryOpen));
    const driveScanTotal = $derived(
        Object.values(displayedDriveScanCounts).reduce(
            (sum, value) => sum + value,
            0,
        ),
    );
    const driveScanRows = $derived.by(() => {
        const labels = [...driveScanOrder.slice(0, 4)];
        while (labels.length < 4) {
            labels.push("");
        }

        return labels.map((label, index): DriveScanRow => {
            const active = label.length > 0;
            const scanned = active ? (displayedDriveScanCounts[label] ?? 0) : 0;

            return {
                label: active ? label : `Drive ${index + 1}`,
                scanned,
                active,
            };
        });
    });

    const searchDurationLabel = $derived.by(() => {
        if (searching) {
            return formatDuration(searchElapsedMs);
        }
        if (lastSearchDurationMs !== null) {
            return formatDuration(lastSearchDurationMs);
        }
        return "";
    });

    function clearSearchTimer() {
        if (searchTimerHandle !== null) {
            clearInterval(searchTimerHandle);
            searchTimerHandle = null;
        }
    }

    function startSearchTimer() {
        clearSearchTimer();
        searchStartedAtMs = Date.now();
        searchElapsedMs = 0;
        lastSearchDurationMs = null;

        searchTimerHandle = setInterval(() => {
            if (searchStartedAtMs !== null) {
                searchElapsedMs = Date.now() - searchStartedAtMs;
            }
        }, 100);
    }

    function stopSearchTimer() {
        if (searchStartedAtMs !== null) {
            searchElapsedMs = Date.now() - searchStartedAtMs;
            lastSearchDurationMs = searchElapsedMs;
        }

        searchStartedAtMs = null;
        clearSearchTimer();
    }

    function formatDuration(ms: number): string {
        if (ms < 1000) {
            return `${ms} ms`;
        }

        const totalSeconds = ms / 1000;
        if (totalSeconds < 60) {
            return `${totalSeconds.toFixed(1)} s`;
        }

        const minutes = Math.floor(totalSeconds / 60);
        const seconds = (totalSeconds % 60).toFixed(1).padStart(4, "0");
        return `${minutes}m ${seconds}s`;
    }

    onMount(() => {
        const onSave = () => {
            void saveFilter();
        };
        const onLoad = () => {
            void loadFilter();
        };
        const onStreamingModeChange = (event: Event) => {
            const customEvent = event as CustomEvent<{ enabled?: unknown }>;
            if (typeof customEvent.detail?.enabled === "boolean") {
                streamingEnabled = customEvent.detail.enabled;
            }
        };

        window.addEventListener("bolt-save-filter", onSave);
        window.addEventListener("bolt-load-filter", onLoad);
        window.addEventListener(
            "bolt-streaming-mode-changed",
            onStreamingModeChange,
        );

        streamWorker = new Worker(
            new URL("./search-stream.worker.ts", import.meta.url),
            {
                type: "module",
            },
        );
        streamWorker.postMessage({
            type: "configure",
            payload: { debounceMs: WORKER_UI_DEBOUNCE_MS },
        } as StreamWorkerInput);
        streamWorker.onmessage = (event: MessageEvent<StreamWorkerOutput>) => {
            if (event.data.runId !== activeRunId) {
                return;
            }

            if (event.data.type === "batched-progress") {
                const allowEntries = activeRunMode === "streaming";
                applyStreamingProgress(event.data.payload, allowEntries);
                return;
            }

            streamTruncated = event.data.payload.truncated;
            scanningFolders = {};
            searchStatus = `Done (${event.data.payload.totalResults} result${event.data.payload.totalResults === 1 ? "" : "s"}${event.data.payload.truncated ? ", max cap reached" : ""})`;

            const resolve = streamCompletionResolvers.get(event.data.runId);
            if (resolve) {
                resolve();
                streamCompletionResolvers.delete(event.data.runId);
            }
        };

        const storedStreaming = localStorage.getItem(
            "bolt-search-streaming-enabled",
        );
        streamingEnabled = !(
            storedStreaming === "0" || storedStreaming === "false"
        );

        void (async () => {
            try {
                availableRoots = await invoke<string[]>("list_search_roots");
                if (searchForm.scopeMode === "drive") {
                    ensureDriveScopeSelection();
                }
            } catch {
                availableRoots = [];
            } finally {
                window.dispatchEvent(new CustomEvent("bolt-ui-ready"));
            }
        })();

        return () => {
            window.removeEventListener("bolt-save-filter", onSave);
            window.removeEventListener("bolt-load-filter", onLoad);
            window.removeEventListener(
                "bolt-streaming-mode-changed",
                onStreamingModeChange,
            );
            stopDriveCountAnimations();
            clearSearchTimer();
            streamWorker?.terminate();
            streamWorker = null;
            streamCompletionResolvers.clear();
        };
    });

    function resetSearchVisualState() {
        results = [];
        openDirectories = {};
        driveScanCounts = {};
        displayedDriveScanCounts = {};
        driveScanOrder = [];
        stopDriveCountAnimations();
        scanningFolders = {};
        streamTruncated = false;
    }

    function resetSearchForm() {
        searchForm = createDefaultSearchForm();
        enforceFolderScopeValidation = false;
    }

    async function pickScopeFolders() {
        const selectedDrive = searchForm.scopeDrive.trim();
        const selectedFolders = searchForm.scopeFolders;
        const defaultPath =
            selectedFolders[0] ||
            (selectedDrive && selectedDrive !== "ALL"
                ? selectedDrive
                : undefined);

        try {
            const selected = await open({
                directory: true,
                multiple: true,
                ...(defaultPath ? { defaultPath } : {}),
            });

            if (Array.isArray(selected)) {
                searchForm.scopeFolders = dedupePaths([
                    ...selectedFolders,
                    ...selected,
                ]);
                searchForm.scopeMode = "folder";
            } else if (typeof selected === "string") {
                searchForm.scopeFolders = dedupePaths([
                    ...selectedFolders,
                    selected,
                ]);
                searchForm.scopeMode = "folder";
            }
        } catch (e) {
            console.error("Folder selection failed:", e);
        }
    }

    function animateNumber(
        prev: number,
        next: number,
        onUpdate: (value: number) => void,
        durationMs = 220,
    ): () => void {
        if (prev === next) {
            onUpdate(next);
            return () => {};
        }

        const from = Number.isFinite(prev) ? prev : 0;
        const to = Number.isFinite(next) ? next : 0;
        const delta = to - from;
        const duration = Math.max(80, durationMs);
        const startedAt = performance.now();
        let frame = 0;

        const tick = (now: number) => {
            const t = Math.min(1, (now - startedAt) / duration);
            const eased = 1 - Math.pow(1 - t, 3);
            onUpdate(Math.round(from + delta * eased));

            if (t < 1) {
                frame = requestAnimationFrame(tick);
            } else {
                onUpdate(to);
            }
        };

        frame = requestAnimationFrame(tick);
        return () => cancelAnimationFrame(frame);
    }

    function stopDriveCountAnimations() {
        for (const cancel of driveCountAnimationCancels.values()) {
            cancel();
        }
        driveCountAnimationCancels.clear();
    }

    function animateDriveCount(drive: string, nextValue: number) {
        driveCountAnimationCancels.get(drive)?.();
        const prevValue = displayedDriveScanCounts[drive] ?? 0;
        const cancel = animateNumber(prevValue, nextValue, (value) => {
            displayedDriveScanCounts[drive] = value;
        });
        driveCountAnimationCancels.set(drive, cancel);
    }

    function initializeDriveScanSlots(rootsToScan: string[]) {
        stopDriveCountAnimations();
        const drives = Array.from(
            new Set(rootsToScan.map(driveLabelFromPath)),
        ).slice(0, 4);
        driveScanOrder = drives;
        driveScanCounts = Object.fromEntries(drives.map((drive) => [drive, 0]));
        displayedDriveScanCounts = Object.fromEntries(
            drives.map((drive) => [drive, 0]),
        );
    }

    function incrementDriveScanned(rootPath: string, scannedFolders: number) {
        if (scannedFolders <= 0) return;

        const drive = driveLabelFromPath(rootPath);
        if (!driveScanOrder.includes(drive) && driveScanOrder.length < 4) {
            driveScanOrder = [...driveScanOrder, drive];
            displayedDriveScanCounts[drive] =
                displayedDriveScanCounts[drive] ?? 0;
        }

        driveScanCounts[drive] = (driveScanCounts[drive] ?? 0) + scannedFolders;
        animateDriveCount(drive, driveScanCounts[drive]);
    }

    function markFolderScanning(folderPath: string, active: boolean) {
        const normalized = normalizePath(folderPath);
        if (!normalized) return;

        if (active) {
            scanningFolders[normalized] = true;
            return;
        }

        if (!scanningFolders[normalized]) {
            return;
        }

        delete scanningFolders[normalized];
    }

    function isFolderScanning(folderPath: string): boolean {
        return !!scanningFolders[folderPath];
    }

    function isDirectoryOpen(path: string, depth: number): boolean {
        const state = openDirectories[path];
        if (state !== undefined) return state;
        return depth === 0;
    }

    function toggleDirectory(path: string, depth: number) {
        const next = !isDirectoryOpen(path, depth);
        openDirectories = { ...openDirectories, [path]: next };
    }

    function appendResults(chunk: FileEntry[]) {
        if (chunk.length === 0 || results.length >= MAX_RESULTS) return;
        const remaining = MAX_RESULTS - results.length;
        results.push(...chunk.slice(0, remaining));
    }

    function applyStreamingProgress(
        payload: StreamingProgressEvent["data"],
        allowEntries: boolean,
    ) {
        if (allowEntries) {
            for (const folder of payload.startedFolders) {
                markFolderScanning(folder, true);
            }

            for (const folder of payload.finishedFolders) {
                markFolderScanning(folder, false);
            }
        } else if (Object.keys(scanningFolders).length > 0) {
            scanningFolders = {};
        }

        if (allowEntries) {
            appendResults(payload.entries);
        }

        const finishedByDrive = new Map<string, number>();
        for (const folder of payload.finishedFolders) {
            const drive = driveLabelFromPath(folder);
            finishedByDrive.set(drive, (finishedByDrive.get(drive) ?? 0) + 1);
        }
        for (const [drive, count] of finishedByDrive) {
            incrementDriveScanned(drive, count);
        }

        const modeLabel = activeRunMode === "batch" ? "Batch" : "Streaming";
        searchStatus = `${modeLabel}: scanned ${payload.scannedFolders} folders, ${payload.totalResults} result${payload.totalResults === 1 ? "" : "s"}`;
    }

    async function scanRootsStreaming(
        rootsToScan: string[],
        scopedQuery: ScopedQuery,
        runId: number,
    ) {
        const onEvent = new Channel<StreamingSearchEvent>();
        const completionPromise = streamWorker
            ? new Promise<void>((resolve) => {
                  streamCompletionResolvers.set(runId, resolve);
              })
            : null;

        if (streamWorker) {
            streamWorker.postMessage({
                type: "reset",
                runId,
            } as StreamWorkerInput);
        }

        onEvent.onmessage = (message) => {
            if (runId !== activeRunId) {
                return;
            }

            switch (message.event) {
                case "progress": {
                    if (streamWorker) {
                        streamWorker.postMessage({
                            type: "progress",
                            runId,
                            payload: message.data,
                        } as StreamWorkerInput);
                    } else {
                        applyStreamingProgress(message.data, true);
                    }
                    break;
                }

                case "completed": {
                    if (streamWorker) {
                        streamWorker.postMessage({
                            type: "completed",
                            runId,
                            payload: message.data,
                        } as StreamWorkerInput);
                    } else {
                        scanningFolders = {};
                        streamTruncated = message.data.truncated;
                        searchStatus = `Done (${message.data.totalResults} result${message.data.totalResults === 1 ? "" : "s"}${message.data.truncated ? ", max cap reached" : ""})`;
                    }
                    break;
                }
            }
        };

        try {
            await invoke("search_streaming", {
                query: scopedQuery,
                roots: rootsToScan,
                limit: MAX_RESULTS,
                runId,
                onEvent,
            });

            if (completionPromise) {
                await completionPromise;
            }
        } catch (error) {
            streamCompletionResolvers.delete(runId);
            throw error;
        }
    }

    async function scanRootsBatchWithProgress(
        rootsToScan: string[],
        scopedQuery: ScopedQuery,
        runId: number,
    ): Promise<FileEntry[]> {
        const onEvent = new Channel<StreamingSearchEvent>();
        const completionPromise = streamWorker
            ? new Promise<void>((resolve) => {
                  streamCompletionResolvers.set(runId, resolve);
              })
            : null;

        if (streamWorker) {
            streamWorker.postMessage({
                type: "reset",
                runId,
            } as StreamWorkerInput);
        }

        onEvent.onmessage = (message) => {
            if (runId !== activeRunId) {
                return;
            }

            switch (message.event) {
                case "progress": {
                    const batchProgressPayload = {
                        ...message.data,
                        entries: [] as FileEntry[],
                    };

                    if (streamWorker) {
                        streamWorker.postMessage({
                            type: "progress",
                            runId,
                            payload: batchProgressPayload,
                        } as StreamWorkerInput);
                    } else {
                        applyStreamingProgress(batchProgressPayload, false);
                    }
                    break;
                }

                case "completed": {
                    if (streamWorker) {
                        streamWorker.postMessage({
                            type: "completed",
                            runId,
                            payload: message.data,
                        } as StreamWorkerInput);
                    } else {
                        scanningFolders = {};
                        streamTruncated = message.data.truncated;
                        searchStatus = `Done (${message.data.totalResults} result${message.data.totalResults === 1 ? "" : "s"}${message.data.truncated ? ", max cap reached" : ""})`;
                    }
                    break;
                }
            }
        };

        try {
            const finalResults = await invoke<FileEntry[]>(
                "search_with_progress",
                {
                    query: scopedQuery,
                    roots: rootsToScan,
                    limit: MAX_RESULTS,
                    runId,
                    onEvent,
                },
            );

            if (completionPromise) {
                await completionPromise;
            }

            return finalResults;
        } catch (error) {
            streamCompletionResolvers.delete(runId);
            throw error;
        }
    }

    async function search() {
        if (searching) return;

        enforceFolderScopeValidation = true;
        const submitIssues = analyzeSearchForm(searchForm, {
            enforceFolderScopeSelection: true,
        });
        if (submitIssues.length > 0) {
            searchStatus = submitIssues[0].message;
            return;
        }

        startSearchTimer();
        const runId = activeRunId + 1;
        activeRunId = runId;
        searching = true;
        searched = true;
        searchStatus = "Preparing roots...";
        resetSearchVisualState();
        try {
            const roots =
                availableRoots.length > 0
                    ? availableRoots
                    : await invoke<string[]>("list_search_roots");

            if (runId !== activeRunId) {
                return;
            }

            const selectedDrive =
                searchForm.scopeMode === "drive"
                    ? searchForm.scopeDrive.trim()
                    : "ALL";
            const selectedSubfolders =
                searchForm.scopeMode === "folder"
                    ? dedupePaths(searchForm.scopeFolders)
                    : [];

            const rootsToScan =
                selectedSubfolders.length > 0
                    ? selectedSubfolders
                    : selectedDrive && selectedDrive !== "ALL"
                      ? roots.filter((r) => r === selectedDrive)
                      : roots;

            const scopedQuery = {
                filters: query.filters.filter(
                    (f) => f.type !== "drive" && f.type !== "subfolder",
                ),
            };

            if (rootsToScan.length === 0) {
                searchStatus = "No roots found";
                return;
            }

            const runMode: Exclude<SearchRunMode, null> = streamingEnabled
                ? "streaming"
                : "batch";
            activeRunMode = runMode;

            if (runMode === "streaming") {
                initializeDriveScanSlots(rootsToScan);
                await scanRootsStreaming(rootsToScan, scopedQuery, runId);
            } else {
                initializeDriveScanSlots(rootsToScan);
                searchStatus = "Batch mode: scanning...";
                const fullResults = await scanRootsBatchWithProgress(
                    rootsToScan,
                    scopedQuery,
                    runId,
                );

                if (runId !== activeRunId) {
                    return;
                }

                results = fullResults.slice(0, MAX_RESULTS);
                searchStatus = `Done (${results.length} result${results.length === 1 ? "" : "s"})`;
            }

            if (runId !== activeRunId) {
                return;
            }

            if (runMode === "streaming" && !searchStatus.startsWith("Done")) {
                searchStatus = `Done (${results.length} result${results.length === 1 ? "" : "s"}${streamTruncated ? ", max cap reached" : ""})`;
            }
        } catch (e) {
            if (runId !== activeRunId) {
                return;
            }

            console.error("Search failed:", e);
            results = [];
            scanningFolders = {};
            searchStatus = "Search failed";
        } finally {
            if (runId === activeRunId) {
                stopSearchTimer();
                searching = false;
                activeRunMode = null;
            }
        }
    }

    async function stopSearch() {
        if (!searching) return;

        const runIdToStop = activeRunId;
        activeRunId = runIdToStop + 1;
        activeRunMode = null;
        stopSearchTimer();
        searching = false;
        scanningFolders = {};
        streamTruncated = false;
        searchStatus = "Stopping search...";

        const resolve = streamCompletionResolvers.get(runIdToStop);
        if (resolve) {
            resolve();
            streamCompletionResolvers.delete(runIdToStop);
        }

        if (streamWorker) {
            streamWorker.postMessage({
                type: "reset",
                runId: runIdToStop,
            } as StreamWorkerInput);
        }

        if (runIdToStop <= 0) {
            searchStatus = "Search stopped";
            return;
        }

        try {
            await invoke("cancel_search", { runId: runIdToStop });
            searchStatus = "Search stopped";
        } catch (e) {
            console.error("Stop search failed:", e);
            searchStatus = "Stop request failed";
        }
    }

    async function openInExplorer(path: string) {
        await invoke("open_in_explorer", { path });
    }

    function clearSearchResults() {
        if (searching) return;
        resetSearchVisualState();
        searched = false;
        searchStatus = "";
        activeRunMode = null;
        streamCompletionResolvers.clear();
    }

    async function saveFilter() {
        try {
            const selectedPath = await saveDialog({
                title: "Save Filter",
                defaultPath: "bolt-filter.bsearch",
                filters: [
                    { name: "Bolt Search Filter", extensions: ["bsearch"] },
                ],
            });

            if (typeof selectedPath !== "string" || !selectedPath.trim()) {
                return;
            }

            const payload = JSON.stringify(
                FilterModel.toSavedFile(searchFilters),
                null,
                2,
            );
            await invoke("save_filter_file", {
                path: selectedPath,
                content: payload,
            });
            searchStatus = `Search profile saved: ${selectedPath}`;
        } catch (e) {
            console.error("Save filter failed:", e);
            searchStatus = "Save filter failed";
        }
    }

    async function loadFilter() {
        try {
            const selectedPath = await open({
                title: "Load Filter",
                multiple: false,
                directory: false,
                filters: [
                    { name: "Bolt Search Filter", extensions: ["bsearch"] },
                ],
            });

            if (typeof selectedPath !== "string" || !selectedPath.trim()) {
                return;
            }

            const content = await invoke<string>("load_filter_file", {
                path: selectedPath,
            });
            const saved = FilterModel.parseSavedFile(content);
            const loadedFilters = FilterModel.fromSavedFile(saved, 0);

            searchForm = formFromFilters(loadedFilters);
            clearSearchResults();
            searchStatus = `Search profile loaded: ${selectedPath}`;
        } catch (e) {
            console.error("Load filter failed:", e);
            searchStatus = "Load filter failed";
        }
    }
</script>

<div class="w-full h-full flex flex-row bg-white dark:bg-zinc-900">
    <SearchSidebar
        bind:searchForm
        bind:showAdvanced
        searching={searching}
        searched={searched}
        resultsCount={results.length}
        hasContradiction={hasContradiction}
        validationIssues={validationIssues}
        driveOptions={driveOptions}
        onSearch={() => {
            void search();
        }}
        onStopSearch={() => {
            void stopSearch();
        }}
        onClearResults={clearSearchResults}
        onResetForm={resetSearchForm}
        onPickScopeFolders={() => {
            void pickScopeFolders();
        }}
        onEnsureDriveScopeSelection={ensureDriveScopeSelection}
        {displayPath}
    />

    <SearchResultsPanel
        searching={searching}
        searched={searched}
        searchStatus={searchStatus}
        resultsCount={results.length}
        driveScanTotal={driveScanTotal}
        searchDurationLabel={searchDurationLabel}
        driveScanRows={driveScanRows}
        treeRows={treeRows}
        {rowIndentClass}
        {displayPath}
        {isFolderScanning}
        {toggleDirectory}
        openInExplorer={(path) => {
            void openInExplorer(path);
        }}
    />
</div>

