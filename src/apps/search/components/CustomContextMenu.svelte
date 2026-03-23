<script lang="ts">
    export type ContextMenuItem = {
        id: string;
        label: string;
    };

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
</script>

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
        class="fixed z-50 min-w-52 rounded-lg border border-zinc-200 bg-white p-1 shadow-xl dark:border-zinc-700 dark:bg-zinc-900"
        style={`left: ${x}px; top: ${y}px;`}
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
