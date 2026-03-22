<script lang="ts">
    type BadgeTone = "neutral" | "success" | "warning" | "danger" | "info";

    let {
        tone = "neutral",
        class: className = "",
        children,
    }: {
        tone?: BadgeTone;
        class?: string;
        children?: import("svelte").Snippet;
    } = $props();

    const toneClassMap: Record<BadgeTone, string> = {
        neutral:
            "border-zinc-300 bg-zinc-100 text-zinc-700 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200",
        success: "border-emerald-300 bg-emerald-100 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/60 dark:text-emerald-300",
        warning: "border-amber-300 bg-amber-100 text-amber-800 dark:border-amber-900 dark:bg-amber-950/60 dark:text-amber-300",
        danger: "border-red-300 bg-red-100 text-red-700 dark:border-red-900 dark:bg-red-950/60 dark:text-red-300",
        info: "border-blue-300 bg-blue-100 text-blue-700 dark:border-blue-900 dark:bg-blue-950/60 dark:text-blue-300",
    };
</script>

<span
    class={`inline-flex w-fit items-center rounded-full border px-2 py-0.5 text-[10px] font-bold uppercase tracking-[0.04em] ${toneClassMap[tone]} ${className}`}
>
    {@render children?.()}
</span>
