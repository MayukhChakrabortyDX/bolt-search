<script lang="ts">
    import ChipSelect from "../../../lib/components/ChipSelect.svelte";
    import { FolderOpen, X } from "lucide-svelte";
    import type { SearchFormState } from "../search/page-types";

    let {
        searchForm,
        driveOptions,
        ensureDriveScopeSelection,
        pickScopeFolders,
        removeScopeFolder,
        displayPath,
    }: {
        searchForm: SearchFormState;
        driveOptions: Array<{ value: string; label: string }>;
        ensureDriveScopeSelection: () => void;
        pickScopeFolders: () => Promise<void>;
        removeScopeFolder: (path: string) => void;
        displayPath: (path: string) => string;
    } = $props();
</script>

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
                                onclick={() => removeScopeFolder(folderPath)}
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
