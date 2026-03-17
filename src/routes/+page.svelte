<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
    import { onMount } from "svelte";
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

    type FolderBatchResult = {
        entries: FileEntry[];
        next_folders: string[];
        scanned_folders: number;
    };

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
    const SEARCH_THREADS_PER_DRIVE = 2;
    const SEARCH_CONCURRENT_REQUESTS = 4;
    const FOLDER_SLICE_SIZE = 8;

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
    let driveScanOrder = $state<string[]>([]);

    const query = $derived(FilterModel.toQuery(filters));

    // ── Contradiction analysis ────────────────────────────────────────────────

    type Contradiction = { message: string; filters: number[] };

    function analyzeContradictions(filters: Filter[]): Contradiction[] {
        const contradictions: Contradiction[] = [];
        const get = (type: FilterType) => filters.filter(f => f.type === type);
        const toBytes = (value: string, unit: string = "B"): number => {
            const n = parseFloat(value);
            if (isNaN(n)) return -1;
            const map: Record<string, number> = { B: 1, KB: 1024, MB: 1024 ** 2, GB: 1024 ** 3 };
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

        const check = (afterType: FilterType, beforeType: FilterType, label: string) => {
            const after  = get(afterType);
            const before = get(beforeType);
            if (after.length && before.length) {
                const a = new Date(after[0].value);
                const b = new Date(before[0].value);
                if (!isNaN(a.getTime()) && !isNaN(b.getTime()) && a >= b) {
                    contradictions.push({
                        message: `"${label} after" must be earlier than "${label} before"`,
                        filters: [after[0].id, before[0].id],
                    });
                }
            }
        };

        check("modified_after", "modified_before", "Modified");
        check("created_after",  "created_before",  "Created");

        const fileOnly   = get("file_only");
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

    const resultTree = $derived(buildResultTree(results));
    const treeRows = $derived(flattenVisibleRows(resultTree));
    const driveScanTotal = $derived(
        Object.values(driveScanCounts).reduce((sum, value) => sum + value, 0)
    );
    const driveScanRows = $derived.by(() => {
        const labels = [...driveScanOrder.slice(0, 4)];
        while (labels.length < 4) {
            labels.push("");
        }

        return labels.map((label, index): DriveScanRow => {
            const active = label.length > 0;
            const scanned = active ? (driveScanCounts[label] ?? 0) : 0;

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

        window.addEventListener("bolt-save-filter", onSave);
        window.addEventListener("bolt-load-filter", onLoad);

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
        };
    });

    // ── Actions ───────────────────────────────────────────────────────────────

    function addFilter() {
        filters.push(FilterModel.create(nextId++));
    }

    function removeFilter(id: number) {
        filters = filters.filter(f => f.id !== id);
    }

    function onFilterTypeChange(filter: Filter) {
        FilterModel.applyTypeDefaults(filter);
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
        const remaining = subfolderPathsFor(filter).filter((p) => p !== pathToRemove);
        filter.value = encodeSubfolderPaths(remaining);
    }

    async function pickSubfolder(filter: Filter) {
        const driveFilter = filters.find(f => f.type === "drive");
        const selectedDrive = (driveFilter?.value ?? "").trim();
        const selectedFolders = subfolderPathsFor(filter);
        const defaultPath = selectedFolders[0] || (selectedDrive && selectedDrive !== "ALL" ? selectedDrive : undefined);

        try {
            const selected = await open({
                directory: true,
                multiple: true,
                ...(defaultPath ? { defaultPath } : {}),
            });

            if (Array.isArray(selected)) {
                filter.value = encodeSubfolderPaths([...selectedFolders, ...selected]);
            } else if (typeof selected === "string") {
                filter.value = encodeSubfolderPaths([...selectedFolders, selected]);
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

    function initializeDriveScanSlots(rootsToScan: string[]) {
        const drives = Array.from(new Set(rootsToScan.map(driveLabelFromPath))).slice(0, 4);
        driveScanOrder = drives;
        driveScanCounts = Object.fromEntries(drives.map((drive) => [drive, 0]));
    }

    function incrementDriveScanned(rootPath: string, scannedFolders: number) {
        if (scannedFolders <= 0) return;

        const drive = driveLabelFromPath(rootPath);
        if (!driveScanOrder.includes(drive) && driveScanOrder.length < 4) {
            driveScanOrder = [...driveScanOrder, drive];
        }

        driveScanCounts = {
            ...driveScanCounts,
            [drive]: (driveScanCounts[drive] ?? 0) + scannedFolders,
        };
    }

    function createMutableNode(name: string, path: string, isDir: boolean): MutableTreeNode {
        return { name, path, isDir, children: new Map() };
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

    function buildResultTree(entries: FileEntry[]): TreeNode[] {
        const roots = new Map<string, MutableTreeNode>();

        for (const entry of entries) {
            const normalized = normalizePath(entry.path);
            if (!normalized) continue;

            const segments = normalized.split("/").filter(Boolean);
            if (segments.length === 0) continue;

            let currentMap = roots;
            let currentPath = "";

            for (let i = 0; i < segments.length; i++) {
                const segment = segments[i];
                const isLast = i === segments.length - 1;
                currentPath = currentPath ? `${currentPath}/${segment}` : segment;

                let node = currentMap.get(segment);
                if (!node) {
                    node = createMutableNode(segment, currentPath, isLast ? entry.is_dir : true);
                    currentMap.set(segment, node);
                } else {
                    if (!isLast || entry.is_dir) {
                        node.isDir = true;
                    }
                }

                currentMap = node.children;
            }
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

    function flattenVisibleRows(nodes: TreeNode[], depth = 0, rows: TreeRow[] = []): TreeRow[] {
        for (const node of nodes) {
            const hasChildren = node.children.length > 0;
            const isOpen = node.isDir && hasChildren ? isDirectoryOpen(node.path, depth) : false;

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
        results = [...results, ...chunk.slice(0, remaining)];
    }

    type RootScanState = {
        root: string;
        queue: string[];
        seen: Set<string>;
        scannedFolders: number;
    };

    async function scanRootsInterleaved(
        rootsToScan: string[],
        scopedQuery: { filters: Array<{ type: string; value?: string; unit?: string }> },
    ) {
        const states: RootScanState[] = [];
        const totalThreadUpperBound = Math.max(1, rootsToScan.length * SEARCH_THREADS_PER_DRIVE);
        const concurrentWorkers = Math.max(1, Math.min(SEARCH_CONCURRENT_REQUESTS, rootsToScan.length));
        const perRequestThreadLimit = Math.max(
            1,
            Math.floor(totalThreadUpperBound / concurrentWorkers)
        );

        const phase1Workers = concurrentWorkers;
        let nextRootIndex = 0;

        const phase1Tasks = Array.from({ length: phase1Workers }, async () => {
            while (results.length < MAX_RESULTS) {
                const currentIndex = nextRootIndex;
                nextRootIndex += 1;

                if (currentIndex >= rootsToScan.length) {
                    return;
                }

                const root = rootsToScan[currentIndex];
                const rootsRemaining = rootsToScan.length - currentIndex;
                const perRootLimit = Math.max(1, Math.ceil((MAX_RESULTS - results.length) / rootsRemaining));

                searchStatus = `Phase 1/2: ${root} (${currentIndex + 1}/${rootsToScan.length})`;

                const rootBatch = await invoke<FolderBatchResult>("search_folder_batch", {
                    query: scopedQuery,
                    folders: [root],
                    limit: perRootLimit,
                    threadLimit: perRequestThreadLimit,
                });

                appendResults(rootBatch.entries);
                incrementDriveScanned(root, rootBatch.scanned_folders);

                const seen = new Set<string>([root]);
                const queue: string[] = [];

                for (const next of rootBatch.next_folders) {
                    if (!seen.has(next)) {
                        seen.add(next);
                        queue.push(next);
                    }
                }

                states.push({
                    root,
                    queue,
                    seen,
                    scannedFolders: rootBatch.scanned_folders,
                });
            }
        });

        await Promise.all(phase1Tasks);

        if (states.length === 0 || results.length >= MAX_RESULTS) {
            return;
        }

        let roundRobinIndex = 0;
        const getNextStateWithWork = (): RootScanState | null => {
            if (states.length === 0) return null;

            for (let i = 0; i < states.length; i++) {
                const idx = (roundRobinIndex + i) % states.length;
                const state = states[idx];
                if (state.queue.length > 0) {
                    roundRobinIndex = (idx + 1) % states.length;
                    return state;
                }
            }

            return null;
        };

        const phase2Workers = Math.max(1, Math.min(concurrentWorkers, states.length));
        const phase2Tasks = Array.from({ length: phase2Workers }, async () => {
            while (results.length < MAX_RESULTS) {
                const state = getNextStateWithWork();
                if (!state) {
                    return;
                }

                const batchFolders = state.queue.splice(0, Math.min(FOLDER_SLICE_SIZE, state.queue.length));
                if (batchFolders.length === 0) {
                    continue;
                }

                const activeRoots = Math.max(1, states.filter((s) => s.queue.length > 0).length);
                const perRootLimit = Math.max(1, Math.ceil((MAX_RESULTS - results.length) / activeRoots));

                searchStatus = `Phase 2/2: ${state.root} | scanned ${state.scannedFolders} folders`;

                const batch = await invoke<FolderBatchResult>("search_folder_batch", {
                    query: scopedQuery,
                    folders: batchFolders,
                    limit: perRootLimit,
                    threadLimit: perRequestThreadLimit,
                });

                appendResults(batch.entries);
                state.scannedFolders += batch.scanned_folders;
                incrementDriveScanned(state.root, batch.scanned_folders);

                for (const next of batch.next_folders) {
                    if (!state.seen.has(next)) {
                        state.seen.add(next);
                        state.queue.push(next);
                    }
                }
            }
        });

        await Promise.all(phase2Tasks);
    }

    async function search() {
        if (hasContradiction || searching) return;
        searching = true;
        searched = true;
        searchStatus = "Preparing roots...";
        results = [];
        openDirectories = {};
        driveScanCounts = {};
        driveScanOrder = [];
        try {
            const roots = availableRoots.length > 0
                ? availableRoots
                : await invoke<string[]>("list_search_roots");

            const driveFilter = filters.find(f => f.type === "drive");
            const selectedDrive = (driveFilter?.value ?? "ALL").trim();
            const selectedSubfolders = dedupePaths(
                filters
                    .filter((f) => f.type === "subfolder")
                    .flatMap((f) => parseSubfolderPaths(f.value))
            );

            const rootsToScan = selectedSubfolders.length > 0
                ? selectedSubfolders
                : selectedDrive && selectedDrive !== "ALL"
                ? roots.filter(r => r === selectedDrive)
                : roots;

            const scopedQuery = {
                filters: query.filters.filter(f => f.type !== "drive" && f.type !== "subfolder"),
            };

            if (rootsToScan.length === 0) {
                searchStatus = "No roots found";
                return;
            }

            initializeDriveScanSlots(rootsToScan);

            await scanRootsInterleaved(rootsToScan, scopedQuery);

            searchStatus = `Done (${results.length} result${results.length === 1 ? "" : "s"})`;
        } catch (e) {
            console.error("Search failed:", e);
            results = [];
            searchStatus = "Search failed";
        } finally {
            searching = false;
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
        driveScanOrder = [];
    }

    async function saveFilter() {
        try {
            const selectedPath = await saveDialog({
                title: "Save Filter",
                defaultPath: "bolt-filter.bsearch",
                filters: [{ name: "Bolt Search Filter", extensions: ["bsearch"] }],
            });

            if (typeof selectedPath !== "string" || !selectedPath.trim()) {
                return;
            }

            const payload = JSON.stringify(FilterModel.toSavedFile(filters), null, 2);
            await invoke("save_filter_file", { path: selectedPath, content: payload });
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
                filters: [{ name: "Bolt Search Filter", extensions: ["bsearch"] }],
            });

            if (typeof selectedPath !== "string" || !selectedPath.trim()) {
                return;
            }

            const content = await invoke<string>("load_filter_file", { path: selectedPath });
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
    <div class="sidebar-panel w-75 h-full bg-zinc-100 border-r border-zinc-300 flex flex-col p-4 gap-3">

        <span class="text-xs font-semibold text-zinc-400 uppercase tracking-widest">Bolt Search</span>

        <!-- Filter tokens -->
        <div class="flex flex-col gap-2 flex-1 overflow-y-auto">
            {#if filters.length === 0}
                <p class="text-xs text-zinc-400 text-center mt-4">No filters yet.</p>
            {/if}

            {#each filters as filter (filter.id)}
                <div class="flex flex-col gap-1 bg-white border border-zinc-200 rounded-md p-2">
                    <div class="flex items-center justify-between mb-1">
                        <select bind:value={filter.type} onchange={() => onFilterTypeChange(filter)}>
                            {#each Object.entries(filterMeta) as [val, meta]}
                                <option value={val}>{meta.label}</option>
                            {/each}
                        </select>
                        <button
                            class="ml-2 p-1 text-zinc-300 hover:text-red-500"
                            onclick={() => removeFilter(filter.id)}
                            aria-label="Remove filter"
                        >
                            <X size={12} strokeWidth={2} />
                        </button>
                    </div>

                    {#if filterMeta[filter.type].hasValue}
                        {#if filter.type === "drive"}
                            <select bind:value={filter.value}>
                                <option value="ALL">Global (all drives)</option>
                                {#each availableRoots as root}
                                    <option value={root}>{root}</option>
                                {/each}
                            </select>
                        {:else if filter.type === "subfolder"}
                            <div class="flex flex-col gap-1 w-full">
                                <input
                                    type="text"
                                    class="text-xs px-2 py-1 rounded border border-zinc-200 focus:outline-none focus:ring-1 focus:ring-zinc-400"
                                    value={subfolderPathsFor(filter).join(" | ") || "No folder selected"}
                                    placeholder="Select folder(s)"
                                    disabled
                                />
                                <div class="flex gap-1">
                                    <button
                                        type="button"
                                        class="h-7 flex-1 rounded border border-zinc-200 bg-white hover:bg-zinc-50 inline-flex items-center justify-center gap-1 text-zinc-600 text-xs"
                                        onclick={() => pickSubfolder(filter)}
                                        aria-label="Browse for folders"
                                        title="Browse"
                                    >
                                        <FolderOpen size={13} strokeWidth={2} />
                                        Browse
                                    </button>
                                    <button
                                        type="button"
                                        class="h-7 w-7 rounded border border-zinc-200 bg-white hover:bg-zinc-50 inline-flex items-center justify-center text-zinc-500"
                                        onclick={() => { filter.value = ""; }}
                                        aria-label="Clear selected folders"
                                        title="Clear"
                                    >
                                        <X size={12} strokeWidth={2} />
                                    </button>
                                </div>

                                {#if subfolderPathsFor(filter).length > 0}
                                    <div class="max-h-24 overflow-auto border border-zinc-200 rounded-md bg-zinc-50">
                                        {#each subfolderPathsFor(filter) as folderPath}
                                            <div class="px-2 py-1 text-[11px] text-zinc-600 border-b border-zinc-200 last:border-b-0 flex items-start justify-between gap-2">
                                                <span class="break-all leading-snug">{displayPath(folderPath)}</span>
                                                <button
                                                    type="button"
                                                    class="text-zinc-400 hover:text-red-500 shrink-0"
                                                    onclick={() => removeSubfolderPath(filter, folderPath)}
                                                    aria-label="Remove selected folder"
                                                >
                                                    <X size={11} strokeWidth={2} />
                                                </button>
                                            </div>
                                        {/each}
                                    </div>
                                {/if}
                            </div>
                        {:else if filter.type.includes("modified") || filter.type.includes("created")}
                            <input
                                type="date"
                                class="text-xs px-2 py-1 rounded border border-zinc-200 focus:outline-none focus:ring-1 focus:ring-zinc-400"
                                bind:value={filter.value}
                            />
                        {:else if filterMeta[filter.type].isSize}
                            <div class="flex gap-1 w-full">
                                <input
                                    type="number"
                                    min="0"
                                    class="text-xs px-2 py-1 rounded border border-zinc-200 focus:outline-none focus:ring-1 focus:ring-zinc-400"
                                    style="width: calc(100% - 52px);"
                                    placeholder="0"
                                    bind:value={filter.value}
                                />
                                <select
                                    style="width: 48px; shrink: 0; padding-right: 4px; background-image: none;"
                                    onchange={(e) => {
                                        filter.unit = (e.target as HTMLSelectElement).value;
                                    }}
                                >
                                    <option value="B">B</option>
                                    <option value="KB">KB</option>
                                    <option value="MB">MB</option>
                                    <option value="GB">GB</option>
                                </select>
                            </div>
                        {:else}
                            <input
                                type="text"
                                class="text-xs px-2 py-1 rounded border border-zinc-200 focus:outline-none focus:ring-1 focus:ring-zinc-400"
                                placeholder={filterMeta[filter.type].placeholder ?? "value..."}
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

        <!-- Buttons -->
        <button
            class="w-full py-2 text-sm rounded-md border border-zinc-300 bg-white hover:bg-zinc-50 text-zinc-600 flex items-center justify-center gap-2"
            onclick={addFilter}
        >
            <Plus size={14} strokeWidth={2} />
            Add Filter
        </button>

        <button
            class="w-full py-2 text-sm rounded-md font-medium transition-colors text-white flex items-center justify-center gap-2
                {hasContradiction ? 'bg-red-500 hover:bg-red-600' : 'bg-zinc-800 hover:bg-zinc-700'}"
            onclick={search}
            disabled={hasContradiction || searching}
        >
            {#if hasContradiction}
                <AlertTriangle size={14} strokeWidth={2} />
                Contradiction
            {:else if searching}
                <LoaderCircle size={14} class="animate-spin" strokeWidth={2} />
                Searching...
            {:else}
                <Search size={14} strokeWidth={2} />
                Search
            {/if}
        </button>

        <button
            class="w-full py-2 text-sm rounded-md border border-zinc-300 bg-white hover:bg-zinc-50 text-zinc-600 flex items-center justify-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
            onclick={clearSearchResults}
            disabled={searching || (!searched && results.length === 0)}
        >
            <Trash2 size={14} strokeWidth={2} />
            Clear Results
        </button>

    </div>

    <!-- Main panel -->
    <div class="h-full flex flex-col" style="width: calc(100% - 300px)">

        <!-- Header -->
        <div class="border-b border-zinc-200 px-4 py-2 flex flex-col gap-1.5">
            <span class="text-xs text-zinc-400">
                {#if searching}
                    {searchStatus || "Searching..."}
                {:else if searched}
                    {results.length} result{results.length === 1 ? "" : "s"}
                {:else}
                    Search Status Bar - Empty
                {/if}
            </span>

            {#if searching || searched}
                <span class="text-xs text-zinc-500">
                    Total scanned: {driveScanTotal} folder{driveScanTotal === 1 ? "" : "s"}
                </span>

                <div class="flex w-full overflow-hidden rounded-md border border-zinc-300 bg-zinc-100">
                    {#each driveScanRows as row, i}
                        <div class="h-8 flex-1 min-w-0 flex items-center justify-between px-3 text-[11px] {row.active ? 'bg-zinc-50' : 'bg-zinc-100'} {i < driveScanRows.length - 1 ? 'border-r border-zinc-300' : ''}">
                            <div class="flex items-center gap-1 min-w-0">
                                <HardDrive size={12} class="text-zinc-500 shrink-0" strokeWidth={2} />
                                <span class="text-zinc-600 font-medium truncate">{row.label}</span>
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
            {#if searching}
                <div class="flex items-center justify-center h-full">
                    <span class="text-sm text-zinc-400 animate-pulse">Searching...</span>
                </div>
            {:else if searched && results.length === 0}
                <div class="flex items-center justify-center h-full">
                    <span class="text-sm text-zinc-400">No files matched your filters.</span>
                </div>
            {:else if !searched}
                <div class="flex items-center justify-center h-full">
                    <span class="text-sm text-zinc-400">No results yet.</span>
                </div>
            {:else}
                <div class="h-full rounded-lg border border-zinc-200 bg-white overflow-auto">
                    {#each treeRows as row (row.node.path)}
                        {#if row.node.isDir}
                            <button
                                class="w-full flex items-center gap-2 py-2 pr-3 text-left border-b border-zinc-100 hover:bg-zinc-50"
                                style={`padding-left: ${8 + row.depth * 18}px;`}
                                onclick={() => toggleDirectory(row.node.path, row.depth)}
                                title={displayPath(row.node.path)}
                            >
                                <span class="text-xs text-zinc-500" style="width: 12px; text-align: center;">
                                    {#if row.hasChildren}
                                        {#if row.isOpen}
                                            <ChevronDown size={12} strokeWidth={2} />
                                        {:else}
                                            <ChevronRight size={12} strokeWidth={2} />
                                        {/if}
                                    {/if}
                                </span>
                                <Folder size={14} class="text-zinc-500" strokeWidth={2} />
                                <span class="text-xs text-zinc-700 font-medium">{row.node.name}</span>
                                <span class="text-xs text-zinc-400 truncate">{displayPath(row.node.path)}</span>
                            </button>
                        {:else}
                            <button
                                class="w-full flex items-center gap-2 py-2 pr-3 text-left border-b border-zinc-100 hover:bg-zinc-50"
                                style={`padding-left: ${20 + row.depth * 18}px;`}
                                onclick={() => openInExplorer(row.node.path)}
                                title={displayPath(row.node.path)}
                            >
                                <File size={14} class="text-zinc-500" strokeWidth={2} />
                                <span class="text-xs text-zinc-700 font-medium">{row.node.name}</span>
                                <span class="text-xs text-zinc-400 truncate">{displayPath(row.node.path)}</span>
                            </button>
                        {/if}
                    {/each}
                </div>
            {/if}
        </div>

    </div>
</div>

<style>
    select {
        appearance: none;
        -webkit-appearance: none;
        -moz-appearance: none;
        background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='8' height='5' viewBox='0 0 8 5'%3E%3Cpath d='M0 0l4 5 4-5z' fill='%2371717a'/%3E%3C/svg%3E");
        background-repeat: no-repeat;
        background-position: right 8px center;
        background-size: 10px 6px;
        background-color: var(--control-bg);
        border: 1px solid var(--control-border);
        border-radius: 0.375rem;
        padding: 4px 24px 4px 8px;
        font-size: 0.75rem;
        color: var(--control-text);
        cursor: pointer;
        width: 100%;
    }

    select:focus {
        outline: none;
        border-color: var(--focus-ring);
        box-shadow: 0 0 0 1px var(--focus-ring);
    }

    :global(html[data-theme='dark']) select {
        background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='8' height='5' viewBox='0 0 8 5'%3E%3Cpath d='M0 0l4 5 4-5z' fill='%23d4d4d8'/%3E%3C/svg%3E");
    }

    :global(html[data-theme='dark']) select option {
        background: #252526;
        color: #d4d4d4;
    }
</style>