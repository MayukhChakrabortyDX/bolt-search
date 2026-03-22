<script lang="ts">
    import { ChevronDown, X } from "lucide-svelte";
    import ChipSelect from "../../../lib/components/ChipSelect.svelte";
    import CalendarField from "../../../lib/components/CalendarField.svelte";
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
    } = $props();
</script>

<section class="flex flex-col gap-2 rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 p-3">
    <button
        type="button"
        class="flex w-full items-center justify-between text-zinc-500 dark:text-zinc-300"
        onclick={onToggleAdvanced}
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
    {/if}
</section>
