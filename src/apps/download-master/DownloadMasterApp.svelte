<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { open } from "@tauri-apps/plugin-dialog";
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
</script>

<svelte:window
    onkeydown={(event) => {
        if (event.key === "Escape" && addModalOpen) {
            closeAddModal();
        }
    }}
/>

<div class="download-app-shell">
    <section class="download-app-header">
        <h1>Download Master</h1>
        <p>Social Download v2: tabbed source workflow with instant queue processing.</p>
    </section>

    <section class="download-toolbar">
        <button type="button" class="primary" onclick={openAddModal}>
            Add Download
        </button>
        <button type="button" class="muted" onclick={clearCompleted} disabled={queueRunning || completedCount === 0}>
            Clear Completed
        </button>
        <button type="button" class="muted" onclick={retryFailed} disabled={queueRunning || failedCount === 0}>
            Retry Failed
        </button>
    </section>

    <section class="download-queue-panel">
        <div class="download-queue-header">
            <h2>Queue</h2>
            <p>
                {queueStatusMessage} | queued: {queuedCount} | active: {activeCount} | completed: {completedCount} |
                failed: {failedCount}
            </p>
        </div>

        {#if queueItems.length === 0}
            <div class="download-empty-state">
                No downloads queued yet. Use Add Download to open the source modal.
            </div>
        {:else}
            <ul class="download-queue-list">
                {#each queueItems as item (item.id)}
                    <li class="download-queue-item">
                        <div class="download-queue-main">
                            <div class="download-badge-row">
                                <span class={`pill ${item.status}`}>{statusLabel(item.status)}</span>
                                <span class="source-pill">{sourceLabel(item.source)}</span>
                                {#if item.source === "youtube" && item.playlistMode}
                                    <span class="playlist-pill">Playlist</span>
                                {/if}
                            </div>
                            <strong>{item.filename.length > 0 ? item.filename : item.url}</strong>
                            <span class="muted">{item.url}</span>
                            {#if item.status === "downloading" || item.progressPct !== null}
                                <div class="download-progress-block">
                                    <div class="download-progress-track" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(item.progressPct ?? 0)}>
                                        <div
                                            class="download-progress-fill"
                                            style={`width: ${Math.max(0, Math.min(100, item.progressPct ?? 0))}%`}
                                        ></div>
                                    </div>
                                    <span class="muted">
                                        {item.progressMessage ?? `${Math.round(item.progressPct ?? 0)}%`}
                                    </span>
                                </div>
                            {/if}
                            {#if item.savedPath}
                                <span class="muted">Saved to: {item.savedPath}</span>
                            {/if}
                            {#if item.source === "youtube" && item.playlistMode && item.playlistFolder.length > 0}
                                <span class="muted">Folder hint: {item.playlistFolder}</span>
                            {/if}
                            {#if item.errorMessage}
                                <span class="error">{item.errorMessage}</span>
                            {/if}
                        </div>
                        <div class="download-queue-actions">
                            <button
                                type="button"
                                class="muted"
                                onclick={() => revealInExplorer(item.savedPath)}
                                disabled={!item.savedPath}
                            >
                                Open In Explorer
                            </button>
                        </div>
                    </li>
                {/each}
            </ul>
        {/if}
    </section>
</div>

{#if addModalOpen}
    <div class="add-modal-overlay">
        <button class="add-modal-backdrop" type="button" aria-label="Close" onclick={closeAddModal}></button>
        <section class="add-modal">
            <div class="add-modal-header">
                <h3>Add Social Download</h3>
                <button type="button" class="close" onclick={closeAddModal}>Close</button>
            </div>

            <div class="source-tabs" role="tablist" aria-label="Social source tabs">
                {#each SOURCE_TABS as tab}
                    <button
                        type="button"
                        class={`source-tab ${activeSource === tab.id ? "active" : ""}`}
                        role="tab"
                        aria-selected={activeSource === tab.id}
                        onclick={() => {
                            activeSource = tab.id;
                            composerUrl = "";
                        }}
                    >
                        {tab.label}
                    </button>
                {/each}
            </div>

            <p class="source-hint">{sourceHint(activeSource)}</p>

            <label>
                <span>{sourceLabel(activeSource)} URL</span>
                <input
                    type="text"
                    bind:value={composerUrl}
                    placeholder={activeSourceMeta.placeholder}
                    disabled={queueRunning}
                />
            </label>

            <div class="modal-grid">
                <label>
                    <span>Custom filename (optional)</span>
                    <input
                        type="text"
                        bind:value={composerFilename}
                        placeholder="my-video"
                        disabled={queueRunning}
                    />
                </label>

                <label>
                    <span>Destination folder</span>
                    <div class="destination-picker">
                        <input
                            type="text"
                            value={composerDestinationDir ?? "Windows Downloads folder (default)"}
                            readonly
                        />
                        <button type="button" onclick={chooseDestinationDir} disabled={queueRunning}>Choose</button>
                    </div>
                </label>
            </div>

            {#if activeSource === "youtube"}
                <div class="youtube-subsection">
                    <h4>YouTube Download Type</h4>
                    <div class="youtube-type-row">
                        <button
                            type="button"
                            class={`youtube-type-button ${youtubeDownloadType === "single" ? "active" : ""}`}
                            onclick={() => {
                                youtubeDownloadType = "single";
                            }}
                        >
                            Single Video
                        </button>
                        <button
                            type="button"
                            class={`youtube-type-button ${youtubeDownloadType === "playlist" ? "active" : ""}`}
                            onclick={() => {
                                youtubeDownloadType = "playlist";
                            }}
                        >
                            Entire Playlist
                        </button>
                    </div>

                    {#if youtubeDownloadType === "playlist"}
                        <label>
                            <span>Playlist folder name (optional)</span>
                            <input
                                type="text"
                                bind:value={playlistFolderInput}
                                placeholder="Roadtrip Mix"
                                disabled={queueRunning}
                            />
                        </label>
                    {/if}
                </div>

                <div class="youtube-preview">
                    <h4>Preview</h4>
                    {#if youtubePreviewUrl}
                        <iframe
                            title="YouTube preview"
                            src={youtubePreviewUrl}
                            loading="lazy"
                            referrerpolicy="strict-origin-when-cross-origin"
                            allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
                            allowfullscreen
                        ></iframe>
                    {:else}
                        <div class="preview-empty">Paste a valid YouTube link to preview the video.</div>
                    {/if}
                </div>
            {/if}

            <div class="add-modal-actions">
                <button type="button" class="muted" onclick={closeAddModal}>Cancel</button>
                <button type="button" class="primary" onclick={addToQueue} disabled={queueRunning}>Add And Start</button>
            </div>
        </section>
    </div>
{/if}

<style>
    .download-app-shell {
        height: 100%;
        overflow: auto;
        padding: 22px;
        color: var(--text);
        background:
            radial-gradient(circle at 12% 10%, rgba(15, 118, 110, 0.11), transparent 42%),
            radial-gradient(circle at 82% 4%, rgba(34, 197, 94, 0.11), transparent 38%),
            var(--surface);
    }

    .download-app-header h1 {
        margin: 0;
        font-size: 1.6rem;
        letter-spacing: 0.01em;
    }

    .download-app-header p {
        margin: 8px 0 0;
        color: var(--text-muted);
        font-size: 0.92rem;
    }

    .download-toolbar {
        margin-top: 14px;
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
    }

    .download-queue-panel {
        margin-top: 16px;
        border: 1px solid var(--stroke);
        border-radius: 12px;
        background: var(--panel);
        padding: 14px;
    }

    .download-toolbar button,
    .download-queue-actions button,
    .destination-picker button,
    .add-modal .close,
    .add-modal-actions button,
    .source-tab {
        border: 1px solid var(--control-border);
        background: var(--control-bg);
        color: var(--control-text);
        border-radius: 8px;
        padding: 8px 12px;
        cursor: pointer;
        font-size: 0.78rem;
        font-weight: 600;
        transition: background-color 0.15s ease, border-color 0.15s ease;
    }

    .download-toolbar button.primary,
    .add-modal-actions button.primary {
        background: color-mix(in srgb, var(--accent) 84%, #ffffff);
        color: #ffffff;
        border-color: color-mix(in srgb, var(--accent) 76%, #000000);
    }

    .download-toolbar button:hover,
    .download-queue-actions button:hover,
    .destination-picker button:hover,
    .add-modal .close:hover,
    .add-modal-actions button:hover,
    .source-tab:hover {
        background: var(--control-bg-hover);
    }

    .download-toolbar button.primary:hover,
    .add-modal-actions button.primary:hover {
        background: color-mix(in srgb, var(--accent) 92%, #ffffff);
    }

    .download-toolbar button.muted,
    .download-queue-actions button.muted,
    .add-modal-actions button.muted {
        color: var(--text-muted);
    }

    button:disabled {
        opacity: 0.55;
        cursor: not-allowed;
    }

    .download-queue-header h2 {
        margin: 0;
        font-size: 1rem;
    }

    .download-queue-header p {
        margin: 4px 0 0;
        color: var(--text-muted);
        font-size: 0.75rem;
    }

    .download-empty-state {
        margin-top: 10px;
        border: 1px dashed var(--stroke);
        border-radius: 8px;
        padding: 16px;
        color: var(--text-muted);
        font-size: 0.86rem;
    }

    .download-queue-list {
        list-style: none;
        margin: 10px 0 0;
        padding: 0;
        display: grid;
        gap: 8px;
    }

    .download-queue-item {
        border: 1px solid var(--stroke);
        border-radius: 10px;
        padding: 10px;
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 12px;
        background: var(--panel-soft);
    }

    .download-queue-main {
        display: grid;
        gap: 4px;
        min-width: 0;
    }

    .download-queue-main strong {
        font-size: 0.86rem;
        line-height: 1.3;
        overflow-wrap: anywhere;
    }

    .download-badge-row {
        display: flex;
        align-items: center;
        gap: 8px;
    }

    .muted {
        color: var(--text-muted);
        font-size: 0.75rem;
        overflow-wrap: anywhere;
    }

    .error {
        color: #dc2626;
        font-size: 0.76rem;
        font-weight: 600;
    }

    .pill {
        display: inline-flex;
        width: fit-content;
        border-radius: 999px;
        padding: 2px 8px;
        font-size: 0.67rem;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        border: 1px solid transparent;
    }

    .pill.queued {
        color: #0f766e;
        background: rgba(20, 184, 166, 0.15);
        border-color: rgba(20, 184, 166, 0.35);
    }

    .pill.downloading {
        color: #7c2d12;
        background: rgba(251, 146, 60, 0.2);
        border-color: rgba(251, 146, 60, 0.35);
    }

    .pill.completed {
        color: #166534;
        background: rgba(74, 222, 128, 0.2);
        border-color: rgba(74, 222, 128, 0.35);
    }

    .pill.failed {
        color: #991b1b;
        background: rgba(248, 113, 113, 0.18);
        border-color: rgba(248, 113, 113, 0.35);
    }

    .source-pill {
        display: inline-flex;
        width: fit-content;
        border-radius: 999px;
        padding: 2px 8px;
        font-size: 0.65rem;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: #1d4ed8;
        background: rgba(59, 130, 246, 0.14);
        border: 1px solid rgba(59, 130, 246, 0.35);
    }

    .playlist-pill {
        display: inline-flex;
        width: fit-content;
        border-radius: 999px;
        padding: 2px 8px;
        font-size: 0.65rem;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: #92400e;
        background: rgba(245, 158, 11, 0.22);
        border: 1px solid rgba(245, 158, 11, 0.4);
    }

    .download-progress-block {
        display: grid;
        gap: 5px;
        margin-top: 2px;
    }

    .download-progress-track {
        width: min(420px, 100%);
        height: 8px;
        border-radius: 999px;
        background: color-mix(in srgb, var(--control-border) 62%, transparent);
        overflow: hidden;
    }

    .download-progress-fill {
        height: 100%;
        border-radius: inherit;
        background: linear-gradient(90deg, #10b981, #0ea5e9);
        transition: width 0.18s ease;
    }

    .add-modal-overlay {
        position: fixed;
        inset: 0;
        z-index: 120;
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 18px;
    }

    .add-modal-backdrop {
        position: absolute;
        inset: 0;
        border: 0;
        background: rgba(8, 12, 15, 0.7);
    }

    .add-modal {
        position: relative;
        width: min(860px, 100%);
        max-height: 86vh;
        overflow: auto;
        border-radius: 14px;
        border: 1px solid var(--stroke);
        background: var(--panel);
        box-shadow: 0 28px 74px rgba(0, 0, 0, 0.36);
        padding: 14px;
    }

    .add-modal-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        margin-bottom: 12px;
    }

    .add-modal-header h3 {
        margin: 0;
        font-size: 1.02rem;
        color: var(--text);
    }

    .source-tabs {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
    }

    .source-tab.active {
        border-color: color-mix(in srgb, var(--accent) 50%, var(--control-border));
        background: color-mix(in srgb, var(--accent) 14%, var(--control-bg));
        color: var(--text);
    }

    .source-hint {
        margin: 10px 0;
        font-size: 0.76rem;
        color: var(--text-muted);
    }

    .add-modal label {
        display: grid;
        gap: 6px;
        margin-top: 10px;
        font-size: 0.8rem;
        color: var(--text-muted);
    }

    .add-modal input {
        width: 100%;
        border: 1px solid var(--control-border);
        background: var(--control-bg);
        color: var(--control-text);
        border-radius: 8px;
        padding: 8px 10px;
        font-size: 0.86rem;
    }

    .modal-grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
        gap: 10px;
    }

    .destination-picker {
        display: grid;
        grid-template-columns: 1fr auto;
        gap: 8px;
    }

    .youtube-preview {
        margin-top: 12px;
        border: 1px solid var(--stroke);
        background: var(--panel-soft);
        border-radius: 10px;
        padding: 10px;
    }

    .youtube-subsection {
        margin-top: 12px;
        border: 1px solid var(--stroke);
        background: var(--panel-soft);
        border-radius: 10px;
        padding: 10px;
    }

    .youtube-subsection h4 {
        margin: 0 0 8px;
        font-size: 0.82rem;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: var(--text-muted);
    }

    .youtube-type-row {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
    }

    .youtube-type-button {
        border: 1px solid var(--control-border);
        background: var(--control-bg);
        color: var(--control-text);
        border-radius: 8px;
        padding: 8px 12px;
        cursor: pointer;
        font-size: 0.74rem;
        font-weight: 700;
    }

    .youtube-type-button.active {
        border-color: color-mix(in srgb, var(--accent) 50%, var(--control-border));
        background: color-mix(in srgb, var(--accent) 15%, var(--control-bg));
    }

    .youtube-preview h4 {
        margin: 0 0 8px;
        font-size: 0.82rem;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: var(--text-muted);
    }

    .youtube-preview iframe {
        display: block;
        width: 100%;
        aspect-ratio: 16 / 9;
        border: 0;
        border-radius: 8px;
        background: #000000;
    }

    .preview-empty {
        border: 1px dashed var(--stroke);
        border-radius: 8px;
        padding: 12px;
        color: var(--text-muted);
        font-size: 0.8rem;
    }

    .add-modal-actions {
        margin-top: 14px;
        display: flex;
        justify-content: flex-end;
        gap: 8px;
    }

    @media (max-width: 720px) {
        .download-app-shell {
            padding: 14px;
        }

        .download-queue-item {
            flex-direction: column;
        }

        .add-modal {
            padding: 12px;
        }
    }
</style>
