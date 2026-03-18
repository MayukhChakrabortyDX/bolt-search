<script lang="ts">
    import { Channel, invoke } from "@tauri-apps/api/core";
    import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
    import { fade } from "svelte/transition";
    import { onMount } from "svelte";
    import ChipSelect, {
        type ChipSelectOption,
    } from "../lib/components/ChipSelect.svelte";
    import { FilterModel, type Filter, type FilterType } from "./filter.svelte";
    import {
        AlertTriangle,
        ChevronDown,
        ChevronRight,
        File,
        Folder,
        FolderOpen,
        HardDrive,
        LoaderCircle,
        Plus,
        Search,
        Trash2,
        X,
    } from "lucide-svelte";

    type FileEntry = {
        name: string;
        path: string;
        is_dir: boolean;
        size: number;
        modified: string;
    };

    type StreamingProgressEvent = {
        event: "progress";
        data: {
            startedFolders: string[];
            finishedFolders: string[];
            entries: FileEntry[];
            scannedFolders: number;
            totalResults: number;
        };
    };

    type StreamingCompletedEvent = {
        event: "completed";
        data: {
            scannedFolders: number;
            totalResults: number;
            truncated: boolean;
        };
    };

    type StreamingSearchEvent =
        | StreamingProgressEvent
        | StreamingCompletedEvent;

    type StreamWorkerInput =
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

    type StreamWorkerOutput =
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

    type ScopedQuery = {
        filters: Array<{
            type: string;
            value?: string;
            value2?: string;
            unit?: string;
        }>;
    };

    type SearchRunMode = "streaming" | "batch" | null;

    type TreeNode = {
        name: string;
        path: string;
        isDir: boolean;
        children: TreeNode[];
    };

    type MutableTreeNode = {
        name: string;
        path: string;
        isDir: boolean;
        children: Map<string, MutableTreeNode>;
    };

    type TreeRow = {
        node: TreeNode;
        depth: number;
        hasChildren: boolean;
        isOpen: boolean;
    };

    type DriveScanRow = {
        label: string;
        scanned: number;
        active: boolean;
    };

    const MAX_RESULTS = 10_000;
    const WORKER_UI_DEBOUNCE_MS = 50;
    const SIZE_UNIT_OPTIONS: ChipSelectOption[] = [
        { value: "B", label: "B" },
        { value: "KB", label: "KB" },
        { value: "MB", label: "MB" },
        { value: "GB", label: "GB" },
    ];

    const filterMeta = FilterModel.meta;

    let filters = $state<Filter[]>([]);
    let nextId = $state(0);
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
    let streamWorker: Worker | null = null;
    const streamCompletionResolvers = new Map<number, () => void>();
    const driveCountAnimationCancels = new Map<string, () => void>();

    const query = $derived(FilterModel.toQuery(filters));
    const filterTypeOptions = $derived(
        (Object.entries(filterMeta) as [FilterType, (typeof filterMeta)[FilterType]][])
            .map(([value, meta]) => ({ value, label: meta.label })),
    );
    const driveOptions = $derived([
        { value: "ALL", label: "Global (all drives)" },
        ...availableRoots.map((root) => ({ value: root, label: root })),
    ]);

    // ── Contradiction analysis ────────────────────────────────────────────────

    type Contradiction = { message: string; filters: number[] };

    function analyzeContradictions(filters: Filter[]): Contradiction[] {
        const contradictions: Contradiction[] = [];
        const get = (type: FilterType) =>
            filters.filter((f) => f.type === type);
        const parseDate = (value: string): Date | null => {
            const trimmed = value.trim();
            if (!trimmed) {
                return null;
            }

            const parsed = new Date(trimmed);
            return isNaN(parsed.getTime()) ? null : parsed;
        };
        const toBytes = (value: string, unit: string = "B"): number => {
            const n = parseFloat(value);
            if (isNaN(n)) return -1;
            const map: Record<string, number> = {
                B: 1,
                KB: 1024,
                MB: 1024 ** 2,
                GB: 1024 ** 3,
            };
            return n * (map[unit] ?? 1);
        };

        const stackable: FilterType[] = FilterModel.stackableTypes;
        const seen = new Map<FilterType, number[]>();
        for (const f of filters) {
            if (stackable.includes(f.type)) continue;
            if (!seen.has(f.type)) seen.set(f.type, []);
            seen.get(f.type)!.push(f.id);
        }
        for (const [type, ids] of seen) {
            if (ids.length > 1) {
                contradictions.push({
                    message: `"${filterMeta[type].label}" is added more than once`,
                    filters: ids,
                });
            }
        }

        const sizeGt = get("size_gt");
        const sizeLt = get("size_lt");
        if (sizeGt.length && sizeLt.length) {
            const gtBytes = toBytes(sizeGt[0].value, sizeGt[0].unit);
            const ltBytes = toBytes(sizeLt[0].value, sizeLt[0].unit);
            if (gtBytes !== -1 && ltBytes !== -1 && gtBytes >= ltBytes) {
                contradictions.push({
                    message: `Size greater than (${sizeGt[0].value}${sizeGt[0].unit}) must be less than size less than (${sizeLt[0].value}${sizeLt[0].unit})`,
                    filters: [sizeGt[0].id, sizeLt[0].id],
                });
            }
        }

        const check = (
            afterType: FilterType,
            beforeType: FilterType,
            label: string,
        ) => {
            const after = get(afterType);
            const before = get(beforeType);
            if (after.length && before.length) {
                const a = parseDate(after[0].value);
                const b = parseDate(before[0].value);
                if (a && b && a >= b) {
                    contradictions.push({
                        message: `"${label} after" must be earlier than "${label} before"`,
                        filters: [after[0].id, before[0].id],
                    });
                }
            }
        };

        const checkRange = (rangeType: FilterType, label: string) => {
            const range = get(rangeType);
            if (!range.length) {
                return;
            }

            const start = parseDate(range[0].value);
            const end = parseDate(range[0].value2);
            if (start && end && start > end) {
                contradictions.push({
                    message: `"${label}" start date must be earlier than or equal to end date`,
                    filters: [range[0].id],
                });
            }
        };

        check("modified_after", "modified_before", "Modified");
        check("created_after", "created_before", "Created");
        checkRange("modified_range", "Range Modified");
        checkRange("created_range", "Range Created");

        const fileOnly = get("file_only");
        const folderOnly = get("folder_only");
        if (fileOnly.length && folderOnly.length) {
            contradictions.push({
                message: `"Files only" and "Folders only" cannot both be active`,
                filters: [fileOnly[0].id, folderOnly[0].id],
            });
        }

        return contradictions;
    }

    const contradictions = $derived(analyzeContradictions(filters));
    const hasContradiction = $derived(contradictions.length > 0);

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
    const treeRows = $derived(flattenVisibleRows(resultTree));
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
            } catch {
                availableRoots = [];
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
            streamWorker?.terminate();
            streamWorker = null;
            streamCompletionResolvers.clear();
        };
    });

    // ── Actions ───────────────────────────────────────────────────────────────

    function addFilter() {
        filters.push(FilterModel.create(nextId++));
    }

    function removeFilter(id: number) {
        filters = filters.filter((f) => f.id !== id);
    }

    function onFilterTypeChange(filter: Filter) {
        FilterModel.applyTypeDefaults(filter);
    }

    function filterTileClass(type: FilterType): string {
        return `filter-tile filter-${type.replaceAll("_", "-")}`;
    }

    function parseSubfolderPaths(value: string): string[] {
        return value
            .split("\n")
            .map((v) => v.trim())
            .filter((v) => v.length > 0);
    }

    function dedupePaths(paths: string[]): string[] {
        const seen = new Set<string>();
        const unique: string[] = [];

        for (const path of paths) {
            const normalized = path.trim();
            if (!normalized || seen.has(normalized)) continue;
            seen.add(normalized);
            unique.push(normalized);
        }

        return unique;
    }

    function encodeSubfolderPaths(paths: string[]): string {
        return dedupePaths(paths).join("\n");
    }

    function subfolderPathsFor(filter: Filter): string[] {
        return parseSubfolderPaths(filter.value);
    }

    function removeSubfolderPath(filter: Filter, pathToRemove: string) {
        const remaining = subfolderPathsFor(filter).filter(
            (p) => p !== pathToRemove,
        );
        filter.value = encodeSubfolderPaths(remaining);
    }

    async function pickSubfolder(filter: Filter) {
        const driveFilter = filters.find((f) => f.type === "drive");
        const selectedDrive = (driveFilter?.value ?? "").trim();
        const selectedFolders = subfolderPathsFor(filter);
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
                filter.value = encodeSubfolderPaths([
                    ...selectedFolders,
                    ...selected,
                ]);
            } else if (typeof selected === "string") {
                filter.value = encodeSubfolderPaths([
                    ...selectedFolders,
                    selected,
                ]);
            }
        } catch (e) {
            console.error("Folder selection failed:", e);
        }
    }

    function normalizePath(path: string): string {
        return path.replace(/\\/g, "/").replace(/\/+/g, "/").replace(/\/$/, "");
    }

    function displayPath(path: string): string {
        return path.replace(/\//g, "\\");
    }

    function driveLabelFromPath(path: string): string {
        const normalized = path.replace(/\//g, "\\");
        const match = normalized.match(/^[A-Za-z]:/);
        if (match) {
            return `${match[0].toUpperCase()}\\`;
        }
        return "Other";
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

    function createMutableNode(
        name: string,
        path: string,
        isDir: boolean,
    ): MutableTreeNode {
        return { name, path, isDir, children: new Map() };
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

    function insertPathIntoTree(
        roots: Map<string, MutableTreeNode>,
        normalizedPath: string,
        isLeafDirectory: boolean,
    ) {
        const segments = normalizedPath.split("/").filter(Boolean);
        if (segments.length === 0) return;

        let currentMap = roots;
        let currentPath = "";

        for (let i = 0; i < segments.length; i++) {
            const segment = segments[i];
            const isLast = i === segments.length - 1;
            currentPath = currentPath ? `${currentPath}/${segment}` : segment;

            let node = currentMap.get(segment);
            if (!node) {
                node = createMutableNode(
                    segment,
                    currentPath,
                    isLast ? isLeafDirectory : true,
                );
                currentMap.set(segment, node);
            } else if (!isLast || isLeafDirectory) {
                node.isDir = true;
            }

            currentMap = node.children;
        }
    }

    function mutableToTree(node: MutableTreeNode): TreeNode {
        const children = Array.from(node.children.values())
            .map(mutableToTree)
            .sort((a, b) => {
                if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
                return a.name.localeCompare(b.name);
            });

        return {
            name: node.name,
            path: node.path,
            isDir: node.isDir || children.length > 0,
            children,
        };
    }

    function buildResultTree(
        entries: FileEntry[],
        inFlightFolders: string[],
    ): TreeNode[] {
        const roots = new Map<string, MutableTreeNode>();

        for (const entry of entries) {
            const normalized = normalizePath(entry.path);
            if (!normalized) continue;
            insertPathIntoTree(roots, normalized, entry.is_dir);
        }

        for (const folder of inFlightFolders) {
            const normalized = normalizePath(folder);
            if (!normalized) continue;
            insertPathIntoTree(roots, normalized, true);
        }

        return Array.from(roots.values())
            .map(mutableToTree)
            .sort((a, b) => {
                if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
                return a.name.localeCompare(b.name);
            });
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

    function flattenVisibleRows(
        nodes: TreeNode[],
        depth = 0,
        rows: TreeRow[] = [],
    ): TreeRow[] {
        for (const node of nodes) {
            const hasChildren = node.children.length > 0;
            const isOpen =
                node.isDir && hasChildren
                    ? isDirectoryOpen(node.path, depth)
                    : false;

            rows.push({ node, depth, hasChildren, isOpen });

            if (node.isDir && hasChildren && isOpen) {
                flattenVisibleRows(node.children, depth + 1, rows);
            }
        }

        return rows;
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
            // Batch mode should never project transient folder nodes in the result tree.
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
        if (hasContradiction || searching) return;
        const runId = activeRunId + 1;
        activeRunId = runId;
        searching = true;
        searched = true;
        searchStatus = "Preparing roots...";
        results = [];
        openDirectories = {};
        driveScanCounts = {};
        displayedDriveScanCounts = {};
        driveScanOrder = [];
        stopDriveCountAnimations();
        scanningFolders = {};
        streamTruncated = false;
        try {
            const roots =
                availableRoots.length > 0
                    ? availableRoots
                    : await invoke<string[]>("list_search_roots");

            if (runId !== activeRunId) {
                return;
            }

            const driveFilter = filters.find((f) => f.type === "drive");
            const selectedDrive = (driveFilter?.value ?? "ALL").trim();
            const selectedSubfolders = dedupePaths(
                filters
                    .filter((f) => f.type === "subfolder")
                    .flatMap((f) => parseSubfolderPaths(f.value)),
            );

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
        results = [];
        searched = false;
        searchStatus = "";
        openDirectories = {};
        driveScanCounts = {};
        displayedDriveScanCounts = {};
        driveScanOrder = [];
        stopDriveCountAnimations();
        scanningFolders = {};
        streamTruncated = false;
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
                FilterModel.toSavedFile(filters),
                null,
                2,
            );
            await invoke("save_filter_file", {
                path: selectedPath,
                content: payload,
            });
            searchStatus = `Filter saved: ${selectedPath}`;
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

            filters = loadedFilters;
            nextId = loadedFilters.length;
            clearSearchResults();
            searchStatus = `Filter loaded: ${selectedPath}`;
        } catch (e) {
            console.error("Load filter failed:", e);
            searchStatus = "Load filter failed";
        }
    }

    function formatSize(bytes: number): string {
        if (bytes === 0) return "0 B";
        if (bytes < 1024) return `${bytes} B`;
        if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
        if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
        return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
    }
</script>

<div class="w-full h-full flex">
    <!-- Sidebar -->
    <div
        class="sidebar-panel w-75 h-full bg-zinc-100 border-r border-zinc-300 flex flex-col p-4 gap-3"
    >
        <span
            class="text-xs font-semibold text-zinc-400 uppercase tracking-widest"
            >Filter Panel</span
        >

        <div class="grid grid-cols-3 gap-2">
            <button
                class="py-2 text-[11px] rounded-md border border-zinc-300 bg-white hover:bg-zinc-50 text-zinc-600 flex items-center justify-center gap-1"
                onclick={addFilter}
            >
                <Plus size={13} strokeWidth={2} />
                Add
            </button>

            <button
                class="py-2 text-[11px] rounded-md font-medium transition-colors text-white flex items-center justify-center gap-1
                    {searching
                    ? 'bg-red-600 hover:bg-red-700'
                    : hasContradiction
                      ? 'bg-red-500 hover:bg-red-600'
                      : 'bg-zinc-800 hover:bg-zinc-700'}"
                onclick={searching ? stopSearch : search}
                disabled={!searching && hasContradiction}
            >
                {#if searching}
                    <X size={13} strokeWidth={2} />
                    Stop
                {:else if hasContradiction}
                    <AlertTriangle size={13} strokeWidth={2} />
                    Blocked
                {:else}
                    <Search size={13} strokeWidth={2} />
                    Search
                {/if}
            </button>

            <button
                class="py-2 text-[11px] rounded-md border border-zinc-300 bg-white hover:bg-zinc-50 text-zinc-600 flex items-center justify-center gap-1 disabled:opacity-50 disabled:cursor-not-allowed"
                onclick={clearSearchResults}
                disabled={searching || (!searched && results.length === 0)}
            >
                <Trash2 size={13} strokeWidth={2} />
                Clear
            </button>
        </div>

        <!-- Filter tokens -->
        <div class="flex flex-col gap-2 flex-1 overflow-y-auto">
            {#if filters.length === 0}
                <p class="text-xs text-zinc-400 text-center mt-16">
                    Add filters to show here
                </p>
            {/if}

            {#each filters as filter (filter.id)}
                <div class={filterTileClass(filter.type)}>
                    <div class="filter-tile-head">
                        <span class="filter-chip">
                            <span class="filter-chip-dot"></span>
                            {filterMeta[filter.type].label}
                        </span>
                        <button
                            class="filter-remove-btn"
                            onclick={() => removeFilter(filter.id)}
                            aria-label="Remove filter"
                        >
                            <X size={12} strokeWidth={2} />
                        </button>
                    </div>

                    <div class="filter-type-row">
                        <ChipSelect
                            containerClass="chip-filter-select filter-type-select"
                            ariaLabel="Filter type"
                            bind:value={filter.type}
                            options={filterTypeOptions}
                            onChange={() => onFilterTypeChange(filter)}
                        />
                    </div>

                    {#if filterMeta[filter.type].hasValue}
                        {#if filter.type === "drive"}
                            <ChipSelect
                                containerClass="chip-filter-select"
                                ariaLabel="Drive scope"
                                bind:value={filter.value}
                                options={driveOptions}
                            />
                        {:else if filter.type === "subfolder"}
                            <div class="flex flex-col gap-1 w-full">
                                <input
                                    type="text"
                                    class="filter-field"
                                    value={subfolderPathsFor(filter).join(
                                        " | ",
                                    ) || "No folder selected"}
                                    placeholder="Select folder(s)"
                                    disabled
                                />
                                <div class="flex gap-1">
                                    <button
                                        type="button"
                                        class="filter-action-btn h-7 flex-1"
                                        onclick={() => pickSubfolder(filter)}
                                        aria-label="Browse for folders"
                                        title="Browse"
                                    >
                                        <FolderOpen size={13} strokeWidth={2} />
                                        Browse
                                    </button>
                                    <button
                                        type="button"
                                        class="filter-action-btn h-7 w-7 text-zinc-500"
                                        onclick={() => {
                                            filter.value = "";
                                        }}
                                        aria-label="Clear selected folders"
                                        title="Clear"
                                    >
                                        <X size={12} strokeWidth={2} />
                                    </button>
                                </div>

                                {#if subfolderPathsFor(filter).length > 0}
                                    <div
                                        class="max-h-24 overflow-auto border border-zinc-200 rounded-md bg-zinc-50"
                                    >
                                        {#each subfolderPathsFor(filter) as folderPath}
                                            <div
                                                class="px-2 py-1 text-[11px] text-zinc-600 border-b border-zinc-200 last:border-b-0 flex items-start justify-between gap-2"
                                            >
                                                <span
                                                    class="break-all leading-snug"
                                                    >{displayPath(
                                                        folderPath,
                                                    )}</span
                                                >
                                                <button
                                                    type="button"
                                                    class="text-zinc-400 hover:text-red-500 shrink-0"
                                                    onclick={() =>
                                                        removeSubfolderPath(
                                                            filter,
                                                            folderPath,
                                                        )}
                                                    aria-label="Remove selected folder"
                                                >
                                                    <X
                                                        size={11}
                                                        strokeWidth={2}
                                                    />
                                                </button>
                                            </div>
                                        {/each}
                                    </div>
                                {/if}
                            </div>
                        {:else if filter.type.endsWith("_range")}
                            <div class="flex gap-1 w-full">
                                <input
                                    type="date"
                                    class="filter-field"
                                    bind:value={filter.value}
                                    title="Start date"
                                />
                                <input
                                    type="date"
                                    class="filter-field"
                                    bind:value={filter.value2}
                                    title="End date"
                                />
                            </div>
                        {:else if filter.type.includes("modified") || filter.type.includes("created")}
                            <input
                                type="date"
                                class="filter-field"
                                bind:value={filter.value}
                            />
                        {:else if filterMeta[filter.type].isSize}
                            <div class="flex gap-1 w-full">
                                <input
                                    type="number"
                                    min="0"
                                    class="filter-field"
                                    style="width: calc(100% - 52px);"
                                    placeholder="0"
                                    bind:value={filter.value}
                                />
                                <ChipSelect
                                    containerClass="chip-filter-select filter-unit-select"
                                    ariaLabel="Size unit"
                                    bind:value={filter.unit}
                                    options={SIZE_UNIT_OPTIONS}
                                />
                            </div>
                        {:else}
                            <input
                                type="text"
                                class="filter-field"
                                placeholder={filterMeta[filter.type]
                                    .placeholder ?? "value..."}
                                bind:value={filter.value}
                            />
                        {/if}
                    {/if}
                </div>
            {/each}
        </div>

        <!-- Contradiction messages -->
        {#if hasContradiction}
            <div class="flex flex-col gap-1">
                {#each contradictions as c}
                    <p class="text-xs text-red-500 leading-snug">{c.message}</p>
                {/each}
            </div>
        {/if}

    </div>

    <!-- Main panel -->
    <div class="h-full flex flex-col" style="width: calc(100% - 300px)">
        <!-- Header -->
        <div class="border-b border-zinc-200 flex flex-col">
            {#if !searching && !searched}
                <div class="px-3 py-2">
                    <span class="text-xs text-zinc-400">
                        {#if searching}
                            {searchStatus || "Searching..."}
                        {:else if searched}
                            {results.length} result{results.length === 1
                                ? ""
                                : "s"}
                        {:else}
                            Search Status Bar - Empty
                        {/if}
                    </span>
                </div>
            {/if}
            {#if searching || searched}
                <div class="px-3 pt-2">
                    <span class="text-xs text-zinc-400">
                        {#if searching}
                            {searchStatus || "Searching..."}
                        {:else if searched}
                            {results.length} result{results.length === 1
                                ? ""
                                : "s"}
                        {:else}
                            Search Status Bar - Empty
                        {/if}
                    </span>
                </div>
                <div class="px-3 pb-2">
                    <span class="text-xs text-zinc-500">
                        Total scanned: {driveScanTotal} folder{driveScanTotal ===
                        1
                            ? ""
                            : "s"}
                    </span>

                    <span class="text-xs text-zinc-500">
                        Mode: {streamingEnabled ? "Streaming" : "Batch"}
                    </span>
                </div>

                <div
                    class="flex w-full overflow-hidden border-t border-zinc-300 bg-zinc-300"
                >
                    {#each driveScanRows as row, i}
                        <div
                            class="h-8 flex-1 min-w-0 flex items-center justify-between px-6 text-[11px] {row.active
                                ? 'bg-zinc-50'
                                : 'bg-zinc-100'} {i < driveScanRows.length - 1
                                ? 'border-r border-zinc-300'
                                : ''}"
                        >
                            <div class="flex items-center gap-1 min-w-0">
                                <HardDrive
                                    size={12}
                                    class="text-zinc-500 shrink-0"
                                    strokeWidth={2}
                                />
                                <span class="text-zinc-600 font-medium truncate"
                                    >{row.label}</span
                                >
                            </div>
                            <span class="text-zinc-500 whitespace-nowrap">
                                {row.active ? `${row.scanned} folders` : "-"}
                            </span>
                        </div>
                    {/each}
                </div>
            {/if}
        </div>

        <!-- Tree -->
        <div class="flex-1 overflow-auto p-4">
            {#if !searched}
                <div
                    class="flex items-center justify-center h-full"
                    transition:fade={{ duration: 180 }}
                >
                    <span class="text-sm text-zinc-400">No results yet.</span>
                </div>
            {:else if treeRows.length === 0}
                <div
                    class="flex items-center justify-center h-full"
                    transition:fade={{ duration: 180 }}
                >
                    <span
                        class="text-sm text-zinc-400 {searching
                            ? 'animate-pulse'
                            : ''}"
                    >
                        {searching
                            ? "Searching..."
                            : "No files matched your filters."}
                    </span>
                </div>
            {:else}
                <div
                    class="h-full rounded-lg border border-zinc-200 bg-white overflow-auto"
                    transition:fade={{ duration: 220 }}
                >
                    {#each treeRows as row (row.node.path)}
                        {#if row.node.isDir}
                            <button
                                class="w-full flex items-center gap-2 py-2 pr-3 text-left border-b border-zinc-100 hover:bg-zinc-50"
                                style={`padding-left: ${8 + row.depth * 18}px;`}
                                onclick={() =>
                                    toggleDirectory(row.node.path, row.depth)}
                                title={displayPath(row.node.path)}
                            >
                                <span
                                    class="text-xs text-zinc-500"
                                    style="width: 12px; text-align: center;"
                                >
                                    {#if row.hasChildren}
                                        {#if row.isOpen}
                                            <ChevronDown
                                                size={12}
                                                strokeWidth={2}
                                            />
                                        {:else}
                                            <ChevronRight
                                                size={12}
                                                strokeWidth={2}
                                            />
                                        {/if}
                                    {/if}
                                </span>
                                <Folder
                                    size={14}
                                    class="text-zinc-500"
                                    strokeWidth={2}
                                />
                                <span class="text-xs text-zinc-700 font-medium"
                                    >{row.node.name}</span
                                >
                                {#if isFolderScanning(row.node.path)}
                                    <LoaderCircle
                                        size={12}
                                        class="animate-spin text-emerald-600"
                                        strokeWidth={2}
                                    />
                                {/if}
                                <span class="text-xs text-zinc-400 truncate"
                                    >{displayPath(row.node.path)}</span
                                >
                            </button>
                        {:else}
                            <button
                                class="w-full flex items-center gap-2 py-2 pr-3 text-left border-b border-zinc-100 hover:bg-zinc-50"
                                style={`padding-left: ${20 + row.depth * 18}px;`}
                                onclick={() => openInExplorer(row.node.path)}
                                title={displayPath(row.node.path)}
                            >
                                <File
                                    size={14}
                                    class="text-zinc-500"
                                    strokeWidth={2}
                                />
                                <span class="text-xs text-zinc-700 font-medium"
                                    >{row.node.name}</span
                                >
                                <span class="text-xs text-zinc-400 truncate"
                                    >{displayPath(row.node.path)}</span
                                >
                            </button>
                        {/if}
                    {/each}
                </div>
            {/if}
        </div>
    </div>
</div>

<style>
    .filter-tile {
        --filter-h: 220;
        --filter-s: 72%;
        --filter-l: 44%;
        --filter-accent: hsl(var(--filter-h) var(--filter-s) var(--filter-l));
        --filter-control-height: 30px;
        display: flex;
        flex-direction: column;
        gap: 0.45rem;
        padding: 0.65rem;
        border-radius: 0.75rem;
        border: 1px solid
            color-mix(in srgb, var(--filter-accent) 45%, var(--control-border));
        background:
            linear-gradient(
                160deg,
                color-mix(in srgb, var(--filter-accent) 9%, var(--panel) 91%) 0%,
                var(--panel) 62%
            ),
            var(--panel);
    }

    :global(html[data-theme="dark"]) .filter-tile {
        --filter-l: 64%;
        border-color: color-mix(in srgb, var(--filter-accent) 40%, #3c3c3c);
        background:
            linear-gradient(
                160deg,
                color-mix(in srgb, var(--filter-accent) 16%, #1f1f1f) 0%,
                #1f1f1f 64%
            ),
            #1f1f1f;
    }

    .filter-tile-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 0.5rem;
    }

    .filter-chip {
        display: inline-flex;
        align-items: center;
        gap: 0.4rem;
        font-size: 0.67rem;
        font-weight: 700;
        letter-spacing: 0.06em;
        text-transform: uppercase;
        color: color-mix(in srgb, var(--filter-accent) 62%, var(--control-text));
        background: color-mix(in srgb, var(--filter-accent) 12%, transparent);
        border: 1px solid color-mix(in srgb, var(--filter-accent) 34%, transparent);
        padding: 0.16rem 0.45rem;
        border-radius: 999px;
        white-space: nowrap;
    }

    .filter-chip-dot {
        width: 0.42rem;
        height: 0.42rem;
        border-radius: 999px;
        background: var(--filter-accent);
    }

    .filter-type-row {
        width: 100%;
    }

    .filter-type-select {
        width: 100%;
    }

    .chip-filter-select {
        width: 100%;
    }

    .filter-unit-select {
        width: 48px;
        flex-shrink: 0;
    }

    .filter-remove-btn {
        width: 1.5rem;
        height: 1.5rem;
        border-radius: 0.45rem;
        border: 1px solid color-mix(in srgb, var(--filter-accent) 28%, var(--control-border));
        background: color-mix(in srgb, var(--panel) 90%, transparent);
        color: color-mix(in srgb, var(--filter-accent) 45%, var(--control-muted));
        display: inline-flex;
        align-items: center;
        justify-content: center;
        transition:
            background-color 0.15s ease,
            color 0.15s ease,
            border-color 0.15s ease;
    }

    .filter-remove-btn:hover {
        color: #ef4444;
        border-color: color-mix(in srgb, #ef4444 45%, var(--control-border));
        background: color-mix(in srgb, #ef4444 14%, transparent);
    }

    .filter-field {
        width: 100%;
        height: var(--filter-control-height);
        min-height: var(--filter-control-height);
        font-size: 0.75rem;
        padding: 0 0.55rem;
        border-radius: 0.45rem;
        border: 1px solid
            color-mix(in srgb, var(--filter-accent) 18%, var(--control-border));
        background: color-mix(in srgb, var(--panel) 86%, transparent);
        color: var(--control-text);
        transition:
            border-color 0.15s ease,
            background-color 0.15s ease;
    }

    .filter-field:focus {
        outline: none;
        border-color: color-mix(in srgb, var(--filter-accent) 50%, var(--focus-ring));
    }

    .filter-action-btn {
        border-radius: 0.45rem;
        border: 1px solid
            color-mix(in srgb, var(--filter-accent) 20%, var(--control-border));
        background: color-mix(in srgb, var(--panel) 90%, transparent);
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 0.25rem;
        font-size: 0.72rem;
        color: var(--control-text);
        transition:
            background-color 0.15s ease,
            border-color 0.15s ease,
            color 0.15s ease;
    }

    .filter-action-btn:hover {
        background: color-mix(in srgb, var(--filter-accent) 14%, var(--control-bg-hover));
        border-color: color-mix(in srgb, var(--filter-accent) 35%, var(--control-border));
    }

    .filter-extension { --filter-h: 220; }
    .filter-name-contains { --filter-h: 174; }
    .filter-path-contains { --filter-h: 196; }
    .filter-path-prefix { --filter-h: 204; }
    .filter-subfolder { --filter-h: 34; }
    .filter-size-gt { --filter-h: 145; }
    .filter-size-lt { --filter-h: 124; }
    .filter-modified-after { --filter-h: 286; }
    .filter-modified-before { --filter-h: 264; }
    .filter-modified-range { --filter-h: 300; }
    .filter-created-after { --filter-h: 340; }
    .filter-created-before { --filter-h: 358; }
    .filter-created-range { --filter-h: 15; }
    .filter-drive { --filter-h: 48; }
    .filter-hidden { --filter-h: 210; --filter-s: 22%; }
    .filter-readonly { --filter-h: 200; --filter-s: 28%; }
    .filter-file-only { --filter-h: 152; }
    .filter-folder-only { --filter-h: 26; }

</style>
