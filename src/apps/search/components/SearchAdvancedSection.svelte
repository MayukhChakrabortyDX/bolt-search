<script lang="ts">
    import { SlidersHorizontal, FolderPlus, X } from "lucide-svelte";
    import ChipSelect from "../../../lib/components/ChipSelect.svelte";
    import CalendarField from "../../../lib/components/CalendarField.svelte";
    import AppModal from "../../../lib/components/ui/AppModal.svelte";
    import type { SearchFormState } from "../search/page-types";

    let {
        searchForm,
        showAdvanced,
        selectedExtensionTokens,
        popularExtensionOptions,
        sizeUnitOptions,
        onToggleAdvanced,
        onTogglePopularExtension,
        onNormalizeExtensionInput,
        onRemoveExtensionToken,
        onPickExcludedFolders,
        onRemoveExcludedFolder,
        displayPath,
    }: {
        searchForm: SearchFormState;
        showAdvanced: boolean;
        selectedExtensionTokens: string[];
        popularExtensionOptions: ReadonlyArray<{ value: string; label: string }>;
        sizeUnitOptions: ReadonlyArray<{ value: string; label: string }>;
        onToggleAdvanced: () => void;
        onTogglePopularExtension: (value: string) => void;
        onNormalizeExtensionInput: () => void;
        onRemoveExtensionToken: (value: string) => void;
        onPickExcludedFolders: () => Promise<void>;
        onRemoveExcludedFolder: (path: string) => void;
        displayPath: (path: string) => string;
    } = $props();

    let excludedModalOpen = $state(false);

    function openExcludedFoldersModal() {
        excludedModalOpen = true;
    }

    function closeExcludedFoldersModal() {
        excludedModalOpen = false;
    }

    function clearExcludedFolders() {
        searchForm.excludedFolders = [];
    }

    function closeAdvancedModal() {
        if (showAdvanced) {
            onToggleAdvanced();
        }
    }

    $effect(() => {
        if (!showAdvanced && excludedModalOpen) {
            excludedModalOpen = false;
        }
    });
</script>

<section class="flex flex-col gap-2 rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 p-3">
    <button
        type="button"
        class="inline-flex items-center justify-center gap-2 rounded-md border border-zinc-300 bg-white px-2 py-2 text-xs font-semibold text-zinc-700 hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:hover:bg-zinc-800"
        onclick={onToggleAdvanced}
    >
        <SlidersHorizontal size={14} strokeWidth={2} />
        Advanced Filters
    </button>

    <p class="text-[11px] text-zinc-500 dark:text-zinc-400">
        Open modal to edit extension, path, date, size and excluded-folder filters.
    </p>
</section>

<AppModal
    open={showAdvanced}
    title="Advanced Filters"
    onClose={closeAdvancedModal}
    class="max-w-4xl"
>
    <div class="grid gap-3">
        <div class="grid gap-2">
            <label class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500" for="ext-input">
                Extensions
            </label>
            <div class="flex flex-wrap gap-1.5">
                {#each popularExtensionOptions as option}
                    <button
                        type="button"
                        class={`rounded-full border px-2 py-1 text-[10px] font-semibold tracking-[0.01em] transition-colors ${selectedExtensionTokens.includes(option.value)
                            ? "border-zinc-900 bg-zinc-900 text-white dark:border-zinc-100 dark:bg-zinc-100 dark:text-zinc-900"
                            : "border-zinc-300 bg-white text-zinc-600 hover:bg-zinc-50 hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"}`}
                        aria-pressed={selectedExtensionTokens.includes(option.value)}
                        onclick={() => onTogglePopularExtension(option.value)}
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
                onblur={onNormalizeExtensionInput}
            />

            {#if selectedExtensionTokens.length > 0}
                <div class="flex flex-wrap gap-1">
                    {#each selectedExtensionTokens as ext}
                        <button
                            type="button"
                            class="inline-flex items-center gap-1 rounded-full border border-zinc-300 bg-zinc-100 px-2 py-0.5 text-[11px] font-semibold text-zinc-700 transition-colors hover:bg-zinc-200 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700"
                            onclick={() => onRemoveExtensionToken(ext)}
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

            <div class="grid gap-1.5">
                <div class="flex items-center justify-between gap-2">
                    <span class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500">Excluded Folders</span>
                    <button
                        type="button"
                        class="inline-flex items-center gap-1 rounded-md border border-zinc-300 bg-white px-2 py-1 text-[11px] font-semibold text-zinc-600 hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
                        onclick={openExcludedFoldersModal}
                    >
                        <FolderPlus size={12} strokeWidth={2} />
                        Edit
                    </button>
                </div>

                {#if searchForm.excludedFolders.length > 0}
                    <div class="flex flex-wrap gap-1">
                        {#each searchForm.excludedFolders.slice(0, 3) as path}
                            <span class="inline-flex max-w-full items-center rounded-full border border-zinc-300 bg-zinc-100 px-2 py-0.5 text-[10px] font-semibold text-zinc-700 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300">
                                <span class="truncate">{displayPath(path)}</span>
                            </span>
                        {/each}
                        {#if searchForm.excludedFolders.length > 3}
                            <span class="inline-flex items-center rounded-full border border-zinc-300 bg-zinc-100 px-2 py-0.5 text-[10px] font-semibold text-zinc-600 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-400">
                                +{searchForm.excludedFolders.length - 3} more
                            </span>
                        {/if}
                    </div>
                {:else}
                    <p class="text-[11px] text-zinc-500 dark:text-zinc-400">No excluded folders configured.</p>
                {/if}
            </div>

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
                    options={sizeUnitOptions}
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
                    options={sizeUnitOptions}
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

        <div class="flex justify-end">
            <button
                type="button"
                class="rounded-md border border-zinc-300 px-2.5 py-1.5 text-[11px] font-semibold text-zinc-700 hover:bg-zinc-100 dark:border-zinc-700 dark:text-zinc-200 dark:hover:bg-zinc-800"
                onclick={closeAdvancedModal}
            >
                Done
            </button>
        </div>
    </div>
</AppModal>

<AppModal
    open={excludedModalOpen}
    title="Excluded Folders"
    onClose={closeExcludedFoldersModal}
    class="max-w-2xl"
>
    <div class="grid gap-3">
        <p class="text-xs text-zinc-600 dark:text-zinc-300">
            Excluded folders are skipped during traversal, including their subfolders.
        </p>

        <div class="flex items-center gap-2">
            <button
                type="button"
                class="inline-flex items-center gap-1 rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-[11px] font-semibold text-zinc-700 hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:hover:bg-zinc-800"
                onclick={onPickExcludedFolders}
            >
                <FolderPlus size={12} strokeWidth={2} />
                Add folders
            </button>

            <button
                type="button"
                class="rounded-md px-2.5 py-1.5 text-[11px] font-semibold text-zinc-500 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
                onclick={clearExcludedFolders}
                disabled={searchForm.excludedFolders.length === 0}
            >
                Clear all
            </button>
        </div>

        {#if searchForm.excludedFolders.length === 0}
            <p class="rounded-md border border-zinc-200 bg-zinc-50 p-2 text-[11px] text-zinc-500 dark:border-zinc-800 dark:bg-zinc-950 dark:text-zinc-400">
                No excluded folders selected.
            </p>
        {:else}
            <div class="max-h-[360px] overflow-auto rounded-md border border-zinc-200 dark:border-zinc-800">
                <ul class="divide-y divide-zinc-200 dark:divide-zinc-800">
                    {#each searchForm.excludedFolders as path}
                        <li class="flex items-center justify-between gap-2 px-3 py-2">
                            <span class="truncate text-xs text-zinc-700 dark:text-zinc-200">{displayPath(path)}</span>
                            <button
                                type="button"
                                class="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-zinc-500 hover:bg-zinc-100 hover:text-zinc-700 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-zinc-200"
                                aria-label={`Remove excluded folder ${displayPath(path)}`}
                                onclick={() => onRemoveExcludedFolder(path)}
                            >
                                <X size={12} strokeWidth={2} />
                            </button>
                        </li>
                    {/each}
                </ul>
            </div>
        {/if}

        <div class="flex justify-end">
            <button
                type="button"
                class="rounded-md border border-zinc-300 px-2.5 py-1.5 text-[11px] font-semibold text-zinc-700 hover:bg-zinc-100 dark:border-zinc-700 dark:text-zinc-200 dark:hover:bg-zinc-800"
                onclick={closeExcludedFoldersModal}
            >
                Done
            </button>
        </div>
    </div>
</AppModal>
