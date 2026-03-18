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
    class={`chip-select ${containerClass}`}
    bind:this={rootEl}
>
    <button
        type="button"
        class="chip-select-trigger"
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        onclick={toggleOpen}
    >
        <span class={`chip-select-label ${selected ? "" : "placeholder"}`}>
            {selected ? selected.label : placeholder}
        </span>
        <ChevronDown
            size={13}
            strokeWidth={2}
            class={`chip-select-caret ${open ? "open" : ""}`}
        />
    </button>

    {#if open}
        <div class="chip-select-menu" role="listbox" aria-label={ariaLabel}>
            {#each options as option (option.value)}
                <button
                    type="button"
                    class={`chip-select-option ${option.value === value ? "active" : ""}`}
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

<style>
    .chip-select {
        position: relative;
        width: 100%;
    }

    .chip-select-trigger {
        width: 100%;
        height: var(--filter-control-height, 30px);
        min-height: var(--filter-control-height, 30px);
        border-radius: 0.45rem;
        border: 1px solid color-mix(in srgb, var(--filter-accent) 22%, var(--control-border));
        background: color-mix(in srgb, var(--panel) 90%, transparent);
        color: var(--control-text);
        padding: 0 0.5rem;
        display: inline-flex;
        align-items: center;
        justify-content: space-between;
        gap: 0.45rem;
        font-size: 0.75rem;
        text-align: left;
        transition: background-color 0.15s ease, border-color 0.15s ease;
    }

    .chip-select-trigger:hover {
        background: color-mix(in srgb, var(--filter-accent) 14%, var(--control-bg-hover));
        border-color: color-mix(in srgb, var(--filter-accent) 36%, var(--control-border));
    }

    .chip-select-trigger:focus-visible {
        outline: none;
        border-color: color-mix(in srgb, var(--filter-accent) 52%, var(--focus-ring));
    }

    .chip-select-label {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        min-width: 0;
    }

    .chip-select-label.placeholder {
        color: var(--control-muted);
    }

    .chip-select-caret {
        color: color-mix(in srgb, var(--filter-accent) 50%, var(--control-muted));
        transition: transform 0.18s ease;
        flex-shrink: 0;
    }

    .chip-select-caret.open {
        transform: rotate(180deg);
    }

    .chip-select-menu {
        position: absolute;
        top: calc(100% + 0.35rem);
        left: 0;
        right: 0;
        z-index: 60;
        max-height: 220px;
        overflow-y: auto;
        padding: 0.35rem;
        border-radius: 0.6rem;
        border: 1px solid color-mix(in srgb, var(--filter-accent) 30%, var(--control-border));
        background: color-mix(in srgb, var(--panel) 94%, transparent);
    }

    .chip-select-option {
        width: 100%;
        border: none;
        border-radius: 0.4rem;
        padding: 0.35rem 0.45rem;
        background: transparent;
        color: var(--control-text);
        text-align: left;
        font-size: 0.75rem;
        cursor: pointer;
        transition: background-color 0.15s ease, color 0.15s ease;
    }

    .chip-select-option:hover {
        background: color-mix(in srgb, var(--filter-accent) 14%, transparent);
    }

    .chip-select-option.active {
        background: color-mix(in srgb, var(--filter-accent) 22%, transparent);
        color: color-mix(in srgb, var(--filter-accent) 70%, var(--control-text));
        font-weight: 600;
    }
</style>
