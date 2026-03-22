<script lang="ts">
    import { convertFileSrc, invoke } from "@tauri-apps/api/core";
    import { fade } from "svelte/transition";
    import {
        ArrowLeft,
        ChevronDown,
        ChevronRight,
        Clock3,
        Eye,
        File,
        Folder,
        HardDrive,
        LoaderCircle,
        X,
    } from "lucide-svelte";
    import type { DriveScanRow, FileEntry, TreeNode, TreeRow } from "../search/page-types";

    type PreviewKind = "image" | "text" | "pdf" | "audio" | "video" | "unsupported";
    type ExplorerLayoutMode = "default" | "focus";
    type FolderPreviewResult = {
        entries: FileEntry[];
        next_folders: string[];
        scanned_folders: number;
    };

    let {
        searching,
        searched,
        layoutMode,
        searchStatus,
        resultsLength,
        driveScanTotal,
        searchDurationLabel,
        driveScanRows,
        treeRows,
        rowIndentClass,
        displayPath,
        isFolderScanning,
        isFolderEmpty,
        focusedFolderPath,
        focusEntries,
        toggleDirectory,
        focusFolder,
        openInExplorer,
    }: {
        searching: boolean;
        searched: boolean;
        layoutMode: ExplorerLayoutMode;
        searchStatus: string;
        resultsLength: number;
        driveScanTotal: number;
        searchDurationLabel: string;
        driveScanRows: DriveScanRow[];
        treeRows: TreeRow[];
        rowIndentClass: (depth: number, kind: "dir" | "file") => string;
        displayPath: (path: string) => string;
        isFolderScanning: (path: string) => boolean;
        isFolderEmpty: (path: string) => boolean;
        focusedFolderPath: string | null;
        focusEntries: TreeNode[];
        toggleDirectory: (path: string, depth: number) => void | Promise<void>;
        focusFolder: (path: string | null) => void | Promise<void>;
        openInExplorer: (path: string) => Promise<void>;
    } = $props();

    let previewModalOpen = $state(false);
    let previewTitle = $state("");
    let previewPath = $state("");
    let previewKind = $state<PreviewKind>("unsupported");
    let previewSrc = $state("");
    let previewText = $state("");
    let previewLoading = $state(false);
    let previewError = $state("");
    let folderPreviewModalOpen = $state(false);
    let folderPreviewTitle = $state("");
    let folderPreviewPath = $state("");
    let folderPreviewLoading = $state(false);
    let folderPreviewError = $state("");
    let folderPreviewEntries = $state<FileEntry[]>([]);
    let focusQuery = $state("");

    const filteredFocusEntries = $derived.by(() => {
        const query = focusQuery.trim().toLowerCase();
        if (!query) {
            return focusEntries;
        }

        return focusEntries.filter((entry) =>
            entry.name.toLowerCase().includes(query),
        );
    });

    const canGoBackInFocus = $derived(
        layoutMode === "focus" && !!focusedFolderPath,
    );

    const IMAGE_EXTENSIONS = new Set([
        "png",
        "jpg",
        "jpeg",
        "gif",
        "webp",
        "bmp",
        "svg",
    ]);
    const TEXT_EXTENSIONS = new Set([
        "txt",
        "md",
        "json",
        "js",
        "ts",
        "tsx",
        "jsx",
        "css",
        "html",
        "xml",
        "csv",
        "log",
        "toml",
        "yaml",
        "yml",
        "rs",
        "py",
        "java",
        "c",
        "cpp",
        "h",
        "hpp",
        "svelte",
    ]);
    const AUDIO_EXTENSIONS = new Set(["mp3", "wav", "ogg", "m4a", "flac"]);
    const VIDEO_EXTENSIONS = new Set(["mp4", "webm", "ogv", "mov", "m4v"]);

    function extensionFromPath(path: string): string {
        const dot = path.lastIndexOf(".");
        if (dot === -1 || dot === path.length - 1) return "";
        return path.slice(dot + 1).toLowerCase();
    }

    function detectPreviewKind(path: string): PreviewKind {
        const ext = extensionFromPath(path);
        if (!ext) return "unsupported";
        if (IMAGE_EXTENSIONS.has(ext)) return "image";
        if (TEXT_EXTENSIONS.has(ext)) return "text";
        if (ext === "pdf") return "pdf";
        if (AUDIO_EXTENSIONS.has(ext)) return "audio";
        if (VIDEO_EXTENSIONS.has(ext)) return "video";
        return "unsupported";
    }

    function parentPath(path: string | null): string | null {
        if (!path) return null;
        const normalized = path.replace(/\\/g, "/").replace(/\/+/g, "/");
        const parts = normalized.split("/").filter(Boolean);
        if (parts.length <= 1) return null;
        return parts.slice(0, -1).join("/");
    }

    async function goBackInFocus(): Promise<void> {
        const nextPath = parentPath(focusedFolderPath);
        await focusFolder(nextPath);
    }

    function resetPreviewState(): void {
        previewText = "";
        previewError = "";
        previewLoading = false;
        previewSrc = "";
    }

    function closePreview(): void {
        previewModalOpen = false;
        resetPreviewState();
    }

    function closeFolderPreview(): void {
        folderPreviewModalOpen = false;
        folderPreviewError = "";
        folderPreviewLoading = false;
        folderPreviewEntries = [];
    }

    async function openFolderPreview(path: string, name: string): Promise<void> {
        folderPreviewModalOpen = true;
        folderPreviewPath = path;
        folderPreviewTitle = name;
        folderPreviewLoading = true;
        folderPreviewError = "";
        folderPreviewEntries = [];

        try {
            const preview = await invoke<FolderPreviewResult>("search_folder_batch", {
                query: { filters: [] },
                folders: [path],
                limit: 500,
                threadLimit: 4,
            });

            folderPreviewEntries = preview.entries
                .slice()
                .sort((a, b) => {
                    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
                    return a.name.localeCompare(b.name);
                });
        } catch (error) {
            folderPreviewError = error instanceof Error
                ? error.message
                : "Failed to load folder preview";
        } finally {
            folderPreviewLoading = false;
        }
    }

    async function openPreview(path: string, name: string): Promise<void> {
        previewModalOpen = true;
        previewTitle = name;
        previewPath = path;
        resetPreviewState();

        const kind = detectPreviewKind(path);
        previewKind = kind;

        if (kind === "unsupported") {
            return;
        }

        const src = convertFileSrc(path);
        previewSrc = src;

        if (kind !== "text") {
            return;
        }

        previewLoading = true;
        try {
            const response = await fetch(src);
            if (!response.ok) {
                throw new Error(`Failed to read file (${response.status})`);
            }
            previewText = await response.text();
        } catch (error) {
            previewError = error instanceof Error ? error.message : "Failed to load preview";
        } finally {
            previewLoading = false;
        }
    }
</script>

<svelte:window
    onkeydown={(event) => {
        if (event.key === "Escape" && previewModalOpen) {
            closePreview();
        }
    }}
/>

<div class="h-full flex-1 min-w-0 flex flex-col">
    <div class="border-b border-zinc-200 dark:border-zinc-800 flex flex-col">
        {#if !searching && !searched}
            <div class="px-3 py-2">
                <span class="text-xs text-zinc-400 dark:text-zinc-500">
                    Search Status Bar - Empty
                </span>
            </div>
        {/if}

        {#if searching || searched}
            <div class="px-3 pt-2">
                <span class="text-xs text-zinc-400 dark:text-zinc-500">
                    {#if searching}
                        {searchStatus || "Searching..."}
                    {:else}
                        {resultsLength} result{resultsLength === 1 ? "" : "s"}
                    {/if}
                </span>
            </div>
            <div class="px-3 pb-2 flex items-center justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                    <span class="text-xs text-zinc-500 dark:text-zinc-400">
                        Total scanned: {driveScanTotal} folder{driveScanTotal === 1
                            ? ""
                            : "s"}
                    </span>
                </div>

                <div
                    class="ml-auto shrink-0 inline-flex items-center gap-1.5 rounded-md border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 dark:text-zinc-300 px-2 py-1 text-xs text-zinc-600"
                    title={searching ? "Elapsed search time" : "Last search duration"}
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
                    {searching ? "Searching..." : "No files matched your criteria."}
                </span>
            </div>
        {:else}
            {#if layoutMode === "focus"}
                <div
                    class="h-full rounded-lg border border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-900 overflow-hidden grid grid-cols-[1.2fr_1fr]"
                    transition:fade={{ duration: 220 }}
                >
                    <div class="border-r border-zinc-200 dark:border-zinc-800 overflow-auto">
                        {#each treeRows as row (row.node.path)}
                            {#if row.node.isDir}
                                <div
                                    class="w-full flex items-center gap-2 py-2 pr-3 text-left border-b border-zinc-100 dark:border-zinc-800 hover:bg-zinc-50 dark:hover:bg-zinc-800/70 {rowIndentClass(row.depth, 'dir')}"
                                    title={displayPath(row.node.path)}
                                >
                                    <button
                                        class="flex min-w-0 flex-1 items-center gap-2 text-left"
                                        onclick={() => toggleDirectory(row.node.path, row.depth)}
                                    >
                                        <span class="w-3 text-center text-xs text-zinc-500 dark:text-zinc-400">
                                            {#if row.hasChildren}
                                                {#if row.isOpen}
                                                    <ChevronDown size={12} strokeWidth={2} />
                                                {:else}
                                                    <ChevronRight size={12} strokeWidth={2} />
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
                                        {#if isFolderEmpty(row.node.path)}
                                            <span
                                                class="inline-flex items-center rounded-md border border-amber-300 bg-amber-50 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.04em] text-amber-700 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-300"
                                            >
                                                Empty
                                            </span>
                                        {/if}
                                        <span class="text-xs text-zinc-400 dark:text-zinc-500 truncate"
                                            >{displayPath(row.node.path)}</span
                                        >
                                    </button>
                                    <button
                                        class="shrink-0 inline-flex items-center gap-1 rounded-md border border-zinc-300 bg-white px-2 py-1 text-[11px] font-semibold text-zinc-600 transition-colors hover:bg-zinc-100 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
                                        onclick={() => openFolderPreview(row.node.path, row.node.name)}
                                        aria-label={`Preview folder ${row.node.name}`}
                                    >
                                        <Eye size={11} strokeWidth={2} />
                                        Preview
                                    </button>
                                </div>
                            {:else}
                                <button
                                    class="w-full flex items-center gap-2 py-2 pr-3 text-left border-b border-zinc-100 dark:border-zinc-800 hover:bg-zinc-50 dark:hover:bg-zinc-800/70 {rowIndentClass(row.depth, 'file')}"
                                    onclick={() => openInExplorer(row.node.path)}
                                    title={displayPath(row.node.path)}
                                >
                                    <File
                                        size={14}
                                        class="text-zinc-500 dark:text-zinc-400 shrink-0"
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

                    <div class="flex flex-col min-h-0">
                        <div class="border-b border-zinc-200 dark:border-zinc-800 p-3 space-y-2">
                            <div class="flex items-center justify-between gap-2">
                                <p class="text-xs font-semibold text-zinc-700 dark:text-zinc-200 truncate">
                                    Focus: {focusedFolderPath ? displayPath(focusedFolderPath) : "Top level"}
                                </p>
                                <button
                                    class="inline-flex items-center gap-1 rounded-md border border-zinc-300 bg-white px-2 py-1 text-[11px] font-semibold text-zinc-600 transition-colors hover:bg-zinc-100 hover:text-zinc-800 disabled:cursor-not-allowed disabled:opacity-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
                                    onclick={goBackInFocus}
                                    disabled={!canGoBackInFocus}
                                    aria-label="Go to parent folder"
                                >
                                    <ArrowLeft size={11} strokeWidth={2} />
                                    Back
                                </button>
                            </div>
                            <input
                                type="text"
                                class="h-[30px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200"
                                placeholder="Filter names in this folder"
                                bind:value={focusQuery}
                            />
                        </div>

                        <div class="flex-1 overflow-auto">
                            {#if filteredFocusEntries.length === 0}
                                <div class="h-full flex items-center justify-center">
                                    <span class="text-xs text-zinc-400 dark:text-zinc-500">
                                        No direct matches in this folder.
                                    </span>
                                </div>
                            {:else}
                                {#each filteredFocusEntries as entry (entry.path)}
                                    {#if entry.isDir}
                                        <div
                                            class="w-full flex items-center gap-2 py-2 px-3 text-left border-b border-zinc-100 dark:border-zinc-800 hover:bg-zinc-50 dark:hover:bg-zinc-800/70"
                                            title={displayPath(entry.path)}
                                        >
                                            <button
                                                class="flex min-w-0 flex-1 items-center gap-2 text-left"
                                                onclick={() => focusFolder(entry.path)}
                                            >
                                                <Folder
                                                    size={14}
                                                    class="text-zinc-500 dark:text-zinc-400"
                                                    strokeWidth={2}
                                                />
                                                <span class="text-xs text-zinc-700 dark:text-zinc-100 font-medium"
                                                    >{entry.name}</span
                                                >
                                                <span class="text-xs text-zinc-400 dark:text-zinc-500 truncate"
                                                    >{displayPath(entry.path)}</span
                                                >
                                            </button>
                                            <button
                                                class="shrink-0 inline-flex items-center gap-1 rounded-md border border-zinc-300 bg-white px-2 py-1 text-[11px] font-semibold text-zinc-600 transition-colors hover:bg-zinc-100 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
                                                onclick={() => openFolderPreview(entry.path, entry.name)}
                                                aria-label={`Preview folder ${entry.name}`}
                                            >
                                                <Eye size={11} strokeWidth={2} />
                                                Preview
                                            </button>
                                        </div>
                                    {:else}
                                        <div
                                            class="w-full flex items-center gap-2 py-2 px-3 text-left border-b border-zinc-100 dark:border-zinc-800 hover:bg-zinc-50 dark:hover:bg-zinc-800/70"
                                            title={displayPath(entry.path)}
                                        >
                                            <button
                                                class="flex min-w-0 flex-1 items-center gap-2 text-left"
                                                onclick={() => openInExplorer(entry.path)}
                                            >
                                                <File
                                                    size={14}
                                                    class="text-zinc-500 dark:text-zinc-400 shrink-0"
                                                    strokeWidth={2}
                                                />
                                                <span class="text-xs text-zinc-700 dark:text-zinc-100 font-medium"
                                                    >{entry.name}</span
                                                >
                                                <span class="text-xs text-zinc-400 dark:text-zinc-500 truncate"
                                                    >{displayPath(entry.path)}</span
                                                >
                                            </button>
                                            <button
                                                class="shrink-0 inline-flex items-center gap-1 rounded-md border border-zinc-300 bg-white px-2 py-1 text-[11px] font-semibold text-zinc-600 transition-colors hover:bg-zinc-100 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
                                                onclick={() => openPreview(entry.path, entry.name)}
                                                aria-label={`Preview ${entry.name}`}
                                            >
                                                <Eye size={11} strokeWidth={2} />
                                                Preview
                                            </button>
                                        </div>
                                    {/if}
                                {/each}
                            {/if}
                        </div>
                    </div>
                </div>
            {:else}
                <div
                    class="h-full rounded-lg border border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-900 overflow-auto"
                    transition:fade={{ duration: 220 }}
                >
                    {#each treeRows as row (row.node.path)}
                        {#if row.node.isDir}
                            <div
                                class="w-full flex items-center gap-2 py-2 pr-3 text-left border-b border-zinc-100 dark:border-zinc-800 hover:bg-zinc-50 dark:hover:bg-zinc-800/70 {rowIndentClass(row.depth, 'dir')}"
                                title={displayPath(row.node.path)}
                            >
                                <button
                                    class="flex min-w-0 flex-1 items-center gap-2 text-left"
                                    onclick={() => toggleDirectory(row.node.path, row.depth)}
                                >
                                    <span class="w-3 text-center text-xs text-zinc-500 dark:text-zinc-400">
                                        {#if row.hasChildren}
                                            {#if row.isOpen}
                                                <ChevronDown size={12} strokeWidth={2} />
                                            {:else}
                                                <ChevronRight size={12} strokeWidth={2} />
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
                                    {#if isFolderEmpty(row.node.path)}
                                        <span
                                            class="inline-flex items-center rounded-md border border-amber-300 bg-amber-50 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.04em] text-amber-700 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-300"
                                        >
                                            Empty
                                        </span>
                                    {/if}
                                    <span class="text-xs text-zinc-400 dark:text-zinc-500 truncate"
                                        >{displayPath(row.node.path)}</span
                                    >
                                </button>
                                <button
                                    class="shrink-0 inline-flex items-center gap-1 rounded-md border border-zinc-300 bg-white px-2 py-1 text-[11px] font-semibold text-zinc-600 transition-colors hover:bg-zinc-100 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
                                    onclick={() => openFolderPreview(row.node.path, row.node.name)}
                                    aria-label={`Preview folder ${row.node.name}`}
                                >
                                    <Eye size={11} strokeWidth={2} />
                                    Preview
                                </button>
                            </div>
                        {:else}
                            <div
                                class="w-full flex items-center gap-2 py-2 pr-3 text-left border-b border-zinc-100 dark:border-zinc-800 hover:bg-zinc-50 dark:hover:bg-zinc-800/70 {rowIndentClass(row.depth, 'file')}"
                                title={displayPath(row.node.path)}
                            >
                                <button
                                    class="flex min-w-0 flex-1 items-center gap-2 text-left"
                                    onclick={() => openInExplorer(row.node.path)}
                                >
                                    <File
                                        size={14}
                                        class="text-zinc-500 dark:text-zinc-400 shrink-0"
                                        strokeWidth={2}
                                    />
                                    <span class="text-xs text-zinc-700 dark:text-zinc-100 font-medium"
                                        >{row.node.name}</span
                                    >
                                    <span class="text-xs text-zinc-400 dark:text-zinc-500 truncate"
                                        >{displayPath(row.node.path)}</span
                                    >
                                </button>
                                <button
                                    class="shrink-0 inline-flex items-center gap-1 rounded-md border border-zinc-300 bg-white px-2 py-1 text-[11px] font-semibold text-zinc-600 transition-colors hover:bg-zinc-100 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
                                    onclick={() => openPreview(row.node.path, row.node.name)}
                                    aria-label={`Preview ${row.node.name}`}
                                >
                                    <Eye size={11} strokeWidth={2} />
                                    Preview
                                </button>
                            </div>
                        {/if}
                    {/each}
                </div>
            {/if}
        {/if}
    </div>
</div>

{#if previewModalOpen}
    <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
        <button
            class="absolute inset-0 bg-zinc-950/70"
            onclick={closePreview}
            aria-label="Close preview backdrop"
        ></button>
        <div
            class="relative w-full max-w-4xl max-h-[88vh] overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-2xl dark:border-zinc-700 dark:bg-zinc-900"
        >
            <div class="flex items-center justify-between gap-3 border-b border-zinc-200 px-4 py-3 dark:border-zinc-800">
                <div class="min-w-0">
                    <p class="truncate text-sm font-semibold text-zinc-800 dark:text-zinc-100">{previewTitle}</p>
                    <p class="truncate text-xs text-zinc-500 dark:text-zinc-400">{displayPath(previewPath)}</p>
                </div>
                <button
                    class="inline-flex items-center rounded-md border border-zinc-300 bg-white p-1.5 text-zinc-600 hover:bg-zinc-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
                    onclick={closePreview}
                    aria-label="Close preview"
                >
                    <X size={14} strokeWidth={2} />
                </button>
            </div>

            <div class="max-h-[76vh] overflow-auto p-4">
                {#if previewKind === "unsupported"}
                    <div class="rounded-lg border border-amber-300 bg-amber-50 p-4 text-sm text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200">
                        This file type is not supported for preview yet.
                    </div>
                {:else if previewKind === "image"}
                    <img src={previewSrc} alt={previewTitle} class="mx-auto max-h-[68vh] w-auto rounded-md" />
                {:else if previewKind === "pdf"}
                    <iframe
                        title={previewTitle}
                        src={previewSrc}
                        class="h-[70vh] w-full rounded-md border border-zinc-200 dark:border-zinc-700"
                    ></iframe>
                {:else if previewKind === "audio"}
                    <audio controls src={previewSrc} class="w-full"></audio>
                {:else if previewKind === "video"}
                    <!-- svelte-ignore a11y_media_has_caption -->
                    <video controls src={previewSrc} class="h-auto max-h-[68vh] w-full rounded-md bg-black"></video>
                {:else}
                    {#if previewLoading}
                        <p class="text-sm text-zinc-500 dark:text-zinc-400">Loading preview...</p>
                    {:else if previewError}
                        <div class="rounded-lg border border-red-300 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200">
                            {previewError}
                        </div>
                    {:else}
                        <pre class="max-h-[68vh] overflow-auto whitespace-pre-wrap rounded-md border border-zinc-200 bg-zinc-50 p-3 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-950 dark:text-zinc-200">{previewText}</pre>
                    {/if}
                {/if}
            </div>
        </div>
    </div>
{/if}

{#if folderPreviewModalOpen}
    <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
        <button
            class="absolute inset-0 bg-zinc-950/70"
            onclick={closeFolderPreview}
            aria-label="Close folder preview backdrop"
        ></button>
        <div
            class="relative w-full max-w-3xl max-h-[86vh] overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-2xl dark:border-zinc-700 dark:bg-zinc-900"
        >
            <div class="flex items-center justify-between gap-3 border-b border-zinc-200 px-4 py-3 dark:border-zinc-800">
                <div class="min-w-0">
                    <p class="truncate text-sm font-semibold text-zinc-800 dark:text-zinc-100">
                        Folder Preview: {folderPreviewTitle}
                    </p>
                    <p class="truncate text-xs text-zinc-500 dark:text-zinc-400">
                        {displayPath(folderPreviewPath)}
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    <button
                        class="inline-flex items-center rounded-md border border-zinc-300 bg-white px-2 py-1 text-xs font-semibold text-zinc-700 hover:bg-zinc-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:hover:bg-zinc-800"
                        onclick={() => openInExplorer(folderPreviewPath)}
                    >
                        Open in Files
                    </button>
                    <button
                        class="inline-flex items-center rounded-md border border-zinc-300 bg-white p-1.5 text-zinc-600 hover:bg-zinc-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
                        onclick={closeFolderPreview}
                        aria-label="Close folder preview"
                    >
                        <X size={14} strokeWidth={2} />
                    </button>
                </div>
            </div>

            <div class="max-h-[74vh] overflow-auto p-3">
                {#if folderPreviewLoading}
                    <p class="text-sm text-zinc-500 dark:text-zinc-400">Loading folder preview...</p>
                {:else if folderPreviewError}
                    <div class="rounded-lg border border-red-300 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200">
                        {folderPreviewError}
                    </div>
                {:else if folderPreviewEntries.length === 0}
                    <p class="text-sm text-zinc-500 dark:text-zinc-400">Folder appears to be empty.</p>
                {:else}
                    <div class="rounded-md border border-zinc-200 dark:border-zinc-700 overflow-hidden">
                        {#each folderPreviewEntries as entry (entry.path)}
                            <div class="flex items-center gap-2 border-b border-zinc-100 dark:border-zinc-800 px-3 py-2 last:border-b-0">
                                {#if entry.is_dir}
                                    <Folder size={14} class="text-zinc-500 dark:text-zinc-400" strokeWidth={2} />
                                {:else}
                                    <File size={14} class="text-zinc-500 dark:text-zinc-400" strokeWidth={2} />
                                {/if}
                                <span class="text-xs font-medium text-zinc-700 dark:text-zinc-100">{entry.name}</span>
                                <span class="text-xs text-zinc-400 dark:text-zinc-500 truncate">{displayPath(entry.path)}</span>
                            </div>
                        {/each}
                    </div>
                {/if}
            </div>
        </div>
    </div>
{/if}
