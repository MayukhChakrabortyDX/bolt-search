<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { open } from "@tauri-apps/plugin-dialog";
    import {
        ExternalLink,
        FolderOpen,
        ListVideo,
        Plus,
        Video,
        X,
    } from "lucide-svelte";
    import AppBadge from "$lib/components/ui/AppBadge.svelte";
    import AppButton from "$lib/components/ui/AppButton.svelte";
    import AppModal from "$lib/components/ui/AppModal.svelte";
    import AppPanel from "$lib/components/ui/AppPanel.svelte";
    import { onMount } from "svelte";

    type DownloadStatus = "queued" | "downloading" | "completed" | "failed";
    type SocialSource = "youtube" | "instagram" | "facebook";

    type DownloadQueueItem = {
        id: number;
        url: string;
        filename: string;
        source: SocialSource;
        playlistMode: boolean;
        playlistFolder: string;
        destinationDir: string | null;
        status: DownloadStatus;
        savedPath: string | null;
        errorMessage: string | null;
        progressPct: number | null;
        progressMessage: string | null;
    };

    type DownloadProgressPayload = {
        job_id: number;
        progress: number | null;
        message: string;
    };

    const SOURCE_TABS: Array<{ id: SocialSource; label: string; placeholder: string }> = [
        {
            id: "youtube",
            label: "YouTube",
            placeholder: "https://www.youtube.com/watch?v=...",
        },
        {
            id: "instagram",
            label: "Instagram",
            placeholder: "https://www.instagram.com/reel/...",
        },
        {
            id: "facebook",
            label: "Facebook",
            placeholder: "https://www.facebook.com/...",
        },
    ];

    let nextDownloadId = 1;
    let queueItems = $state<DownloadQueueItem[]>([]);
    let queueRunning = $state(false);
    let queueStatusMessage = $state("Ready");

    let addModalOpen = $state(false);
    let activeSource = $state<SocialSource>("youtube");
    let youtubeDownloadType = $state<"single" | "playlist">("single");
    let playlistFolderInput = $state("");
    let composerUrl = $state("");
    let composerFilename = $state("");
    let composerDestinationDir = $state<string | null>(null);

    const queuedCount = $derived(
        queueItems.filter((item) => item.status === "queued").length,
    );

    const activeCount = $derived(
        queueItems.filter((item) => item.status === "downloading").length,
    );

    const completedCount = $derived(
        queueItems.filter((item) => item.status === "completed").length,
    );

    const failedCount = $derived(
        queueItems.filter((item) => item.status === "failed").length,
    );

    const activeSourceMeta = $derived(
        SOURCE_TABS.find((entry) => entry.id === activeSource) ?? SOURCE_TABS[0],
    );

    const youtubePreviewUrl = $derived.by(() => {
        if (activeSource !== "youtube") return null;
        return getYouTubeEmbedUrl(composerUrl.trim());
    });

    onMount(() => {
        let unlisten: (() => void) | null = null;

        void (async () => {
            unlisten = await listen<DownloadProgressPayload>(
                "bolt-download-progress",
                (event) => {
                    const payload = event.payload;
                    updateQueueItem(payload.job_id, (item) => ({
                        ...item,
                        progressPct: payload.progress,
                        progressMessage: payload.message,
                    }));
                },
            );
        })();

        return () => {
            if (unlisten) {
                unlisten();
            }
        };
    });

    function openAddModal() {
        addModalOpen = true;
    }

    function closeAddModal() {
        addModalOpen = false;
    }

    function resetComposer() {
        composerUrl = "";
        composerFilename = "";
        playlistFolderInput = "";
        youtubeDownloadType = "single";
    }

    function parseSource(url: string): SocialSource | null {
        try {
            const parsed = new URL(url);
            const host = parsed.hostname.toLowerCase();

            if (
                host === "youtube.com" ||
                host === "www.youtube.com" ||
                host === "m.youtube.com" ||
                host === "youtu.be"
            ) {
                return "youtube";
            }
            if (host === "instagram.com" || host === "www.instagram.com") {
                return "instagram";
            }
            if (
                host === "facebook.com" ||
                host === "www.facebook.com" ||
                host === "m.facebook.com" ||
                host === "fb.watch"
            ) {
                return "facebook";
            }

            return null;
        } catch {
            return null;
        }
    }

    function sourceLabel(source: SocialSource): string {
        switch (source) {
            case "youtube":
                return "YouTube";
            case "instagram":
                return "Instagram";
            case "facebook":
                return "Facebook";
        }
    }

    function sourceHint(source: SocialSource): string {
        switch (source) {
            case "youtube":
                return "Video and shorts URLs are supported.";
            case "instagram":
                return "Reel, post, and video URLs are supported by extractor.";
            case "facebook":
                return "Video post URLs and watch links are supported.";
        }
    }

    function getYouTubeEmbedUrl(url: string): string | null {
        try {
            const parsed = new URL(url);
            const host = parsed.hostname.toLowerCase();

            if (host === "youtu.be") {
                const id = parsed.pathname.split("/").filter(Boolean)[0];
                return id ? `https://www.youtube-nocookie.com/embed/${id}` : null;
            }

            if (host === "youtube.com" || host === "www.youtube.com" || host === "m.youtube.com") {
                const v = parsed.searchParams.get("v");
                if (v) return `https://www.youtube-nocookie.com/embed/${v}`;

                const parts = parsed.pathname.split("/").filter(Boolean);
                const shortsIdx = parts.findIndex((segment) => segment === "shorts");
                if (shortsIdx >= 0 && parts[shortsIdx + 1]) {
                    return `https://www.youtube-nocookie.com/embed/${parts[shortsIdx + 1]}`;
                }
            }

            return null;
        } catch {
            return null;
        }
    }

    async function chooseDestinationDir() {
        try {
            const selected = await open({
                directory: true,
                multiple: false,
                ...(composerDestinationDir ? { defaultPath: composerDestinationDir } : {}),
            });

            if (typeof selected === "string") {
                composerDestinationDir = selected;
            }
        } catch (error) {
            console.error("Failed to choose destination directory:", error);
            queueStatusMessage = "Failed to open folder picker";
        }
    }

    function addToQueue() {
        const trimmedUrl = composerUrl.trim();
        if (!trimmedUrl) {
            queueStatusMessage = "Enter a URL before adding to queue";
            return;
        }

        try {
            const parsed = new URL(trimmedUrl);
            if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
                queueStatusMessage = "Only HTTP(S) URLs are supported";
                return;
            }
        } catch {
            queueStatusMessage = "The URL is invalid";
            return;
        }

        const detectedSource = parseSource(trimmedUrl);
        if (!detectedSource) {
            queueStatusMessage = "Supported sources: YouTube, Instagram, Facebook";
            return;
        }

        if (detectedSource !== activeSource) {
            queueStatusMessage = `URL does not match active ${sourceLabel(activeSource)} tab`;
            return;
        }

        if (activeSource === "youtube" && youtubeDownloadType === "playlist") {
            const parsed = new URL(trimmedUrl);
            const hasListParam = parsed.searchParams.has("list");
            const isPlaylistPath = parsed.pathname.toLowerCase().startsWith("/playlist");
            if (!hasListParam && !isPlaylistPath) {
                queueStatusMessage = "Use a YouTube playlist URL (must include list=...)";
                return;
            }
        }

        queueItems = [
            ...queueItems,
            {
                id: nextDownloadId,
                url: trimmedUrl,
                filename: composerFilename.trim(),
                source: activeSource,
                playlistMode: activeSource === "youtube" && youtubeDownloadType === "playlist",
                playlistFolder: playlistFolderInput.trim(),
                destinationDir: composerDestinationDir,
                status: "queued",
                savedPath: null,
                errorMessage: null,
                progressPct: null,
                progressMessage: null,
            },
        ];

        nextDownloadId += 1;
        queueStatusMessage = `Added to queue: ${sourceLabel(activeSource)}`;
        closeAddModal();
        resetComposer();

        queueMicrotask(() => {
            void runQueue();
        });
    }

    function updateQueueItem(id: number, updater: (item: DownloadQueueItem) => DownloadQueueItem) {
        queueItems = queueItems.map((item) => (item.id === id ? updater(item) : item));
    }

    async function runQueue() {
        if (queueRunning) {
            return;
        }

        const pending = queueItems.filter((item) => item.status === "queued").length;
        if (pending === 0) {
            queueStatusMessage = "Queue is empty";
            return;
        }

        queueRunning = true;
        queueStatusMessage = "Running queue";

        try {
            while (true) {
                const next = queueItems.find((item) => item.status === "queued");
                if (!next) {
                    break;
                }

                updateQueueItem(next.id, (item) => ({
                    ...item,
                    status: "downloading",
                    errorMessage: null,
                    progressPct: 0,
                    progressMessage: "Starting",
                }));

                try {
                    const savedPath = await invoke<string>("social_download", {
                        url: next.url,
                        destinationDir: next.destinationDir,
                        filename: next.filename.length > 0 ? next.filename : null,
                        playlistMode: next.playlistMode,
                        playlistFolder: next.playlistFolder.length > 0 ? next.playlistFolder : null,
                        jobId: next.id,
                    });

                    updateQueueItem(next.id, (item) => ({
                        ...item,
                        status: "completed",
                        savedPath,
                        errorMessage: null,
                        progressPct: 100,
                        progressMessage: "Completed",
                    }));
                } catch (error) {
                    const message =
                        error instanceof Error
                            ? error.message
                            : typeof error === "string"
                              ? error
                              : "Unknown download error";

                    updateQueueItem(next.id, (item) => ({
                        ...item,
                        status: "failed",
                        errorMessage: message,
                        progressMessage: item.progressMessage,
                    }));
                }
            }

            const failed = queueItems.filter((item) => item.status === "failed").length;
            queueStatusMessage = failed > 0 ? "Queue completed with some errors" : "Queue completed";
        } finally {
            queueRunning = false;
        }
    }

    function clearCompleted() {
        queueItems = queueItems.filter((item) => item.status !== "completed");
    }

    function retryFailed() {
        queueItems = queueItems.map((item) =>
            item.status === "failed"
                ? {
                      ...item,
                      status: "queued",
                      errorMessage: null,
                        progressPct: null,
                        progressMessage: null,
                  }
                : item,
        );

        queueMicrotask(() => {
            void runQueue();
        });
    }

    async function revealInExplorer(path: string | null) {
        if (!path) {
            return;
        }

        try {
            await invoke("open_in_explorer", { path });
        } catch (error) {
            console.error("Failed to open path in Explorer:", error);
        }
    }

    function statusLabel(status: DownloadStatus): string {
        switch (status) {
            case "queued":
                return "Queued";
            case "downloading":
                return "Downloading";
            case "completed":
                return "Completed";
            case "failed":
                return "Failed";
        }
    }

    function statusTone(status: DownloadStatus): "neutral" | "warning" | "success" | "danger" {
        switch (status) {
            case "queued":
                return "neutral";
            case "downloading":
                return "warning";
            case "completed":
                return "success";
            case "failed":
                return "danger";
        }
    }
</script>

<svelte:window
    onkeydown={(event) => {
        if (event.key === "Escape" && addModalOpen) {
            closeAddModal();
        }
    }}
/>

<div class="h-full min-h-0 overflow-auto bg-zinc-100/70 p-4 dark:bg-zinc-950 sm:p-5">
    <div class="mx-auto flex w-full max-w-6xl flex-col gap-3">
        <AppPanel class="bg-gradient-to-r from-teal-50 to-white dark:from-zinc-900 dark:to-zinc-900">
            <h1 class="text-lg font-extrabold tracking-[0.01em] text-zinc-900 dark:text-zinc-100">Download Master</h1>
            <p class="mt-1 text-xs text-zinc-600 dark:text-zinc-400">
                Social Download pipeline with source tabs, queue automation, and progressive job status.
            </p>
        </AppPanel>

        <div class="flex flex-wrap items-center gap-2">
            <AppButton variant="primary" onclick={openAddModal}>
                <Plus size={13} strokeWidth={2} />
                Add Download
            </AppButton>
            <AppButton
                variant="secondary"
                onclick={clearCompleted}
                disabled={queueRunning || completedCount === 0}
            >
                Clear Completed
            </AppButton>
            <AppButton
                variant="secondary"
                onclick={retryFailed}
                disabled={queueRunning || failedCount === 0}
            >
                Retry Failed
            </AppButton>
        </div>

        <AppPanel class="flex flex-col gap-3">
            <div class="flex flex-col gap-1 border-b border-zinc-200 pb-2 dark:border-zinc-800">
                <h2 class="text-sm font-bold text-zinc-900 dark:text-zinc-100">Queue</h2>
                <p class="text-[11px] text-zinc-500 dark:text-zinc-400">
                    {queueStatusMessage} | queued: {queuedCount} | active: {activeCount} | completed: {completedCount}
                    | failed: {failedCount}
                </p>
            </div>

            {#if queueItems.length === 0}
                <div class="rounded-lg border border-dashed border-zinc-300 bg-zinc-50 p-4 text-xs text-zinc-500 dark:border-zinc-700 dark:bg-zinc-950 dark:text-zinc-400">
                    No downloads queued yet. Use Add Download to open the source modal.
                </div>
            {:else}
                <ul class="grid gap-2">
                    {#each queueItems as item (item.id)}
                        <li class="flex flex-col gap-3 rounded-xl border border-zinc-200 bg-zinc-50 p-3 dark:border-zinc-800 dark:bg-zinc-950 sm:flex-row sm:items-start sm:justify-between">
                            <div class="grid min-w-0 gap-1.5">
                                <div class="flex flex-wrap items-center gap-1.5">
                                    <AppBadge tone={statusTone(item.status)}>{statusLabel(item.status)}</AppBadge>
                                    <AppBadge tone="info">{sourceLabel(item.source)}</AppBadge>
                                    {#if item.source === "youtube" && item.playlistMode}
                                        <AppBadge tone="warning">Playlist</AppBadge>
                                    {/if}
                                </div>

                                <strong class="overflow-wrap-anywhere text-sm font-semibold text-zinc-900 dark:text-zinc-100">
                                    {item.filename.length > 0 ? item.filename : item.url}
                                </strong>
                                <span class="overflow-wrap-anywhere text-[11px] text-zinc-500 dark:text-zinc-400">
                                    {item.url}
                                </span>

                                {#if item.status === "downloading" || item.progressPct !== null}
                                    <div class="mt-1 grid gap-1">
                                        <div
                                            class="h-2 w-full max-w-xl overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800"
                                            role="progressbar"
                                            aria-valuemin="0"
                                            aria-valuemax="100"
                                            aria-valuenow={Math.round(item.progressPct ?? 0)}
                                        >
                                            <div
                                                class="h-full rounded-full bg-gradient-to-r from-emerald-500 to-sky-500 transition-[width] duration-150"
                                                style={`width: ${Math.max(0, Math.min(100, item.progressPct ?? 0))}%`}
                                            ></div>
                                        </div>
                                        <span class="text-[11px] text-zinc-500 dark:text-zinc-400">
                                            {item.progressMessage ?? `${Math.round(item.progressPct ?? 0)}%`}
                                        </span>
                                    </div>
                                {/if}

                                {#if item.savedPath}
                                    <span class="overflow-wrap-anywhere text-[11px] text-zinc-500 dark:text-zinc-400">
                                        Saved to: {item.savedPath}
                                    </span>
                                {/if}
                                {#if item.source === "youtube" && item.playlistMode && item.playlistFolder.length > 0}
                                    <span class="text-[11px] text-zinc-500 dark:text-zinc-400">
                                        Folder hint: {item.playlistFolder}
                                    </span>
                                {/if}
                                {#if item.errorMessage}
                                    <span class="text-[11px] font-semibold text-red-600 dark:text-red-300">
                                        {item.errorMessage}
                                    </span>
                                {/if}
                            </div>

                            <AppButton
                                variant="secondary"
                                onclick={() => revealInExplorer(item.savedPath)}
                                disabled={!item.savedPath}
                                class="sm:self-start"
                            >
                                <ExternalLink size={12} strokeWidth={2} />
                                Open In Explorer
                            </AppButton>
                        </li>
                    {/each}
                </ul>
            {/if}
        </AppPanel>
    </div>
</div>

<AppModal open={addModalOpen} title="Add Social Download" onClose={closeAddModal} class="max-w-4xl">
    {#snippet headerActions()}
        <AppButton variant="ghost" onclick={closeAddModal} class="h-7 w-7 p-0">
            <X size={14} strokeWidth={2} />
        </AppButton>
    {/snippet}

    <div class="grid gap-3">
        <div class="flex flex-wrap gap-1.5" role="tablist" aria-label="Social source tabs">
            {#each SOURCE_TABS as tab}
                <AppButton
                    variant={activeSource === tab.id ? "primary" : "secondary"}
                    role="tab"
                    aria-selected={activeSource === tab.id}
                    onclick={() => {
                        activeSource = tab.id;
                        composerUrl = "";
                    }}
                >
                    {tab.id === "youtube" ? "YouTube" : tab.id === "instagram" ? "Instagram" : "Facebook"}
                </AppButton>
            {/each}
        </div>

        <p class="text-xs text-zinc-500 dark:text-zinc-400">{sourceHint(activeSource)}</p>

        <label class="grid gap-1.5">
            <span class="text-[11px] font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400">
                {sourceLabel(activeSource)} URL
            </span>
            <input
                type="text"
                bind:value={composerUrl}
                placeholder={activeSourceMeta.placeholder}
                disabled={queueRunning}
                class="h-[34px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 transition-colors focus:border-zinc-500 focus:outline-none dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200"
            />
        </label>

        <div class="grid gap-2 md:grid-cols-2">
            <label class="grid gap-1.5">
                <span class="text-[11px] font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400">
                    Custom filename (optional)
                </span>
                <input
                    type="text"
                    bind:value={composerFilename}
                    placeholder="my-video"
                    disabled={queueRunning}
                    class="h-[34px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 transition-colors focus:border-zinc-500 focus:outline-none dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200"
                />
            </label>

            <label class="grid gap-1.5">
                <span class="text-[11px] font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400">
                    Destination folder
                </span>
                <div class="grid grid-cols-[1fr_auto] gap-1.5">
                    <input
                        type="text"
                        value={composerDestinationDir ?? "Windows Downloads folder (default)"}
                        readonly
                        class="h-[34px] w-full rounded-md border border-zinc-300 bg-zinc-50 px-2 text-xs text-zinc-600 dark:border-zinc-700 dark:bg-zinc-950 dark:text-zinc-300"
                    />
                    <AppButton variant="secondary" onclick={chooseDestinationDir} disabled={queueRunning}>
                        <FolderOpen size={12} strokeWidth={2} />
                        Choose
                    </AppButton>
                </div>
            </label>
        </div>

        {#if activeSource === "youtube"}
            <AppPanel class="grid gap-3 bg-zinc-50 dark:bg-zinc-950">
                <div class="flex items-center gap-2">
                    <Video size={14} strokeWidth={2} class="text-zinc-500 dark:text-zinc-400" />
                    <h4 class="text-xs font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400">
                        YouTube Download Type
                    </h4>
                </div>

                <div class="flex flex-wrap gap-1.5">
                    <AppButton
                        variant={youtubeDownloadType === "single" ? "primary" : "secondary"}
                        onclick={() => {
                            youtubeDownloadType = "single";
                        }}
                    >
                        Single Video
                    </AppButton>
                    <AppButton
                        variant={youtubeDownloadType === "playlist" ? "primary" : "secondary"}
                        onclick={() => {
                            youtubeDownloadType = "playlist";
                        }}
                    >
                        <ListVideo size={12} strokeWidth={2} />
                        Entire Playlist
                    </AppButton>
                </div>

                {#if youtubeDownloadType === "playlist"}
                    <label class="grid gap-1.5">
                        <span class="text-[11px] font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400">
                            Playlist folder name (optional)
                        </span>
                        <input
                            type="text"
                            bind:value={playlistFolderInput}
                            placeholder="Roadtrip Mix"
                            disabled={queueRunning}
                            class="h-[34px] w-full rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-700 transition-colors focus:border-zinc-500 focus:outline-none dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200"
                        />
                    </label>
                {/if}
            </AppPanel>

            <AppPanel class="grid gap-2 bg-zinc-50 dark:bg-zinc-950">
                <h4 class="text-xs font-bold uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400">Preview</h4>
                {#if youtubePreviewUrl}
                    <iframe
                        title="YouTube preview"
                        src={youtubePreviewUrl}
                        loading="lazy"
                        referrerpolicy="strict-origin-when-cross-origin"
                        allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
                        allowfullscreen
                        class="block aspect-video w-full rounded-md border-0 bg-black"
                    ></iframe>
                {:else}
                    <div class="rounded-md border border-dashed border-zinc-300 bg-white p-3 text-xs text-zinc-500 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-400">
                        Paste a valid YouTube link to preview the video.
                    </div>
                {/if}
            </AppPanel>
        {/if}

        <div class="mt-1 flex justify-end gap-2">
            <AppButton variant="secondary" onclick={closeAddModal}>Cancel</AppButton>
            <AppButton variant="primary" onclick={addToQueue} disabled={queueRunning}>
                Add And Start
            </AppButton>
        </div>
    </div>
</AppModal>
