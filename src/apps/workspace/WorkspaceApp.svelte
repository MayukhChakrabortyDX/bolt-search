<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { open } from "@tauri-apps/plugin-dialog";
    import { onMount } from "svelte";
    import AppBadge from "../../lib/components/ui/AppBadge.svelte";
    import AppButton from "../../lib/components/ui/AppButton.svelte";
    import AppModal from "../../lib/components/ui/AppModal.svelte";
    import {
        WORKSPACE_BOLT_FILE_NAME,
        type WorkspaceBoltJson,
        WorkspaceManifest,
    } from "./workspace-schema";
    import { BOLT_TOOLS_INDEX_FILE, type WorkspaceToolJson } from "./workspace-tools";
    import WorkspaceEditor from "./components/WorkspaceEditor.svelte";

    type WorkspaceHistoryItem = {
        id: string;
        name: string;
        path: string;
        lastOpenedAt: string;
        source: "created" | "opened";
    };

    const HISTORY_STORAGE_KEY = "bolt-workspace-history";
    const MANIFEST_JSON_SPACES = 2;

    type ActiveWorkspace = {
        name: string;
        path: string;
        tools: WorkspaceToolJson[];
    };

    type WorkspaceRoute =
        | { id: "home" }
        | {
              id: "editor";
              workspace: ActiveWorkspace;
          };

    let createModalOpen = $state(false);
    let openModalOpen = $state(false);
    let statusMessage = $state<string | null>(null);
    let statusTone = $state<"success" | "info" | "warning">("info");
    let workspaceHistory = $state<WorkspaceHistoryItem[]>([]);
    let workspaceRoute = $state<WorkspaceRoute>({ id: "home" });

    let createName = $state("");
    let createPath = $state("");
    let openPath = $state("");
    let selectingCreateFolder = $state(false);
    let selectingOpenFolder = $state(false);

    const activeWorkspace = $derived.by(() =>
        workspaceRoute.id === "editor" ? workspaceRoute.workspace : null,
    );

    function readHistoryFromStorage(): WorkspaceHistoryItem[] {
        const raw = localStorage.getItem(HISTORY_STORAGE_KEY);
        if (!raw) {
            return [];
        }

        try {
            const parsed = JSON.parse(raw) as unknown;
            if (!Array.isArray(parsed)) {
                return [];
            }

            return parsed
                .filter((entry) => typeof entry === "object" && entry !== null)
                .map((entry) => entry as Record<string, unknown>)
                .map((entry): WorkspaceHistoryItem | null => {
                    const id = typeof entry.id === "string" ? entry.id.trim() : "";
                    const name = typeof entry.name === "string" ? entry.name.trim() : "";
                    const path = typeof entry.path === "string" ? entry.path.trim() : "";
                    const lastOpenedAt =
                        typeof entry.lastOpenedAt === "string" ? entry.lastOpenedAt.trim() : "";
                    const source =
                        entry.source === "created"
                            ? "created"
                            : entry.source === "opened"
                              ? "opened"
                              : null;

                    if (!id || !name || !path || !lastOpenedAt || !source) {
                        return null;
                    }

                    return {
                        id,
                        name,
                        path,
                        lastOpenedAt,
                        source,
                    };
                })
                .filter((entry): entry is WorkspaceHistoryItem => entry !== null)
                .slice(0, 24);
        } catch {
            return [];
        }
    }

    function writeHistoryToStorage(next: WorkspaceHistoryItem[]) {
        localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(next));
    }

    function rememberWorkspace(item: WorkspaceHistoryItem) {
        const deduped = workspaceHistory.filter(
            (existing) => existing.path.toLowerCase() !== item.path.toLowerCase(),
        );
        workspaceHistory = [item, ...deduped].slice(0, 24);
        writeHistoryToStorage(workspaceHistory);
    }

    function basenameFromPath(path: string): string {
        const normalized = path.replace(/\\+/g, "/").replace(/\/+$/, "").trim();
        if (!normalized) {
            return "Untitled Workspace";
        }

        const chunks = normalized.split("/").filter((segment) => segment.length > 0);
        return chunks[chunks.length - 1] ?? "Untitled Workspace";
    }

    function openCreateModal() {
        createModalOpen = true;
        openModalOpen = false;
        statusMessage = null;
    }

    function openOpenModal() {
        openModalOpen = true;
        createModalOpen = false;
        statusMessage = null;
    }

    function closeCreateModal() {
        createModalOpen = false;
    }

    function closeOpenModal() {
        openModalOpen = false;
    }

    function navigateToHome() {
        workspaceRoute = { id: "home" };
    }

    function navigateToEditor(workspace: ActiveWorkspace) {
        workspaceRoute = {
            id: "editor",
            workspace,
        };
    }

    async function pickCreateFolder() {
        selectingCreateFolder = true;

        try {
            const selected = await open({
                directory: true,
                multiple: false,
                title: "Choose a folder for this workspace",
            });

            if (typeof selected === "string" && selected.trim()) {
                createPath = selected.trim();
            }
        } catch {
            statusTone = "warning";
            statusMessage = "Could not open folder picker. Please try again.";
        } finally {
            selectingCreateFolder = false;
        }
    }

    async function pickOpenFolder() {
        selectingOpenFolder = true;

        try {
            const selected = await open({
                directory: true,
                multiple: false,
                title: "Choose an existing workspace folder",
            });

            if (typeof selected === "string" && selected.trim()) {
                openPath = selected.trim();
            }
        } catch {
            statusTone = "warning";
            statusMessage = "Could not open folder picker. Please try again.";
        } finally {
            selectingOpenFolder = false;
        }
    }

    async function submitCreateWorkspace() {
        const trimmedName = createName.trim();
        const trimmedPath = createPath.trim();

        if (!trimmedName) {
            statusTone = "warning";
            statusMessage = "Workspace name is required.";
            return;
        }

        if (!trimmedPath) {
            statusTone = "warning";
            statusMessage = "Please select a folder for this workspace.";
            return;
        }

        const manifest = WorkspaceManifest.create(trimmedName);
        const json: WorkspaceBoltJson = manifest.toJSON();
        WorkspaceManifest.parse(json);

        try {
            await invoke("write_workspace_file", {
                root: trimmedPath,
                relativePath: WORKSPACE_BOLT_FILE_NAME,
                content: JSON.stringify(json, null, MANIFEST_JSON_SPACES),
            });

            await invoke("write_workspace_file", {
                root: trimmedPath,
                relativePath: BOLT_TOOLS_INDEX_FILE,
                content: JSON.stringify([], null, MANIFEST_JSON_SPACES),
            });
        } catch (error) {
            statusTone = "warning";
            statusMessage = error instanceof Error ? error.message : "Failed to create workspace.";
            return;
        }

        rememberWorkspace({
            id: `${Date.now()}-created`,
            name: manifest.name,
            path: trimmedPath,
            lastOpenedAt: new Date().toISOString(),
            source: "created",
        });

        statusTone = "success";
        statusMessage = `${manifest.name} is ready. You can find it in your recent workspaces below.`;
        navigateToEditor({
            name: manifest.name,
            path: trimmedPath,
            tools: manifest.tools,
        });
        createModalOpen = false;
        createName = "";
        createPath = "";
    }

    async function submitOpenWorkspace() {
        const trimmedPath = openPath.trim();

        if (!trimmedPath) {
            statusTone = "warning";
            statusMessage = "Please select a workspace folder to open.";
            return;
        }

        let inferredName = basenameFromPath(trimmedPath);
        let parsedTools: WorkspaceToolJson[] = [];

        try {
            const manifestRaw = await invoke<string>("read_workspace_file", {
                root: trimmedPath,
                relativePath: WORKSPACE_BOLT_FILE_NAME,
            });
            const manifest = WorkspaceManifest.parse(manifestRaw);
            inferredName = manifest.name;
            parsedTools = manifest.tools;
        } catch {
            statusTone = "warning";
            statusMessage = `Could not find a valid ${WORKSPACE_BOLT_FILE_NAME} in the selected folder.`;
            return;
        }

        rememberWorkspace({
            id: `${Date.now()}-opened`,
            name: inferredName,
            path: trimmedPath,
            lastOpenedAt: new Date().toISOString(),
            source: "opened",
        });

        statusTone = "info";
        statusMessage = `${inferredName} is now in your recent workspaces.`;
        navigateToEditor({
            name: inferredName,
            path: trimmedPath,
            tools: parsedTools,
        });
        openModalOpen = false;
        openPath = "";
    }

    function closeWorkspaceEditor() {
        navigateToHome();
    }

    function removeHistoryItem(id: string) {
        workspaceHistory = workspaceHistory.filter((item) => item.id !== id);
        writeHistoryToStorage(workspaceHistory);
    }

    function formatLastOpened(iso: string): string {
        const parsed = new Date(iso);
        if (Number.isNaN(parsed.getTime())) {
            return "Unknown";
        }

        return parsed.toLocaleString();
    }

    const statusClass = $derived.by(() => {
        if (statusTone === "success") {
            return "border-emerald-300 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/60 dark:text-emerald-300";
        }

        if (statusTone === "warning") {
            return "border-amber-300 bg-amber-50 text-amber-800 dark:border-amber-900 dark:bg-amber-950/60 dark:text-amber-300";
        }

        return "border-blue-300 bg-blue-50 text-blue-800 dark:border-blue-900 dark:bg-blue-950/60 dark:text-blue-300";
    });

    onMount(() => {
        workspaceHistory = readHistoryFromStorage();
        window.dispatchEvent(new CustomEvent("bolt-ui-ready"));
    });
</script>

{#if workspaceRoute.id === "editor" && activeWorkspace}
<div class="h-full min-h-0 overflow-hidden">
    <WorkspaceEditor
        workspaceName={workspaceRoute.workspace.name}
        workspacePath={workspaceRoute.workspace.path}
        initialTools={workspaceRoute.workspace.tools}
        onBack={closeWorkspaceEditor}
    />
</div>
{:else}
<div class="h-full min-h-0 overflow-auto p-5 sm:p-7">
    <div class="mx-auto flex w-full max-w-5xl flex-col gap-6 sm:gap-8">
        <section class="space-y-5 px-1 py-2">
            <h2 class="text-lg font-extrabold tracking-[0.01em] text-zinc-900 dark:text-zinc-100">
                Welcome to your workspace hub
            </h2>

            <p class="max-w-3xl text-sm leading-6 text-zinc-600 dark:text-zinc-300">
                Keep your notes, to-do lists, study links, and planning files in one place. Create
                a new workspace to start fresh, or open one you already use.
            </p>

            <div class="flex flex-wrap items-center gap-3">
                <AppButton variant="primary" size="md" onclick={openCreateModal}>Create New Workspace</AppButton>
                <AppButton variant="secondary" size="md" onclick={openOpenModal}>Open Existing Workspace</AppButton>
            </div>
        </section>

        {#if statusMessage}
            <div class={`rounded-md border px-4 py-3 text-sm ${statusClass}`}>
                {statusMessage}
            </div>
        {/if}

        <section class="space-y-4 px-1 py-1">
            <div class="flex flex-wrap items-center justify-between gap-2">
                <h2 class="text-base font-bold tracking-[0.01em] text-zinc-900 dark:text-zinc-100">
                    Workspace History
                </h2>
                <AppBadge tone="neutral">{workspaceHistory.length} entries</AppBadge>
            </div>

            {#if workspaceHistory.length === 0}
                <div class="rounded-lg px-1 py-5 text-sm text-zinc-600 dark:text-zinc-300">
                    No history yet. Use Create or Open to add your first workspace record.
                </div>
            {:else}
                <div class="divide-y divide-zinc-200/70 dark:divide-zinc-800/80">
                    {#each workspaceHistory as item (item.id)}
                        <div class="px-1 py-5 sm:px-2">
                            <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
                                <div class="flex items-center gap-2">
                                    <p class="text-sm font-semibold text-zinc-900 dark:text-zinc-100">{item.name}</p>
                                    <AppBadge tone={item.source === "created" ? "success" : "info"}>
                                        {item.source}
                                    </AppBadge>
                                </div>
                                <div class="flex items-center gap-2">
                                    <AppButton
                                        variant="ghost"
                                        onclick={() => {
                                            openPath = item.path;
                                            void submitOpenWorkspace();
                                        }}
                                    >
                                        Open
                                    </AppButton>
                                    <AppButton
                                        variant="ghost"
                                        size="sm"
                                        class="text-zinc-500 hover:text-red-600 dark:text-zinc-400 dark:hover:text-red-300"
                                        onclick={() => removeHistoryItem(item.id)}
                                    >
                                        Remove
                                    </AppButton>
                                </div>
                            </div>
                            <p class="mb-2 truncate text-xs text-zinc-600 dark:text-zinc-300">{item.path}</p>
                            <p class="text-[11px] uppercase tracking-[0.04em] text-zinc-500 dark:text-zinc-400">
                                Last opened {formatLastOpened(item.lastOpenedAt)}
                            </p>
                        </div>
                    {/each}
                </div>
            {/if}
        </section>
    </div>
</div>
{/if}

<AppModal open={createModalOpen} title="Create Workspace" onClose={closeCreateModal} class="sm:max-w-xl">
    <form
        class="space-y-5"
        onsubmit={(event) => {
            event.preventDefault();
            void submitCreateWorkspace();
        }}
    >
        <div class="space-y-2">
            <label class="block text-xs font-semibold text-zinc-700 dark:text-zinc-200" for="workspace-name">
                Workspace Name
            </label>
            <input
                id="workspace-name"
                class="w-full rounded-md border border-zinc-300 bg-zinc-50 px-3 py-2.5 text-sm text-zinc-900 outline-none focus:border-teal-600 dark:border-zinc-700 dark:bg-zinc-950 dark:text-zinc-100"
                bind:value={createName}
                placeholder="Focused Study"
            />
        </div>

        <div class="space-y-2">
            <div class="flex items-center justify-between gap-3">
                <p class="text-xs font-semibold text-zinc-700 dark:text-zinc-200">
                    Workspace Folder
                </p>
                <AppButton
                    variant="secondary"
                    size="sm"
                    type="button"
                    onclick={pickCreateFolder}
                    disabled={selectingCreateFolder}
                >
                    {selectingCreateFolder ? "Selecting..." : "Select Folder"}
                </AppButton>
            </div>
            <div class="min-h-11 rounded-md border border-zinc-300 bg-zinc-50 px-3 py-2.5 text-sm text-zinc-700 dark:border-zinc-700 dark:bg-zinc-950 dark:text-zinc-200">
                {createPath || "No folder selected"}
            </div>
        </div>

        <div class="flex items-center justify-end gap-2 border-t border-zinc-200 pt-3 dark:border-zinc-800">
            <AppButton variant="ghost" type="button" onclick={closeCreateModal}>Cancel</AppButton>
            <AppButton variant="primary" type="submit">Create Workspace</AppButton>
        </div>
    </form>
</AppModal>

<AppModal open={openModalOpen} title="Open Workspace" onClose={closeOpenModal} class="sm:max-w-xl">
    <form
        class="space-y-5"
        onsubmit={(event) => {
            event.preventDefault();
            void submitOpenWorkspace();
        }}
    >
        <div class="space-y-2">
            <div class="flex items-center justify-between gap-3">
                <p class="text-xs font-semibold text-zinc-700 dark:text-zinc-200">
                    Workspace Folder
                </p>
                <AppButton
                    variant="secondary"
                    size="sm"
                    type="button"
                    onclick={pickOpenFolder}
                    disabled={selectingOpenFolder}
                >
                    {selectingOpenFolder ? "Selecting..." : "Select Folder"}
                </AppButton>
            </div>
            <div class="min-h-11 rounded-md border border-zinc-300 bg-zinc-50 px-3 py-2.5 text-sm text-zinc-700 dark:border-zinc-700 dark:bg-zinc-950 dark:text-zinc-200">
                {openPath || "No folder selected"}
            </div>
        </div>

        <div class="flex items-center justify-end gap-2 border-t border-zinc-200 pt-3 dark:border-zinc-800">
            <AppButton variant="ghost" type="button" onclick={closeOpenModal}>Cancel</AppButton>
            <AppButton variant="secondary" type="submit">Open Workspace</AppButton>
        </div>
    </form>
</AppModal>
