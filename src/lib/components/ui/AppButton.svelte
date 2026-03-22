<script lang="ts">
    import type { HTMLButtonAttributes } from "svelte/elements";

    type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
    type ButtonSize = "sm" | "md";
    type AppButtonProps = HTMLButtonAttributes & {
        variant?: ButtonVariant;
        size?: ButtonSize;
        children?: import("svelte").Snippet;
    };

    let {
        type = "button",
        disabled = false,
        variant = "secondary",
        size = "sm",
        class: className = "",
        children,
        ...rest
    }: AppButtonProps = $props();

    const variantClassMap: Record<ButtonVariant, string> = {
        primary:
            "border-transparent bg-teal-700 text-white hover:bg-teal-600 dark:bg-teal-600 dark:hover:bg-teal-500",
        secondary:
            "border-zinc-300 bg-white text-zinc-700 hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:hover:bg-zinc-800",
        ghost:
            "border-transparent bg-transparent text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800",
        danger:
            "border-transparent bg-red-600 text-white hover:bg-red-700",
    };

    const sizeClassMap: Record<ButtonSize, string> = {
        sm: "h-8 px-3 text-[11px]",
        md: "h-9 px-3.5 text-xs",
    };

    const baseClass =
        "inline-flex items-center justify-center gap-1.5 rounded-md border font-semibold tracking-[0.01em] transition-colors disabled:opacity-50 disabled:cursor-not-allowed";
</script>

<button
    {type}
    {disabled}
    class={`${baseClass} ${variantClassMap[variant]} ${sizeClassMap[size]} ${className}`}
    {...rest}
>
    {@render children?.()}
</button>
