<script lang="ts">
    import {
        AlertTriangle,
        ChevronDown,
        FolderOpen,
        Search,
        Trash2,
        X,
    } from "lucide-svelte";
    import ChipSelect, {
        type ChipSelectOption,
    } from "../../lib/components/ChipSelect.svelte";
    import CalendarField from "../../lib/components/CalendarField.svelte";
    import {
        POPULAR_EXTENSION_OPTIONS,
        SIZE_UNIT_OPTIONS,
    } from "../search-page/constants";
    import {
        normalizeExtension,
        parseExtensionTokens,
    } from "../search-page/form-utils";
    import type { SearchFormState, ValidationIssue } from "../search-page/types";

    let {
        searchForm = $bindable(),
        showAdvanced = $bindable(),
        searching,
        searched,
        resultsCount,
        hasContradiction,
        validationIssues,
        driveOptions,
        onSearch,
        onStopSearch,
        onClearResults,
        onResetForm,
        onPickScopeFolders,
        onEnsureDriveScopeSelection,
        displayPath,
    }: {
        searchForm: SearchFormState;
        showAdvanced: boolean;
        searching: boolean;
        searched: boolean;
        resultsCount: number;
        hasContradiction: boolean;
        validationIssues: ValidationIssue[];
        driveOptions: ChipSelectOption[];
        onSearch: () => void;
        onStopSearch: () => void;
        onClearResults: () => void;
        onResetForm: () => void;
        onPickScopeFolders: () => void;
        onEnsureDriveScopeSelection: () => void;
        displayPath: (path: string) => string;
    } = $props();

    const selectedExtensionTokens = $derived(
        parseExtensionTokens(searchForm.extensionInput),
    );

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

    function removeScopeFolder(pathToRemove: string) {
        searchForm.scopeFolders = searchForm.scopeFolders.filter(
            (path) => path !== pathToRemove,
        );
    }
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
            onclick={searching ? onStopSearch : onSearch}
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
            disabled={searching || (!searched && resultsCount === 0)}
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
        <section
            class="flex flex-col gap-2 rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 p-3"
        >
            <label
                class="text-[11px] font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400"
                for="query-input"
            >
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

        <section
            class="flex flex-col gap-2 rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 p-3"
        >
            <div class="flex items-center justify-between">
                <span
                    class="text-[11px] font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400"
                    >Scope</span
                >
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
                        onEnsureDriveScopeSelection();
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
                        onclick={onPickScopeFolders}
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
                        <div
                            class="max-h-32 overflow-auto rounded-md border border-zinc-300 bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900"
                        >
                            {#each searchForm.scopeFolders as folderPath}
                                <div
                                    class="flex items-start justify-between gap-2 border-b border-zinc-200 dark:border-zinc-800 px-2 py-1.5 last:border-b-0"
                                >
                                    <span
                                        class="break-all text-[11px] text-zinc-500 dark:text-zinc-300"
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

        <section
            class="flex flex-col gap-2 rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 p-3"
        >
            <span
                class="text-[11px] font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400"
                >Type</span
            >
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

        <section
            class="flex flex-col gap-2 rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 p-3"
        >
            <button
                type="button"
                class="flex w-full items-center justify-between text-zinc-500 dark:text-zinc-300"
                onclick={() => {
                    showAdvanced = !showAdvanced;
                }}
            >
                <span
                    class="text-xs font-semibold tracking-[0.02em] text-zinc-700 dark:text-zinc-200"
                    >Advanced</span
                >
                <ChevronDown
                    size={14}
                    strokeWidth={2}
                    class={showAdvanced ? "" : "rotate-[-90deg]"}
                />
            </button>

            {#if showAdvanced}
                <div class="grid gap-2">
                    <label
                        class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500"
                        for="ext-input"
                    >
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

                    <label
                        class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500"
                        for="path-contains-input"
                    >
                        Path Contains
                    </label>
                    <input
                        id="path-contains-input"
                        type="text"
                        class="h-[30px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:placeholder:text-zinc-500 transition-colors focus:border-zinc-500 focus:ring-1 focus:ring-zinc-200/80 dark:focus:ring-zinc-700/60 focus:outline-none"
                        placeholder="src, workspace, backup"
                        bind:value={searchForm.pathContainsInput}
                    />

                    <label
                        class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500"
                        for="path-prefix-input"
                    >
                        Path Prefix
                    </label>
                    <input
                        id="path-prefix-input"
                        type="text"
                        class="h-[30px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:placeholder:text-zinc-500 transition-colors focus:border-zinc-500 focus:ring-1 focus:ring-zinc-200/80 dark:focus:ring-zinc-700/60 focus:outline-none"
                        placeholder="C:/Users/me/Projects"
                        bind:value={searchForm.pathPrefix}
                    />

                    <span
                        class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500"
                        >Size Range</span
                    >
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

                    <span
                        class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500"
                        >Modified</span
                    >
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

                    <span
                        class="text-[10px] font-semibold uppercase tracking-[0.07em] text-zinc-400 dark:text-zinc-500"
                        >Created</span
                    >
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
        <div
            class="flex flex-col gap-1 rounded-md border border-red-300 bg-red-50 dark:border-red-900 dark:bg-red-950/40 p-2"
        >
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
        transition:
            background-color 0.15s ease,
            border-color 0.15s ease,
            color 0.15s ease;
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
