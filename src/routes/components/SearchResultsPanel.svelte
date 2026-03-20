<script lang="ts">
    import { fade } from "svelte/transition";
    import {
        ChevronDown,
        ChevronRight,
        Clock3,
        File,
        Folder,
        HardDrive,
        LoaderCircle,
    } from "lucide-svelte";
    import type { DriveScanRow, TreeRow } from "../search/page-types";

    let {
        searching,
        searched,
        searchStatus,
        resultsLength,
        driveScanTotal,
        searchDurationLabel,
        driveScanRows,
        treeRows,
        rowIndentClass,
        displayPath,
        isFolderScanning,
        toggleDirectory,
        openInExplorer,
    }: {
        searching: boolean;
        searched: boolean;
        searchStatus: string;
        resultsLength: number;
        driveScanTotal: number;
        searchDurationLabel: string;
        driveScanRows: DriveScanRow[];
        treeRows: TreeRow[];
        rowIndentClass: (depth: number, kind: "dir" | "file") => string;
        displayPath: (path: string) => string;
        isFolderScanning: (path: string) => boolean;
        toggleDirectory: (path: string, depth: number) => void;
        openInExplorer: (path: string) => Promise<void>;
    } = $props();
</script>

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
            <div
                class="h-full rounded-lg border border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-900 overflow-auto"
                transition:fade={{ duration: 220 }}
            >
                {#each treeRows as row (row.node.path)}
                    {#if row.node.isDir}
                        <button
                            class="w-full flex items-center gap-2 py-2 pr-3 text-left border-b border-zinc-100 dark:border-zinc-800 hover:bg-zinc-50 dark:hover:bg-zinc-800/70 {rowIndentClass(row.depth, 'dir')}"
                            onclick={() => toggleDirectory(row.node.path, row.depth)}
                            title={displayPath(row.node.path)}
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
