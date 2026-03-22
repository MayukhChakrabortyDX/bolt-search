<script lang="ts">
    let {
        open = false,
        title = "",
        onClose,
        class: className = "",
        headerActions,
        children,
    }: {
        open?: boolean;
        title?: string;
        onClose?: () => void;
        class?: string;
        headerActions?: import("svelte").Snippet;
        children?: import("svelte").Snippet;
    } = $props();
</script>

{#if open}
    <div class="fixed inset-0 z-120 flex items-center justify-center p-4 sm:p-5">
        <button
            type="button"
            aria-label="Close dialog"
            class="absolute inset-0 border-0 bg-black/60"
            onclick={() => onClose?.()}
        ></button>

        <div
            role="dialog"
            aria-modal="true"
            aria-label={title}
            class={`relative z-10 w-full max-h-[88vh] overflow-auto rounded-xl border border-zinc-200 bg-white p-4 shadow-2xl dark:border-zinc-800 dark:bg-zinc-900 sm:p-5 ${className}`}
        >
            <header class="mb-4 flex items-center justify-between gap-3">
                <h3 class="text-sm font-bold tracking-[0.01em] text-zinc-900 dark:text-zinc-100">
                    {title}
                </h3>
                {@render headerActions?.()}
            </header>

            {@render children?.()}
        </div>
    </div>
{/if}
