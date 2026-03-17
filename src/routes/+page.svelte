<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { open } from "@tauri-apps/plugin-dialog";
    import { onMount } from "svelte";
    import {
        AlertTriangle,
        ChevronDown,
        ChevronRight,
        File,
        Folder,
        FolderOpen,
        LoaderCircle,
        Plus,
        Search,
        X,
    } from "lucide-svelte";

    type FilterType =
        | "extension"
        | "name_contains"
        | "path_contains"
        | "subfolder"
        | "size_gt"
        | "size_lt"
        | "modified_after"
        | "modified_before"
        | "created_after"
        | "created_before"
        | "drive"
        | "hidden"
        | "readonly"
        | "file_only"
        | "folder_only";

    interface Filter {
        id: number;
        type: FilterType;
        value: string;
        unit?: string;
    }

    interface FilterMeta {
        label: string;
        placeholder?: string;
        hasValue: boolean;
        isSize?: boolean;
    }

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

    const MAX_RESULTS = 10_000;
    const SEARCH_THREAD_LIMIT = 6;
    const FOLDER_BATCH_SIZE = 24;

    const filterMeta: Record<FilterType, FilterMeta> = {
        extension:       { label: "Extension",         placeholder: ".rs, .toml", hasValue: true },
        name_contains:   { label: "Name contains",     placeholder: "config",     hasValue: true },
        path_contains:   { label: "Path contains",     placeholder: "src/",       hasValue: true },
        subfolder:       { label: "Subfolder",                                     hasValue: true },
        size_gt:         { label: "Size greater than",                             hasValue: true, isSize: true },
        size_lt:         { label: "Size less than",                                hasValue: true, isSize: true },
        modified_after:  { label: "Modified after",                                hasValue: true },
        modified_before: { label: "Modified before",                               hasValue: true },
        created_after:   { label: "Created after",                                 hasValue: true },
        created_before:  { label: "Created before",                                hasValue: true },
        drive:           { label: "Drive",                                         hasValue: true },
        hidden:          { label: "Hidden files",                                  hasValue: false },
        readonly:        { label: "Read only",                                     hasValue: false },
        file_only:       { label: "Files only",                                    hasValue: false },
        folder_only:     { label: "Folders only",                                  hasValue: false },
    };

    let filters = $state<Filter[]>([]);
    let nextId = $state(0);
    let results = $state<FileEntry[]>([]);
    let searching = $state(false);
    let searched = $state(false);
    let searchStatus = $state("");
    let availableRoots = $state<string[]>([]);
    let openDirectories = $state<Record<string, boolean>>({});

    const query = $derived({
        filters: filters.map(({ type, value, unit }) => ({
            type,
            ...(filterMeta[type].hasValue ? {
                value,
                ...(filterMeta[type].isSize ? { unit: unit ?? "B" } : {}),
            } : {}),
        })),
    });

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

        const stackable: FilterType[] = ["extension", "path_contains"];
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

    onMount(async () => {
        try {
            availableRoots = await invoke<string[]>("list_search_roots");
        } catch {
            availableRoots = [];
        }
    });

    // ── Actions ───────────────────────────────────────────────────────────────

    function addFilter() {
        filters.push({ id: nextId++, type: "extension", value: "" });
    }

    function removeFilter(id: number) {
        filters = filters.filter(f => f.id !== id);
    }

    function onFilterTypeChange(filter: Filter) {
        if (filter.type === "drive") {
            filter.value = filter.value || "ALL";
            return;
        }
        if (filter.type === "subfolder") {
            filter.value = filter.value || "";
            return;
        }
        if (!filterMeta[filter.type].hasValue) {
            filter.value = "";
        }
        if (filterMeta[filter.type].isSize && !filter.unit) {
            filter.unit = "B";
        }
    }

    async function pickSubfolder(filter: Filter) {
        const driveFilter = filters.find(f => f.type === "drive");
        const selectedDrive = (driveFilter?.value ?? "").trim();
        const defaultPath = filter.value || (selectedDrive && selectedDrive !== "ALL" ? selectedDrive : undefined);

        try {
            const selected = await open({
                directory: true,
                multiple: false,
                ...(defaultPath ? { defaultPath } : {}),
            });

            if (typeof selected === "string") {
                filter.value = selected;
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

    async function scanRootProgressively(
        root: string,
        scopedQuery: { filters: Array<{ type: string; value?: string; unit?: string }> },
        rootIndex: number,
        rootCount: number,
    ) {
        searchStatus = `Phase 1/2: ${root} (${rootIndex + 1}/${rootCount})`;
        const rootBatch = await invoke<FolderBatchResult>("search_folder_batch", {
            query: scopedQuery,
            folders: [root],
            limit: MAX_RESULTS - results.length,
            threadLimit: SEARCH_THREAD_LIMIT,
        });

        appendResults(rootBatch.entries);

        let scannedFolders = rootBatch.scanned_folders;
        const seenFolders = new Set<string>([root]);
        const queue: string[] = [];

        for (const next of rootBatch.next_folders) {
            if (!seenFolders.has(next)) {
                seenFolders.add(next);
                queue.push(next);
            }
        }

        while (queue.length > 0 && results.length < MAX_RESULTS) {
            const batchFolders = queue.splice(0, Math.min(FOLDER_BATCH_SIZE, queue.length));

            searchStatus = `Phase 2/2: ${root} | scanned ${scannedFolders} folders | queue ${queue.length + batchFolders.length}`;

            const batch = await invoke<FolderBatchResult>("search_folder_batch", {
                query: scopedQuery,
                folders: batchFolders,
                limit: MAX_RESULTS - results.length,
                threadLimit: SEARCH_THREAD_LIMIT,
            });

            appendResults(batch.entries);
            scannedFolders += batch.scanned_folders;

            for (const next of batch.next_folders) {
                if (!seenFolders.has(next)) {
                    seenFolders.add(next);
                    queue.push(next);
                }
            }
        }
    }

    async function search() {
        if (hasContradiction || searching) return;
        searching = true;
        searched = true;
        searchStatus = "Preparing roots...";
        results = [];
        openDirectories = {};
        try {
            const roots = availableRoots.length > 0
                ? availableRoots
                : await invoke<string[]>("list_search_roots");

            const driveFilter = filters.find(f => f.type === "drive");
            const selectedDrive = (driveFilter?.value ?? "ALL").trim();
            const subfolderFilter = filters.find(f => f.type === "subfolder");
            const selectedSubfolder = (subfolderFilter?.value ?? "").trim();

            const rootsToScan = selectedSubfolder
                ? [selectedSubfolder]
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

            for (let i = 0; i < rootsToScan.length; i++) {
                if (results.length >= MAX_RESULTS) break;
                await scanRootProgressively(rootsToScan[i], scopedQuery, i, rootsToScan.length);
            }

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
    <div class="w-75 h-full bg-zinc-100 border-r border-zinc-300 flex flex-col p-4 gap-3">

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
                            <div class="flex gap-1 w-full">
                                <input
                                    type="text"
                                    class="text-xs px-2 py-1 rounded border border-zinc-200 focus:outline-none focus:ring-1 focus:ring-zinc-400"
                                    style="width: calc(100% - 62px);"
                                    placeholder="Choose any folder path"
                                    bind:value={filter.value}
                                />
                                <button
                                    type="button"
                                    class="h-7 w-7 rounded border border-zinc-200 bg-white hover:bg-zinc-50 inline-flex items-center justify-center text-zinc-500"
                                    onclick={() => pickSubfolder(filter)}
                                    aria-label="Browse for folder"
                                    title="Browse"
                                >
                                    <FolderOpen size={13} strokeWidth={2} />
                                </button>
                                <button
                                    type="button"
                                    class="h-7 w-7 rounded border border-zinc-200 bg-white hover:bg-zinc-50 inline-flex items-center justify-center text-zinc-500"
                                    onclick={() => { filter.value = ""; }}
                                    aria-label="Clear folder path"
                                    title="Clear"
                                >
                                    <X size={12} strokeWidth={2} />
                                </button>
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

    </div>

    <!-- Main panel -->
    <div class="h-full flex flex-col" style="width: calc(100% - 300px)">

        <!-- Header -->
        <div class="px-6 py-3 border-b border-zinc-200 flex items-center justify-between">
            <span class="text-xs text-zinc-400">
                {#if searching}
                    {searchStatus || "Searching..."}
                {:else if searched}
                    {results.length} result{results.length === 1 ? "" : "s"}
                {:else}
                    Add filters and search
                {/if}
            </span>
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
        background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='6' height='3' viewBox='0 0 6 3'%3E%3Cpath d='M0 0l3 3 3-3z' fill='%2371717a'/%3E%3C/svg%3E");
        background-repeat: no-repeat;
        background-position: right 8px center;
        background-size: 12px 5px;
        background-color: white;
        border: 1px solid rgb(228 228 231);
        border-radius: 0.375rem;
        padding: 4px 24px 4px 8px;
        font-size: 0.75rem;
        color: rgb(82 82 91);
        cursor: pointer;
        width: 100%;
    }

    select:focus {
        outline: none;
        border-color: rgb(161 161 170);
    }
</style>