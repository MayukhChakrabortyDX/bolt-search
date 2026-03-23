<script lang="ts">
    import { AlertTriangle, Search, Trash2, X } from "lucide-svelte";
    import type {
        SearchFormState,
        ValidationIssue,
    } from "../search/page-types";
    import SearchScopeSection from "./SearchScopeSection.svelte";
    import SearchAdvancedSection from "./SearchAdvancedSection.svelte";

    let {
        searching,
        searched,
        resultsLength,
        hasContradiction,
        validationIssues,
        searchForm,
        showAdvanced,
        selectedExtensionTokens,
        driveOptions,
        popularExtensionOptions,
        sizeUnitOptions,
        onSearch,
        onStop,
        onClearResults,
        onResetForm,
        onSetShowAdvanced,
        onEnsureDriveScopeSelection,
        onPickScopeFolders,
        onRemoveScopeFolder,
        onTogglePopularExtension,
        onNormalizeExtensionInput,
        onRemoveExtensionToken,
        displayPath,
    }: {
        searching: boolean;
        searched: boolean;
        resultsLength: number;
        hasContradiction: boolean;
        validationIssues: ValidationIssue[];
        searchForm: SearchFormState;
        showAdvanced: boolean;
        selectedExtensionTokens: string[];
        driveOptions: Array<{ value: string; label: string }>;
        popularExtensionOptions: ReadonlyArray<{ value: string; label: string }>;
        sizeUnitOptions: ReadonlyArray<{ value: string; label: string }>;
        onSearch: () => Promise<void>;
        onStop: () => Promise<void>;
        onClearResults: () => void;
        onResetForm: () => void;
        onSetShowAdvanced: (next: boolean) => void;
        onEnsureDriveScopeSelection: () => void;
        onPickScopeFolders: () => Promise<void>;
        onRemoveScopeFolder: (path: string) => void;
        onTogglePopularExtension: (value: string) => void;
        onNormalizeExtensionInput: () => void;
        onRemoveExtensionToken: (value: string) => void;
        displayPath: (path: string) => string;
    } = $props();
</script>

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
            onclick={searching ? onStop : onSearch}
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
            onclick={onClearResults}
            disabled={searching || (!searched && resultsLength === 0)}
        >
            <Trash2 size={13} strokeWidth={2} />
            Clear Results
        </button>

        <button
            class="py-2 text-[11px] rounded-md border border-zinc-300 bg-white hover:bg-zinc-50 text-zinc-600 dark:border-zinc-700 dark:bg-zinc-800 dark:hover:bg-zinc-700 dark:text-zinc-300 flex items-center justify-center gap-1 disabled:opacity-50 disabled:cursor-not-allowed"
            onclick={onResetForm}
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

        <SearchScopeSection
            {searchForm}
            {driveOptions}
            ensureDriveScopeSelection={onEnsureDriveScopeSelection}
            pickScopeFolders={onPickScopeFolders}
            removeScopeFolder={onRemoveScopeFolder}
            {displayPath}
        />

        <section class="flex flex-col gap-2 rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 p-3">
            <span class="text-[11px] font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400">Type</span>
            <div class="flex flex-wrap gap-1.5">
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
                    <span class={`form-toggle-indicator ${searchForm.includeHidden ? "on" : ""}`}></span>
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
                    <span class={`form-toggle-indicator ${searchForm.readonlyOnly ? "on" : ""}`}></span>
                    <span class="form-toggle-label">Read only</span>
                </button>
            </div>
        </section>

        <SearchAdvancedSection
            {searchForm}
            {showAdvanced}
            {selectedExtensionTokens}
            {popularExtensionOptions}
            {sizeUnitOptions}
            onToggleAdvanced={() => onSetShowAdvanced(!showAdvanced)}
            onTogglePopularExtension={onTogglePopularExtension}
            onNormalizeExtensionInput={onNormalizeExtensionInput}
            onRemoveExtensionToken={onRemoveExtensionToken}
        />
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
