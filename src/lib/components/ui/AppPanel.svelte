<script lang="ts">
    let {
        class: className = "",
        padded = true,
        children,
    }: {
        class?: string;
        padded?: boolean;
        children?: import("svelte").Snippet;
    } = $props();
</script>

<section
    class={`rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900 ${padded ? "p-3" : ""} ${className}`}
>
    {@render children?.()}
</section>
