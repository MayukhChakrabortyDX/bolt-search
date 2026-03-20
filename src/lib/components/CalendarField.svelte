<script lang="ts">
    import { onMount } from "svelte";
    import { Calendar, X } from "lucide-svelte";
    import "./CalendarField.css";
    import {
        WEEKDAY_LABELS,
        YEAR_OPTIONS,
        MONTH_OPTIONS,
        buildCalendarCells,
        clampMonthToBounds,
        dateLabelFormatter,
        formatIsoDate,
        parseIsoDate,
        startOfMonth,
    } from "./CalendarField.utils";

    let {
        value = $bindable(""),
        ariaLabel = "Choose date",
        placeholder = "Select date",
        containerClass = "",
    }: {
        value?: string;
        ariaLabel?: string;
        placeholder?: string;
        containerClass?: string;
    } = $props();

    let open = $state(false);
    let yearListEl = $state<HTMLDivElement | null>(null);
    let activeMonth = $state(startOfMonth(parseIsoDate(value) ?? new Date()));
    let lastSyncedValue = $state(value);

    const selectedDate = $derived(parseIsoDate(value));
    const displayLabel = $derived.by(() => {
        const parsed = parseIsoDate(value);
        return parsed ? dateLabelFormatter.format(parsed) : placeholder;
    });
    const calendarCells = $derived(buildCalendarCells(activeMonth));

    $effect(() => {
        if (value === lastSyncedValue) {
            return;
        }

        lastSyncedValue = value;
        const parsed = parseIsoDate(value);
        if (!parsed) return;

        activeMonth = clampMonthToBounds(startOfMonth(parsed));
    });

    function setActiveMonth(year: number, monthIndex: number) {
        activeMonth = clampMonthToBounds(new Date(year, monthIndex, 1));
    }

    function chooseMonth(monthIndex: number) {
        setActiveMonth(activeMonth.getFullYear(), monthIndex);
    }

    function chooseYear(year: number) {
        setActiveMonth(year, activeMonth.getMonth());
    }

    function scrollActiveYearIntoView() {
        if (!yearListEl) return;
        const activeYear = activeMonth.getFullYear();
        const selected = yearListEl.querySelector(
            `[data-year="${activeYear}"]`,
        ) as HTMLElement | null;
        selected?.scrollIntoView({ block: "center" });
    }

    function openCalendar() {
        if (open) {
            closeCalendar();
            return;
        }

        activeMonth = clampMonthToBounds(
            startOfMonth(parseIsoDate(value) ?? new Date()),
        );
        open = true;
        requestAnimationFrame(scrollActiveYearIntoView);
    }

    function closeCalendar() {
        open = false;
    }

    function chooseDate(iso: string) {
        value = value === iso ? "" : iso;
        closeCalendar();
    }

    function clearDate() {
        value = "";
    }

    function chooseToday() {
        const today = new Date();
        value = formatIsoDate(today);
        activeMonth = startOfMonth(today);
        closeCalendar();
    }

    function onBackdropClick(event: MouseEvent) {
        if (event.target === event.currentTarget) {
            closeCalendar();
        }
    }

    function onBackdropKeyDown(event: KeyboardEvent) {
        if (event.key === "Escape") {
            closeCalendar();
        }
    }

    onMount(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key === "Escape") {
                closeCalendar();
            }
        };

        window.addEventListener("keydown", onKeyDown);
        return () => {
            window.removeEventListener("keydown", onKeyDown);
        };
    });
</script>

<div class={`w-full ${containerClass}`}>
    <div class="flex w-full items-center gap-1">
        <button
            type="button"
            aria-label={ariaLabel}
            aria-haspopup="dialog"
            aria-expanded={open}
            class="inline-flex h-7.5 w-full items-center justify-between gap-2 rounded-md border border-zinc-300 bg-white px-2 text-left text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 transition-colors hover:bg-zinc-50 dark:hover:bg-zinc-800 focus-visible:border-zinc-500 focus-visible:outline-none"
            onclick={openCalendar}
        >
            <span class={`truncate ${selectedDate ? "" : "text-zinc-400 dark:text-zinc-500"}`}>
                {displayLabel}
            </span>
            <Calendar size={13} class="shrink-0 text-zinc-500 dark:text-zinc-400" strokeWidth={2} />
        </button>

        {#if value.trim()}
            <button
                type="button"
                class="inline-flex h-7.5 w-7.5 items-center justify-center rounded-md border border-zinc-300 bg-white text-zinc-500 hover:bg-zinc-50 hover:text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-zinc-200"
                aria-label="Clear date"
                onclick={clearDate}
            >
                <X size={12} strokeWidth={2} />
            </button>
        {/if}
    </div>

    {#if open}
        <div
            class="fixed inset-0 z-120 flex items-center justify-center bg-zinc-950/55 p-4"
            role="dialog"
            aria-modal="true"
            aria-label={ariaLabel}
            tabindex="-1"
            onclick={onBackdropClick}
            onkeydown={onBackdropKeyDown}
        >
            <div class="grid h-120 w-160 grid-cols-[125px_minmax(0,1fr)] overflow-hidden rounded-xl border border-zinc-300 bg-white shadow-2xl dark:border-zinc-700 dark:bg-zinc-900">
                <aside class="flex min-h-0 flex-col border-r border-zinc-300 py-1 bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-950/50">
                    <div class="year-scroll-overlay flex-1 overflow-y-auto p-1.5" bind:this={yearListEl}>
                        <div class="year-scroll-content">
                            {#each YEAR_OPTIONS as year}
                                <button
                                    type="button"
                                    data-year={year}
                                    class={`mb-1 w-full rounded-md px-2 py-1 text-center text-[11px] font-semibold transition-colors ${activeMonth.getFullYear() === year
                                        ? "bg-teal-700 text-white"
                                        : "text-zinc-600 hover:bg-zinc-200 dark:text-zinc-300 dark:hover:bg-zinc-800"}`}
                                    onclick={() => chooseYear(year)}
                                >
                                    {year}
                                </button>
                            {/each}
                        </div>
                    </div>
                </aside>

                <section class="grid min-w-0 grid-rows-[auto_1fr_auto]">
                    <div class="border-b border-zinc-300 px-2 py-2 dark:border-zinc-700">
                        <div class="mb-2 flex items-center justify-between">
                            <span class="text-[10px] font-bold uppercase tracking-[0.08em] text-zinc-500 dark:text-zinc-400">
                                Month
                            </span>
                            <span class="text-[11px] font-semibold text-zinc-600 dark:text-zinc-300">
                                {MONTH_OPTIONS[activeMonth.getMonth()]?.label} {activeMonth.getFullYear()}
                            </span>
                            <button
                                type="button"
                                class="inline-flex h-6 w-6 items-center justify-center rounded-md text-zinc-500 hover:bg-zinc-100 hover:text-zinc-700 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
                                aria-label="Close calendar"
                                onclick={closeCalendar}
                            >
                                <X size={12} strokeWidth={2} />
                            </button>
                        </div>

                        <div class="flex gap-1 overflow-x-auto pb-1">
                            {#each MONTH_OPTIONS as option}
                                <button
                                    type="button"
                                    class={`shrink-0 rounded-md px-2 py-1 text-[11px] font-semibold transition-colors ${activeMonth.getMonth() === option.value
                                        ? "bg-teal-700 text-white"
                                        : "bg-zinc-100 text-zinc-600 hover:bg-zinc-200 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700"}`}
                                    onclick={() => chooseMonth(option.value)}
                                >
                                    {option.label}
                                </button>
                            {/each}
                        </div>
                    </div>

                    <div class="overflow-y-auto p-3">
                        <div class="mb-1 grid grid-cols-7 gap-1">
                            {#each WEEKDAY_LABELS as weekday}
                                <span class="py-1 text-center text-[10px] font-semibold uppercase tracking-[0.05em] text-zinc-400 dark:text-zinc-500">
                                    {weekday}
                                </span>
                            {/each}
                        </div>

                        <div class="grid grid-cols-7 gap-1" role="grid" aria-label={ariaLabel}>
                            {#each calendarCells as cell (cell.iso)}
                                <button
                                    type="button"
                                    class={`inline-flex h-9 items-center justify-center rounded-md text-[11px] transition-colors ${value === cell.iso
                                        ? "bg-teal-700 text-white"
                                        : cell.inCurrentMonth
                                          ? "text-zinc-700 hover:bg-zinc-100 dark:text-zinc-200 dark:hover:bg-zinc-800"
                                          : "text-zinc-400 hover:bg-zinc-100 dark:text-zinc-600 dark:hover:bg-zinc-800"} ${cell.isToday && value !== cell.iso
                                        ? "ring-1 ring-zinc-400 dark:ring-zinc-500"
                                        : ""}`}
                                    onclick={() => chooseDate(cell.iso)}
                                >
                                    {cell.day}
                                </button>
                            {/each}
                        </div>
                    </div>

                    <div class="flex items-center justify-between border-t border-zinc-300 px-3 py-2 dark:border-zinc-700">
                        <button
                            type="button"
                            class="rounded-md px-2 py-1 text-[11px] font-semibold text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
                            onclick={chooseToday}
                        >
                            Today
                        </button>

                        <div class="flex items-center gap-1">
                            {#if value.trim()}
                                <button
                                    type="button"
                                    class="rounded-md px-2 py-1 text-[11px] font-semibold text-zinc-500 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
                                    onclick={clearDate}
                                >
                                    Clear
                                </button>
                            {/if}
                            <button
                                type="button"
                                class="rounded-md border border-zinc-300 px-2 py-1 text-[11px] font-semibold text-zinc-600 hover:bg-zinc-100 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800"
                                onclick={closeCalendar}
                            >
                                Close
                            </button>
                        </div>
                    </div>
                </section>
            </div>
        </div>
    {/if}
</div>
