<script lang="ts">
    import { Channel, invoke } from "@tauri-apps/api/core";
    import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
    import { onMount } from "svelte";
    import { fade } from "svelte/transition";
    import ChipSelect, {
        type ChipSelectOption,
    } from "../lib/components/ChipSelect.svelte";
    import CalendarField from "../lib/components/CalendarField.svelte";
    import { FilterModel, type Filter, type FilterType } from "./filter.svelte";
    import {
        AlertTriangle,
        ChevronDown,
        ChevronRight,
        Clock3,
        File,
        Folder,
        FolderOpen,
        HardDrive,
        LoaderCircle,
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

    type SearchScopeMode = "all" | "drive" | "folder";
    type SearchKind = "any" | "file" | "folder";

    type SearchFormState = {
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

    type ValidationIssue = { message: string };

    const MAX_RESULTS = 10_000;
    const WORKER_UI_DEBOUNCE_MS = 50;
    const SIZE_UNIT_OPTIONS: ChipSelectOption[] = [
        { value: "B", label: "B" },
        { value: "KB", label: "KB" },
        { value: "MB", label: "MB" },
        { value: "GB", label: "GB" },
    ];
    const POPULAR_EXTENSION_OPTIONS = [
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
    const DIR_INDENT_CLASSES = [
        "pl-2",
        "pl-[26px]",
        "pl-[44px]",
        "pl-[62px]",
        "pl-[80px]",
        "pl-[98px]",
        "pl-[116px]",
        "pl-[134px]",
        "pl-[152px]",
        "pl-[170px]",
        "pl-[188px]",
        "pl-[206px]",
        "pl-[224px]",
    ] as const;
    const FILE_INDENT_CLASSES = [
        "pl-5",
        "pl-[38px]",
        "pl-[56px]",
        "pl-[74px]",
        "pl-[92px]",
        "pl-[110px]",
        "pl-[128px]",
        "pl-[146px]",
        "pl-[164px]",
        "pl-[182px]",
        "pl-[200px]",
        "pl-[218px]",
        "pl-[236px]",
    ] as const;

    function createDefaultSearchForm(): SearchFormState {
        return {
            query: "",
            extensionInput: "",
            pathContainsInput: "",
            pathPrefix: "",
            scopeMode: "all",
            scopeDrive: "ALL",
            scopeFolders: [],
            kind: "any",
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

    function resolvePreferredDrive(roots: string[]): string {
        const normalizedPreferred = roots.find((root) => {
            const normalized = root.trim().replace(/\//g, "\\").toUpperCase();
            return normalized === "C:\\" || normalized === "C:";
        });

        if (normalizedPreferred) {
            return normalizedPreferred;
        }

        return roots[0] ?? "C:\\";
    }

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
    const selectedExtensionTokens = $derived(
        parseExtensionTokens(searchForm.extensionInput),
    );
    const driveOptions = $derived([
        { value: "ALL", label: "Global (all drives)" },
        ...availableRoots.map((root) => ({ value: root, label: root })),
    ]);

    // ── Search form validation ────────────────────────────────────────────────

    function analyzeSearchForm(
        form: SearchFormState,
        options?: { enforceFolderScopeSelection?: boolean },
    ): ValidationIssue[] {
        const issues: ValidationIssue[] = [];
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

        if (form.scopeMode === "drive") {
            const selectedDrive = form.scopeDrive.trim();
            if (!selectedDrive || selectedDrive === "ALL") {
                issues.push({
                    message: "Choose a specific drive or switch scope to All drives",
                });
            }
        }

        if (
            options?.enforceFolderScopeSelection &&
            form.scopeMode === "folder" &&
            form.scopeFolders.length === 0
        ) {
            issues.push({
                message: "Folder scope requires at least one selected folder",
            });
        }

        if (form.sizeMin.trim() && form.sizeMax.trim()) {
            const gtBytes = toBytes(form.sizeMin, form.sizeMinUnit);
            const ltBytes = toBytes(form.sizeMax, form.sizeMaxUnit);
            if (gtBytes !== -1 && ltBytes !== -1 && gtBytes >= ltBytes) {
                issues.push({
                    message: "Minimum size must be smaller than maximum size",
                });
            }
        }

        const checkRange = (from: string, to: string, label: string) => {
            if (from.trim() && to.trim()) {
                const a = parseDate(from);
                const b = parseDate(to);
                if (a && b && a >= b) {
                    issues.push({
                        message: `${label} start date must be earlier than end date`,
                    });
                }
            }
        };

        checkRange(form.modifiedFrom, form.modifiedTo, "Modified");
        checkRange(form.createdFrom, form.createdTo, "Created");

        return issues;
    }

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

    function rowIndentClass(depth: number, kind: "dir" | "file"): string {
        const classes = kind === "dir" ? DIR_INDENT_CLASSES : FILE_INDENT_CLASSES;
        const index = Math.max(0, Math.min(depth, classes.length - 1));
        return classes[index];
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

    // ── Actions ───────────────────────────────────────────────────────────────

    function parseMultiValueInput(value: string): string[] {
        return value
            .split(/[\n,;]+/)
            .map((token) => token.trim())
            .filter((token) => token.length > 0);
    }

    function normalizeExtension(value: string): string {
        const trimmed = value.trim().toLowerCase();
        if (!trimmed) return "";
        return trimmed.startsWith(".") ? trimmed : `.${trimmed}`;
    }

    function parseExtensionTokens(value: string): string[] {
        const normalized = parseMultiValueInput(value)
            .map((token) => normalizeExtension(token))
            .filter((token) => token.length > 0);

        return Array.from(new Set(normalized));
    }

    function normalizeExtensionInput() {
        searchForm.extensionInput = parseExtensionTokens(
            searchForm.extensionInput,
        ).join(", ");
    }

    function togglePopularExtension(rawValue: string) {
        const extension = normalizeExtension(rawValue);
        if (!extension) return;

        const current = parseExtensionTokens(searchForm.extensionInput);
        const next = current.includes(extension)
            ? current.filter((item) => item !== extension)
            : [...current, extension];

        searchForm.extensionInput = next.join(", ");
    }

    function removeExtensionToken(valueToRemove: string) {
        const extension = normalizeExtension(valueToRemove);
        if (!extension) return;

        searchForm.extensionInput = parseExtensionTokens(
            searchForm.extensionInput,
        )
            .filter((item) => item !== extension)
            .join(", ");
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

    function toDateRangeFilter(
        filters: Filter[],
        id: number,
        from: string,
        to: string,
        rangeType: Extract<FilterType, "modified_range" | "created_range">,
        afterType: Extract<FilterType, "modified_after" | "created_after">,
        beforeType: Extract<FilterType, "modified_before" | "created_before">,
    ): number {
        const hasFrom = from.trim().length > 0;
        const hasTo = to.trim().length > 0;

        if (hasFrom && hasTo) {
            filters.push({
                id,
                type: rangeType,
                value: from,
                value2: to,
            });
            return id + 1;
        }

        if (hasFrom) {
            filters.push({ id, type: afterType, value: from, value2: "" });
            id += 1;
        }

        if (hasTo) {
            filters.push({ id, type: beforeType, value: to, value2: "" });
            id += 1;
        }

        return id;
    }

    function formToFilters(form: SearchFormState): Filter[] {
        const built: Filter[] = [];
        let id = 0;

        const push = (
            type: FilterType,
            value = "",
            value2 = "",
            unit?: string,
        ) => {
            built.push({ id: id++, type, value, value2, unit });
        };

        const queryValue = form.query.trim();
        if (queryValue) {
            push("name_contains", queryValue);
        }

        for (const extension of parseExtensionTokens(form.extensionInput)) {
            push("extension", extension);
        }

        for (const fragment of parseMultiValueInput(form.pathContainsInput)) {
            push("path_contains", fragment);
        }

        const pathPrefix = form.pathPrefix.trim();
        if (pathPrefix) {
            push("path_prefix", pathPrefix);
        }

        if (form.scopeMode === "drive") {
            push("drive", form.scopeDrive || "ALL");
        } else {
            push("drive", "ALL");
        }

        if (form.scopeMode === "folder" && form.scopeFolders.length > 0) {
            push("subfolder", dedupePaths(form.scopeFolders).join("\n"));
        }

        if (form.kind === "file") {
            push("file_only");
        } else if (form.kind === "folder") {
            push("folder_only");
        }

        if (form.includeHidden) {
            push("hidden");
        }

        if (form.readonlyOnly) {
            push("readonly");
        }

        const minSize = form.sizeMin.trim();
        if (minSize) {
            push("size_gt", minSize, "", form.sizeMinUnit || "B");
        }

        const maxSize = form.sizeMax.trim();
        if (maxSize) {
            push("size_lt", maxSize, "", form.sizeMaxUnit || "B");
        }

        id = toDateRangeFilter(
            built,
            id,
            form.modifiedFrom,
            form.modifiedTo,
            "modified_range",
            "modified_after",
            "modified_before",
        );

        toDateRangeFilter(
            built,
            id,
            form.createdFrom,
            form.createdTo,
            "created_range",
            "created_after",
            "created_before",
        );

        return built;
    }

    function formFromFilters(filters: Filter[]): SearchFormState {
        const next = createDefaultSearchForm();
        const getFirst = (type: FilterType) =>
            filters.find((filter) => filter.type === type);
        const getMany = (type: FilterType) =>
            filters.filter((filter) => filter.type === type);

        next.query = getFirst("name_contains")?.value ?? "";
        next.extensionInput = getMany("extension")
            .map((filter) => filter.value)
            .join(", ");
        next.extensionInput = parseExtensionTokens(next.extensionInput)
            .join(", ");
        next.pathContainsInput = getMany("path_contains")
            .map((filter) => filter.value.trim())
            .filter((value) => value.length > 0)
            .join(", ");
        next.pathPrefix = getFirst("path_prefix")?.value ?? "";

        const drive = (getFirst("drive")?.value ?? "ALL").trim();
        if (drive && drive !== "ALL") {
            next.scopeMode = "drive";
            next.scopeDrive = drive;
        }

        const folderPaths = dedupePaths(
            getMany("subfolder").flatMap((filter) =>
                parseSubfolderPaths(filter.value),
            ),
        );
        if (folderPaths.length > 0) {
            next.scopeMode = "folder";
            next.scopeFolders = folderPaths;
        }

        if (getMany("file_only").length > 0) {
            next.kind = "file";
        } else if (getMany("folder_only").length > 0) {
            next.kind = "folder";
        }

        next.includeHidden = getMany("hidden").length > 0;
        next.readonlyOnly = getMany("readonly").length > 0;

        const sizeMin = getFirst("size_gt");
        if (sizeMin) {
            next.sizeMin = sizeMin.value;
            next.sizeMinUnit = sizeMin.unit ?? "B";
        }

        const sizeMax = getFirst("size_lt");
        if (sizeMax) {
            next.sizeMax = sizeMax.value;
            next.sizeMaxUnit = sizeMax.unit ?? "B";
        }

        const modifiedRange = getFirst("modified_range");
        if (modifiedRange) {
            next.modifiedFrom = modifiedRange.value;
            next.modifiedTo = modifiedRange.value2;
        } else {
            next.modifiedFrom = getFirst("modified_after")?.value ?? "";
            next.modifiedTo = getFirst("modified_before")?.value ?? "";
        }

        const createdRange = getFirst("created_range");
        if (createdRange) {
            next.createdFrom = createdRange.value;
            next.createdTo = createdRange.value2;
        } else {
            next.createdFrom = getFirst("created_after")?.value ?? "";
            next.createdTo = getFirst("created_before")?.value ?? "";
        }

        return next;
    }

    function removeScopeFolder(pathToRemove: string) {
        searchForm.scopeFolders = searchForm.scopeFolders.filter(
            (path) => path !== pathToRemove,
        );
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

<div class="w-full h-full flex flex-row">
    <!-- Sidebar -->
    <div
        class="sidebar-panel w-[360px] min-w-[320px] h-full bg-zinc-50 border-r border-zinc-200 dark:bg-zinc-950 dark:border-zinc-800 flex flex-col p-4 gap-3"
    >
        <span
            class="text-xs font-semibold text-zinc-400 dark:text-zinc-500 uppercase tracking-widest"
            >Search Builder</span
        >

        <div class="grid grid-cols-2 gap-2">
            <button
                class="col-span-2 py-2 text-[11px] rounded-md font-medium transition-colors text-white flex items-center justify-center gap-1
                    {searching
                    ? 'bg-red-600 hover:bg-red-700'
                    : hasContradiction
                      ? 'bg-red-500 hover:bg-red-600'
                                            : 'bg-teal-700 hover:bg-teal-600'}"
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
                class="py-2 text-[11px] rounded-md border border-zinc-300 bg-white hover:bg-zinc-50 text-zinc-600 dark:border-zinc-700 dark:bg-zinc-800 dark:hover:bg-zinc-700 dark:text-zinc-300 flex items-center justify-center gap-1 disabled:opacity-50 disabled:cursor-not-allowed"
                onclick={clearSearchResults}
                disabled={searching || (!searched && results.length === 0)}
            >
                <Trash2 size={13} strokeWidth={2} />
                Clear Results
            </button>

            <button
                class="py-2 text-[11px] rounded-md border border-zinc-300 bg-white hover:bg-zinc-50 text-zinc-600 dark:border-zinc-700 dark:bg-zinc-800 dark:hover:bg-zinc-700 dark:text-zinc-300 flex items-center justify-center gap-1 disabled:opacity-50 disabled:cursor-not-allowed"
                onclick={resetSearchForm}
                disabled={searching}
            >
                <X size={13} strokeWidth={2} />
                Reset Form
            </button>
        </div>

        <div class="sidebar-scroll flex flex-1 flex-col gap-3 overflow-y-auto pr-1">
            <section class="flex flex-col gap-2 rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 p-3">
                <label class="text-[11px] font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400" for="query-input">
                    Name Contains
                </label>
                <input
                    id="query-input"
                    type="text"
                    class="h-[30px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:placeholder:text-zinc-500 transition-colors focus:border-zinc-500 focus:ring-1 focus:ring-zinc-200/80 dark:focus:ring-zinc-700/60 focus:outline-none"
                    placeholder="invoice, notes, report"
                    bind:value={searchForm.query}
                />
            </section>

            <section class="flex flex-col gap-2 rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 p-3">
                <div class="flex items-center justify-between">
                    <span class="text-[11px] font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400">Scope</span>
                </div>

                <div class="flex flex-wrap gap-1.5">
                    <button
                        type="button"
                        class="rounded-full border px-2 py-1 text-[11px] font-semibold tracking-[0.01em] transition-colors {searchForm.scopeMode ===
                        'all'
                            ? 'border-zinc-900 bg-zinc-900 text-white dark:border-zinc-100 dark:bg-zinc-100 dark:text-zinc-900'
                            : 'border-zinc-300 bg-white text-zinc-600 hover:bg-zinc-50 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100'}"
                        onclick={() => {
                            searchForm.scopeMode = "all";
                        }}
                    >
                        All Drives
                    </button>
                    <button
                        type="button"
                        class="rounded-full border px-2 py-1 text-[11px] font-semibold tracking-[0.01em] transition-colors {searchForm.scopeMode ===
                        'drive'
                            ? 'border-zinc-900 bg-zinc-900 text-white dark:border-zinc-100 dark:bg-zinc-100 dark:text-zinc-900'
                            : 'border-zinc-300 bg-white text-zinc-600 hover:bg-zinc-50 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100'}"
                        onclick={() => {
                            searchForm.scopeMode = "drive";
                            ensureDriveScopeSelection();
                        }}
                    >
                        One Drive
                    </button>
                    <button
                        type="button"
                        class="rounded-full border px-2 py-1 text-[11px] font-semibold tracking-[0.01em] transition-colors {searchForm.scopeMode ===
                        'folder'
                            ? 'border-zinc-900 bg-zinc-900 text-white dark:border-zinc-100 dark:bg-zinc-100 dark:text-zinc-900'
                            : 'border-zinc-300 bg-white text-zinc-600 hover:bg-zinc-50 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100'}"
                        onclick={() => {
                            searchForm.scopeMode = "folder";
                        }}
                    >
                        Folder
                    </button>
                </div>

                {#if searchForm.scopeMode === "drive"}
                    <ChipSelect
                        containerClass="w-full"
                        ariaLabel="Drive scope"
                        bind:value={searchForm.scopeDrive}
                        options={driveOptions}
                    />
                {/if}

                {#if searchForm.scopeMode === "folder"}
                    <div class="flex flex-col gap-2">
                        <button
                            type="button"
                            class="inline-flex h-[30px] items-center justify-center gap-1 rounded-md border border-zinc-300 bg-white px-2 text-xs font-semibold text-zinc-700 hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:hover:bg-zinc-800"
                            onclick={pickScopeFolders}
                            aria-label="Browse for folders"
                        >
                            <FolderOpen size={13} strokeWidth={2} />
                            Browse folders
                        </button>

                        {#if searchForm.scopeFolders.length === 0}
                            <p class="text-[11px] text-zinc-400 dark:text-zinc-500">
                                No folders selected
                            </p>
                        {:else}
                            <div class="max-h-32 overflow-auto rounded-md border border-zinc-300 bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900">
                                {#each searchForm.scopeFolders as folderPath}
                                    <div class="flex items-start justify-between gap-2 border-b border-zinc-200 dark:border-zinc-800 px-2 py-1.5 last:border-b-0">
                                        <span class="break-all text-[11px] text-zinc-500 dark:text-zinc-300"
                                            >{displayPath(folderPath)}</span
                                        >
                                        <button
                                            type="button"
                                            class="inline-flex items-center justify-center text-zinc-400 dark:text-zinc-500 hover:text-red-500"
                                            onclick={() =>
                                                removeScopeFolder(folderPath)}
                                            aria-label="Remove selected folder"
                                        >
                                            <X size={11} strokeWidth={2} />
                                        </button>
                                    </div>
                                {/each}
                            </div>
                        {/if}
                    </div>
                {/if}
            </section>

            <section class="flex flex-col gap-2 rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 p-3">
                <span class="text-[11px] font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400">Type</span>
                <div class="flex flex-wrap gap-1.5">
                    <button
                        type="button"
                        class="rounded-full border px-2 py-1 text-[11px] font-semibold tracking-[0.01em] transition-colors {searchForm.kind ===
                        'any'
                            ? 'border-zinc-900 bg-zinc-900 text-white dark:border-zinc-100 dark:bg-zinc-100 dark:text-zinc-900'
                            : 'border-zinc-300 bg-white text-zinc-600 hover:bg-zinc-50 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100'}"
                        onclick={() => {
                            searchForm.kind = "any";
                        }}
                    >
                        Any
                    </button>
                    <button
                        type="button"
                        class="rounded-full border px-2 py-1 text-[11px] font-semibold tracking-[0.01em] transition-colors {searchForm.kind ===
                        'file'
                            ? 'border-zinc-900 bg-zinc-900 text-white dark:border-zinc-100 dark:bg-zinc-100 dark:text-zinc-900'
                            : 'border-zinc-300 bg-white text-zinc-600 hover:bg-zinc-50 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100'}"
                        onclick={() => {
                            searchForm.kind = "file";
                        }}
                    >
                        Files
                    </button>
                    <button
                        type="button"
                        class="rounded-full border px-2 py-1 text-[11px] font-semibold tracking-[0.01em] transition-colors {searchForm.kind ===
                        'folder'
                            ? 'border-zinc-900 bg-zinc-900 text-white dark:border-zinc-100 dark:bg-zinc-100 dark:text-zinc-900'
                            : 'border-zinc-300 bg-white text-zinc-600 hover:bg-zinc-50 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100'}"
                        onclick={() => {
                            searchForm.kind = "folder";
                        }}
                    >
                        Folders
                    </button>
                </div>

                <div class="flex flex-wrap gap-1.5">
                    <button
                        type="button"
                        aria-pressed={searchForm.includeHidden}
                        class={`form-toggle ${searchForm.includeHidden ? "on" : ""}`}
                        onclick={() => {
                            searchForm.includeHidden = !searchForm.includeHidden;
                        }}
                    >
                        <span
                            class={`form-toggle-indicator ${searchForm.includeHidden ? "on" : ""}`}
                        ></span>
                        <span class="form-toggle-label">Hidden</span>
                    </button>

                    <button
                        type="button"
                        aria-pressed={searchForm.readonlyOnly}
                        class={`form-toggle ${searchForm.readonlyOnly ? "on" : ""}`}
                        onclick={() => {
                            searchForm.readonlyOnly = !searchForm.readonlyOnly;
                        }}
                    >
                        <span
                            class={`form-toggle-indicator ${searchForm.readonlyOnly ? "on" : ""}`}
                        ></span>
                        <span class="form-toggle-label">Read only</span>
                    </button>
                </div>
            </section>

            <section class="flex flex-col gap-2 rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 p-3">
                <button
                    type="button"
                    class="flex w-full items-center justify-between text-zinc-500 dark:text-zinc-300"
                    onclick={() => {
                        showAdvanced = !showAdvanced;
                    }}
                >
                    <span class="text-xs font-semibold tracking-[0.02em] text-zinc-700 dark:text-zinc-200">Advanced</span>
                    <ChevronDown
                        size={14}
                        strokeWidth={2}
                        class={showAdvanced ? "" : "rotate-[-90deg]"}
                    />
                </button>

                {#if showAdvanced}
                    <div class="grid gap-2">
                        <label class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500" for="ext-input">
                            Extensions
                        </label>
                        <div class="flex flex-wrap gap-1.5">
                            {#each POPULAR_EXTENSION_OPTIONS as option}
                                <button
                                    type="button"
                                    class={`rounded-full border px-2 py-1 text-[10px] font-semibold tracking-[0.01em] transition-colors ${selectedExtensionTokens.includes(option.value)
                                        ? "border-zinc-900 bg-zinc-900 text-white dark:border-zinc-100 dark:bg-zinc-100 dark:text-zinc-900"
                                        : "border-zinc-300 bg-white text-zinc-600 hover:bg-zinc-50 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"}`}
                                    aria-pressed={selectedExtensionTokens.includes(
                                        option.value,
                                    )}
                                    onclick={() =>
                                        togglePopularExtension(option.value)}
                                >
                                    {option.label}
                                </button>
                            {/each}
                        </div>
                        <input
                            id="ext-input"
                            type="text"
                            class="h-[30px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:placeholder:text-zinc-500 transition-colors focus:border-zinc-500 focus:ring-1 focus:ring-zinc-200/80 dark:focus:ring-zinc-700/60 focus:outline-none"
                            placeholder="Type custom extensions: rs, .svelte, toml"
                            bind:value={searchForm.extensionInput}
                            onblur={normalizeExtensionInput}
                        />

                        {#if selectedExtensionTokens.length > 0}
                            <div class="flex flex-wrap gap-1">
                                {#each selectedExtensionTokens as ext}
                                    <button
                                        type="button"
                                        class="inline-flex items-center gap-1 rounded-full border border-zinc-300 bg-zinc-100 px-2 py-0.5 text-[11px] font-semibold text-zinc-700 transition-colors hover:bg-zinc-200 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700"
                                        onclick={() => removeExtensionToken(ext)}
                                        aria-label={`Remove extension ${ext}`}
                                    >
                                        {ext}
                                        <X size={10} strokeWidth={2} />
                                    </button>
                                {/each}
                            </div>
                        {/if}

                        <label class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500" for="path-contains-input">
                            Path Contains
                        </label>
                        <input
                            id="path-contains-input"
                            type="text"
                            class="h-[30px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:placeholder:text-zinc-500 transition-colors focus:border-zinc-500 focus:ring-1 focus:ring-zinc-200/80 dark:focus:ring-zinc-700/60 focus:outline-none"
                            placeholder="src, workspace, backup"
                            bind:value={searchForm.pathContainsInput}
                        />

                        <label class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500" for="path-prefix-input">
                            Path Prefix
                        </label>
                        <input
                            id="path-prefix-input"
                            type="text"
                            class="h-[30px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:placeholder:text-zinc-500 transition-colors focus:border-zinc-500 focus:ring-1 focus:ring-zinc-200/80 dark:focus:ring-zinc-700/60 focus:outline-none"
                            placeholder="C:/Users/me/Projects"
                            bind:value={searchForm.pathPrefix}
                        />

                        <span class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500">Size Range</span>
                        <div class="grid grid-cols-[1fr_auto_1fr_auto] gap-1.5">
                            <input
                                type="number"
                                min="0"
                                class="h-[30px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 transition-colors focus:border-zinc-500 focus:ring-1 focus:ring-zinc-200/80 dark:focus:ring-zinc-700/60 focus:outline-none"
                                placeholder="Min"
                                bind:value={searchForm.sizeMin}
                            />
                            <ChipSelect
                                containerClass="w-[54px] shrink-0"
                                ariaLabel="Minimum size unit"
                                bind:value={searchForm.sizeMinUnit}
                                options={SIZE_UNIT_OPTIONS}
                            />
                            <input
                                type="number"
                                min="0"
                                class="h-[30px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 transition-colors focus:border-zinc-500 focus:ring-1 focus:ring-zinc-200/80 dark:focus:ring-zinc-700/60 focus:outline-none"
                                placeholder="Max"
                                bind:value={searchForm.sizeMax}
                            />
                            <ChipSelect
                                containerClass="w-[54px] shrink-0"
                                ariaLabel="Maximum size unit"
                                bind:value={searchForm.sizeMaxUnit}
                                options={SIZE_UNIT_OPTIONS}
                            />
                        </div>

                        <span class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500">Modified</span>
                        <div class="grid grid-cols-2 gap-1.5">
                            <CalendarField
                                ariaLabel="Modified from"
                                placeholder="From"
                                bind:value={searchForm.modifiedFrom}
                            />
                            <CalendarField
                                ariaLabel="Modified to"
                                placeholder="To"
                                bind:value={searchForm.modifiedTo}
                            />
                        </div>

                        <span class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500">Created</span>
                        <div class="grid grid-cols-2 gap-1.5">
                            <CalendarField
                                ariaLabel="Created from"
                                placeholder="From"
                                bind:value={searchForm.createdFrom}
                            />
                            <CalendarField
                                ariaLabel="Created to"
                                placeholder="To"
                                bind:value={searchForm.createdTo}
                            />
                        </div>
                    </div>
                {/if}
            </section>
        </div>

        {#if hasContradiction}
            <div class="flex flex-col gap-1 rounded-md border border-red-300 bg-red-50 dark:border-red-900 dark:bg-red-950/40 p-2">
                {#each validationIssues as issue}
                    <p class="text-xs text-red-600 dark:text-red-300 leading-snug">
                        {issue.message}
                    </p>
                {/each}
            </div>
        {/if}

    </div>

    <!-- Main panel -->
    <div class="h-full flex-1 min-w-0 flex flex-col">
        <!-- Header -->
        <div class="border-b border-zinc-200 dark:border-zinc-800 flex flex-col">
            {#if !searching && !searched}
                <div class="px-3 py-2">
                    <span class="text-xs text-zinc-400 dark:text-zinc-500">
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
                    <span class="text-xs text-zinc-400 dark:text-zinc-500">
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
                <div class="px-3 pb-2 flex items-center justify-between gap-3">
                    <div class="flex items-center gap-3 min-w-0">
                        <span class="text-xs text-zinc-500 dark:text-zinc-400">
                            Total scanned: {driveScanTotal} folder{driveScanTotal ===
                            1
                                ? ""
                                : "s"}
                        </span>
                    </div>

                    <div
                        class="ml-auto shrink-0 inline-flex items-center gap-1.5 rounded-md border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 dark:text-zinc-300 px-2 py-1 text-xs text-zinc-600"
                        title={searching
                            ? "Elapsed search time"
                            : "Last search duration"}
                    >
                        <Clock3 size={12} strokeWidth={2} class="text-zinc-500 dark:text-zinc-400" />
                        <span class="font-medium text-zinc-700 dark:text-zinc-100 tabular-nums"
                            >{searchDurationLabel || "--"}</span
                        >
                    </div>
                </div>

                <div
                    class="flex w-full overflow-hidden border-t border-zinc-200 dark:border-zinc-800 bg-zinc-100/80 dark:bg-zinc-900"
                >
                    {#each driveScanRows as row, i}
                        <div
                            class="h-8 flex-1 min-w-0 flex items-center justify-between px-6 text-[11px] {row.active
                                ? 'bg-zinc-50 dark:bg-zinc-900'
                                : 'bg-zinc-100 dark:bg-zinc-800'} {i < driveScanRows.length - 1
                                ? 'border-r border-zinc-200 dark:border-zinc-800'
                                : ''}"
                        >
                            <div class="flex items-center gap-1 min-w-0">
                                <HardDrive
                                    size={12}
                                    class="text-zinc-500 dark:text-zinc-400 shrink-0"
                                    strokeWidth={2}
                                />
                                <span class="text-zinc-600 dark:text-zinc-200 font-medium truncate"
                                    >{row.label}</span
                                >
                            </div>
                            <span class="text-zinc-500 dark:text-zinc-400 whitespace-nowrap">
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
                    <span class="text-sm text-zinc-400 dark:text-zinc-500">No results yet.</span>
                </div>
            {:else if treeRows.length === 0}
                <div
                    class="flex items-center justify-center h-full"
                    transition:fade={{ duration: 180 }}
                >
                    <span
                        class="text-sm text-zinc-400 dark:text-zinc-500 {searching
                            ? 'animate-pulse'
                            : ''}"
                    >
                        {searching
                            ? "Searching..."
                            : "No files matched your criteria."}
                    </span>
                </div>
            {:else}
                <div
                    class="h-full rounded-lg border border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-900 overflow-auto"
                    transition:fade={{ duration: 220 }}
                >
                    {#each treeRows as row (row.node.path)}
                        {#if row.node.isDir}
                            <button
                                class="w-full flex items-center gap-2 py-2 pr-3 text-left border-b border-zinc-100 dark:border-zinc-800 hover:bg-zinc-50 dark:hover:bg-zinc-800/70 {rowIndentClass(row.depth, 'dir')}"
                                onclick={() =>
                                    toggleDirectory(row.node.path, row.depth)}
                                title={displayPath(row.node.path)}
                            >
                                <span
                                    class="w-3 text-center text-xs text-zinc-500 dark:text-zinc-400"
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
                                    class="text-zinc-500 dark:text-zinc-400"
                                    strokeWidth={2}
                                />
                                <span class="text-xs text-zinc-700 dark:text-zinc-100 font-medium"
                                    >{row.node.name}</span
                                >
                                {#if isFolderScanning(row.node.path)}
                                    <LoaderCircle
                                        size={12}
                                        class="animate-spin text-emerald-600"
                                        strokeWidth={2}
                                    />
                                {/if}
                                <span class="text-xs text-zinc-400 dark:text-zinc-500 truncate"
                                    >{displayPath(row.node.path)}</span
                                >
                            </button>
                        {:else}
                            <button
                                class="w-full flex items-center gap-2 py-2 pr-3 text-left border-b border-zinc-100 dark:border-zinc-800 hover:bg-zinc-50 dark:hover:bg-zinc-800/70 {rowIndentClass(row.depth, 'file')}"
                                onclick={() => openInExplorer(row.node.path)}
                                title={displayPath(row.node.path)}
                            >
                                <File
                                    size={14}
                                    class="text-zinc-500 dark:text-zinc-400"
                                    strokeWidth={2}
                                />
                                <span class="text-xs text-zinc-700 dark:text-zinc-100 font-medium"
                                    >{row.node.name}</span
                                >
                                <span class="text-xs text-zinc-400 dark:text-zinc-500 truncate"
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
    .form-toggle {
        display: inline-flex;
        align-items: center;
        gap: 0.45rem;
        border-radius: 999px;
        border: 1px solid var(--control-border);
        background: var(--control-bg);
        color: var(--control-text);
        padding: 0.3rem 0.62rem;
        font-size: 11px;
        font-weight: 600;
        transition: background-color 0.15s ease, border-color 0.15s ease, color 0.15s ease;
    }

    .form-toggle:hover {
        background: var(--control-bg-hover);
    }

    .form-toggle.on {
        border-color: color-mix(in srgb, var(--accent) 45%, var(--control-border));
    }

    .form-toggle-label {
        letter-spacing: 0.01em;
    }

    .form-toggle-indicator {
        width: 8px;
        height: 8px;
        border-radius: 999px;
        background: #71717a;
        transition: background-color 0.15s ease, box-shadow 0.15s ease;
    }

    .form-toggle-indicator.on {
        background: var(--accent);
        box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 25%, transparent);
    }

    .sidebar-scroll {
        -ms-overflow-style: none;
        scrollbar-width: none;
    }

    .sidebar-scroll::-webkit-scrollbar {
        display: none;
    }
</style>

