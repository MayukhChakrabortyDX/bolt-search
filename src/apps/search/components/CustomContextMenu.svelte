<script lang="ts">
    import { tick } from "svelte";

    export type ContextMenuItem = {
        id: string;
        label: string;
    };

    const VIEWPORT_PADDING = 8;

    let {
        open,
        x,
        y,
        items,
        onSelect,
        onClose,
    }: {
        open: boolean;
        x: number;
        y: number;
        items: ContextMenuItem[];
        onSelect: (id: string) => void | Promise<void>;
        onClose: () => void;
    } = $props();

    let menuEl = $state<HTMLDivElement | null>(null);
    let menuX = $state(0);
    let menuY = $state(0);

    function clampToViewport(nextX: number, nextY: number): { x: number; y: number } {
        const width = menuEl?.offsetWidth ?? 0;
        const height = menuEl?.offsetHeight ?? 0;
        const maxX = Math.max(VIEWPORT_PADDING, window.innerWidth - width - VIEWPORT_PADDING);
        const maxY = Math.max(VIEWPORT_PADDING, window.innerHeight - height - VIEWPORT_PADDING);

        return {
            x: Math.min(Math.max(nextX, VIEWPORT_PADDING), maxX),
            y: Math.min(Math.max(nextY, VIEWPORT_PADDING), maxY),
        };
    }

    async function updateMenuPosition(): Promise<void> {
        if (!open) {
            return;
        }

        await tick();
        const next = clampToViewport(x, y);
        menuX = next.x;
        menuY = next.y;
    }

    $effect(() => {
        if (!open) {
            return;
        }

        void updateMenuPosition();
    });
</script>

<svelte:window onresize={() => void updateMenuPosition()} />

{#if open}
    <button
        class="fixed inset-0 z-40 cursor-default bg-transparent"
        onclick={onClose}
        oncontextmenu={(event) => {
            event.preventDefault();
            onClose();
        }}
        aria-label="Close context menu"
    ></button>

    <div
        bind:this={menuEl}
        class="fixed z-50 min-w-52 rounded-lg border border-zinc-200 bg-white p-1 shadow-xl dark:border-zinc-700 dark:bg-zinc-900"
        style={`left: ${menuX}px; top: ${menuY}px;`}
        role="menu"
        tabindex={-1}
        oncontextmenu={(event) => event.preventDefault()}
    >
        {#each items as item (item.id)}
            <button
                class="flex w-full items-center rounded-md px-3 py-2 text-left text-xs font-medium text-zinc-700 hover:bg-zinc-100 dark:text-zinc-200 dark:hover:bg-zinc-800"
                role="menuitem"
                onclick={() => onSelect(item.id)}
            >
                {item.label}
            </button>
        {/each}
    </div>
{/if}
