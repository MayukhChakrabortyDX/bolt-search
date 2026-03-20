<script lang="ts">
    import { onMount } from "svelte";
    import { ChevronDown } from "lucide-svelte";

    export type ChipSelectOption = {
        value: string;
        label: string;
    };

    let {
        value = $bindable(""),
        options = [] as ChipSelectOption[],
        ariaLabel = "Select option",
        placeholder = "Select...",
        containerClass = "",
        onChange,
    }: {
        value?: string;
        options?: ChipSelectOption[];
        ariaLabel?: string;
        placeholder?: string;
        containerClass?: string;
        onChange?: (nextValue: string) => void;
    } = $props();

    let open = $state(false);
    let rootEl: HTMLDivElement | null = null;

    const selected = $derived(
        options.find((option) => option.value === value) ?? null,
    );

    function toggleOpen() {
        if (options.length === 0) return;
        open = !open;
    }

    function closeMenu() {
        open = false;
    }

    function choose(optionValue: string) {
        value = optionValue;
        onChange?.(optionValue);
        closeMenu();
    }

    onMount(() => {
        const onPointerDown = (event: PointerEvent) => {
            if (!open || !rootEl) return;
            const target = event.target as Node | null;
            if (target && !rootEl.contains(target)) {
                closeMenu();
            }
        };

        const onKeyDown = (event: KeyboardEvent) => {
            if (open && event.key === "Escape") {
                closeMenu();
            }
        };

        window.addEventListener("pointerdown", onPointerDown);
        window.addEventListener("keydown", onKeyDown);
        return () => {
            window.removeEventListener("pointerdown", onPointerDown);
            window.removeEventListener("keydown", onKeyDown);
        };
    });
</script>

<div
    class={`relative w-full ${containerClass}`}
    bind:this={rootEl}
>
    <button
        type="button"
        class="inline-flex h-7.5 min-h-7.5 w-full items-center justify-between gap-2 rounded-md border border-zinc-300 bg-white dark:border-zinc-700 dark:bg-zinc-900 px-2 text-left text-xs text-zinc-700 dark:text-zinc-200 transition-colors hover:bg-zinc-50 dark:hover:bg-zinc-800 focus-visible:border-zinc-500 focus-visible:outline-none"
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        onclick={toggleOpen}
    >
        <span class={`min-w-0 overflow-hidden text-ellipsis whitespace-nowrap ${selected ? "" : "text-zinc-400 dark:text-zinc-500"}`}>
            {selected ? selected.label : placeholder}
        </span>
        <ChevronDown
            size={13}
            strokeWidth={2}
            class={`shrink-0 text-zinc-500 dark:text-zinc-400 transition-transform ${open ? "rotate-180" : ""}`}
        />
    </button>

    {#if open}
        <div
            class="absolute left-0 right-0 top-[calc(100%+0.35rem)] z-60 max-h-55 overflow-y-auto rounded-lg border border-zinc-300 dark:border-zinc-700 bg-white dark:bg-zinc-900 p-1.5"
            role="listbox"
            aria-label={ariaLabel}
        >
            {#each options as option (option.value)}
                <button
                    type="button"
                    class={`w-full rounded-md px-2 py-1.5 text-left text-xs transition-colors ${option.value === value
                        ? "bg-zinc-200 dark:bg-zinc-700 font-semibold text-zinc-800 dark:text-zinc-100"
                        : "text-zinc-700 dark:text-zinc-200 hover:bg-zinc-100 dark:hover:bg-zinc-800"}`}
                    role="option"
                    aria-selected={option.value === value}
                    onclick={() => choose(option.value)}
                >
                    {option.label}
                </button>
            {/each}
        </div>
    {/if}
</div>
