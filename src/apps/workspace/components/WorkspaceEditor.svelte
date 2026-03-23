<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { ChevronDown, ChevronRight } from "lucide-svelte";
    import AppButton from "../../../lib/components/ui/AppButton.svelte";
    import { WORKSPACE_BOLT_FILE_NAME, WorkspaceManifest } from "../workspace-schema";
    import {
        BOLT_TOOLS_INDEX_FILE,
        findWorkspaceToolModule,
        isBoltInternalPath,
        listMissingToolKinds,
        toolStorageFilePath,
        type WorkspaceToolJson,
        type WorkspaceToolKind,
    } from "../workspace-tools";

    type WorkspaceEntry = {
        name: string;
        relative_path: string;
        is_dir: boolean;
        size: number;
        modified: string;
    };

    let {
        workspaceName,
        workspacePath,
        initialTools,
        onBack,
    }: {
        workspaceName: string;
        workspacePath: string;
        initialTools: WorkspaceToolJson[];
        onBack?: () => void;
    } = $props();

    let entries = $state<WorkspaceEntry[]>([]);
    let loading = $state(false);
    let busy = $state(false);
    let errorMessage = $state<string | null>(null);

    let selectedPath = $state<string | null>(null);
    let selectedIsDir = $state(false);
    let fileContent = $state("");
    let originalContent = $state("");
    let sidebarCollapsed = $state(false);
    let installedTools = $state<WorkspaceToolJson[]>([]);
    let expandedToolIds = $state<Record<string, boolean>>({});

    const missingToolKinds = $derived.by(() => listMissingToolKinds(installedTools));

    function pathDepth(relativePath: string): number {
        const normalized = relativePath.trim().replace(/\\/g, "/");
        if (!normalized) {
            return 0;
        }

        return Math.max(0, normalized.split("/").filter(Boolean).length - 1);
    }

    function displayName(relativePath: string): string {
        const normalized = relativePath.trim().replace(/\\/g, "/");
        const chunks = normalized.split("/").filter(Boolean);
        return chunks[chunks.length - 1] ?? relativePath;
    }

    function selectedIsDirty(): boolean {
        return selectedPath !== null && !selectedIsDir && fileContent !== originalContent;
    }

    function absolutePathFromRelative(relativePath: string): string {
        const root = workspacePath.replace(/[\\/]+$/, "");
        const relative = relativePath.replace(/^\/+/, "").replace(/\//g, "\\");
        return `${root}\\${relative}`;
    }

    async function openFileByPath(relativePath: string) {
        selectedPath = relativePath;
        selectedIsDir = false;
        errorMessage = null;

        try {
            busy = true;
            const content = await invoke<string>("read_workspace_file", {
                root: workspacePath,
                relativePath,
            });
            fileContent = content;
            originalContent = content;
        } catch (error) {
            errorMessage = error instanceof Error ? error.message : "Failed to read file.";
        } finally {
            busy = false;
        }
    }

    async function refreshEntries() {
        loading = true;
        errorMessage = null;

        try {
            const result = await invoke<WorkspaceEntry[]>("list_workspace_entries", {
                root: workspacePath,
            });
            entries = result.filter((entry) => !isBoltInternalPath(entry.relative_path));

            if (selectedPath) {
                const stillExists = result.some((entry) => entry.relative_path === selectedPath);
                if (!stillExists) {
                    selectedPath = null;
                    selectedIsDir = false;
                    fileContent = "";
                    originalContent = "";
                }
            }
        } catch (error) {
            errorMessage = error instanceof Error ? error.message : "Failed to load workspace files.";
        } finally {
            loading = false;
        }
    }

    async function openEntry(entry: WorkspaceEntry) {
        selectedPath = entry.relative_path;
        selectedIsDir = entry.is_dir;
        errorMessage = null;

        if (entry.is_dir) {
            fileContent = "";
            originalContent = "";
            return;
        }

        await openFileByPath(entry.relative_path);
    }

    async function saveCurrentFile() {
        if (!selectedPath || selectedIsDir) return;

        try {
            busy = true;
            await invoke("write_workspace_file", {
                root: workspacePath,
                relativePath: selectedPath,
                content: fileContent,
            });
            originalContent = fileContent;
            await refreshEntries();
        } catch (error) {
            errorMessage = error instanceof Error ? error.message : "Failed to save file.";
        } finally {
            busy = false;
        }
    }

    async function loadInstalledTools() {
        try {
            const indexRaw = await invoke<string>("read_workspace_file", {
                root: workspacePath,
                relativePath: BOLT_TOOLS_INDEX_FILE,
            });
            const parsed = JSON.parse(indexRaw) as unknown;
            if (!Array.isArray(parsed)) {
                installedTools = [];
                return;
            }

            installedTools = parsed
                .filter((entry): entry is WorkspaceToolJson => {
                    if (typeof entry !== "object" || entry === null) return false;
                    const candidate = entry as Record<string, unknown>;
                    return (
                        typeof candidate.id === "string" &&
                        typeof candidate.kind === "string" &&
                        typeof candidate.name === "string" &&
                        typeof candidate.createdAt === "string" &&
                        typeof candidate.config === "object" &&
                        candidate.config !== null
                    );
                })
                .map((entry) => ({
                    ...entry,
                    id: entry.id.trim(),
                    name: entry.name.trim(),
                    createdAt: new Date(entry.createdAt).toISOString(),
                }))
                .filter((entry) => entry.id.length > 0 && entry.name.length > 0);
        } catch {
            // Fallback to manifest for older workspaces that do not yet have a .bolt index.
            try {
                const manifestRaw = await invoke<string>("read_workspace_file", {
                    root: workspacePath,
                    relativePath: WORKSPACE_BOLT_FILE_NAME,
                });
                const manifest = WorkspaceManifest.parse(manifestRaw);
                installedTools = manifest.tools;
            } catch {
                installedTools = [];
            }
        }
    }

    async function saveToolsToManifest(tools: WorkspaceToolJson[]) {
        const manifestRaw = await invoke<string>("read_workspace_file", {
            root: workspacePath,
            relativePath: WORKSPACE_BOLT_FILE_NAME,
        });
        const manifest = WorkspaceManifest.parse(manifestRaw);
        const nextManifest = new WorkspaceManifest({
            name: manifest.name,
            createdAt: manifest.createdAt,
            updatedAt: new Date(),
            tools,
        });

        await invoke("write_workspace_file", {
            root: workspacePath,
            relativePath: WORKSPACE_BOLT_FILE_NAME,
            content: JSON.stringify(nextManifest.toJSON(), null, 2),
        });

        await invoke("write_workspace_file", {
            root: workspacePath,
            relativePath: BOLT_TOOLS_INDEX_FILE,
            content: JSON.stringify(tools, null, 2),
        });
    }

    function toolLabel(kind: WorkspaceToolKind): string {
        return findWorkspaceToolModule(kind).title;
    }

    function toolDescription(kind: WorkspaceToolKind): string {
        return findWorkspaceToolModule(kind).description;
    }

    function toolStoragePath(tool: WorkspaceToolJson): string {
        return toolStorageFilePath(tool);
    }

    function isToolExpanded(toolId: string): boolean {
        return expandedToolIds[toolId] === true;
    }

    function toggleToolExpanded(toolId: string) {
        expandedToolIds = {
            ...expandedToolIds,
            [toolId]: !isToolExpanded(toolId),
        };
    }

    async function openToolDataFile(tool: WorkspaceToolJson) {
        await openFileByPath(toolStoragePath(tool));
    }

    async function revealToolInExplorer(tool: WorkspaceToolJson) {
        try {
            await invoke("open_in_explorer", {
                path: absolutePathFromRelative(toolStoragePath(tool)),
            });
        } catch (error) {
            errorMessage = error instanceof Error ? error.message : "Failed to open in explorer.";
        }
    }

    async function removeTool(tool: WorkspaceToolJson) {
        try {
            busy = true;
            errorMessage = null;
            await invoke("delete_workspace_entry", {
                root: workspacePath,
                relativePath: toolStoragePath(tool),
            });

            const nextTools = installedTools.filter((entry) => entry.id !== tool.id);
            await saveToolsToManifest(nextTools);
            installedTools = nextTools;

            if (isToolExpanded(tool.id)) {
                const nextExpanded = { ...expandedToolIds };
                delete nextExpanded[tool.id];
                expandedToolIds = nextExpanded;
            }
        } catch (error) {
            errorMessage = error instanceof Error ? error.message : "Failed to remove tool module.";
        } finally {
            busy = false;
        }
    }

    async function installTool(kind: WorkspaceToolKind) {
        if (installedTools.some((tool) => tool.kind === kind)) {
            return;
        }

        try {
            busy = true;
            errorMessage = null;
            const module = findWorkspaceToolModule(kind);
            const toolEntity = module.createEntity();
            const nextTools = [...installedTools, toolEntity];

            await invoke("write_workspace_file", {
                root: workspacePath,
                relativePath: toolStorageFilePath(toolEntity),
                content: JSON.stringify(toolEntity, null, 2),
            });

            await saveToolsToManifest(nextTools);
            installedTools = nextTools;
        } catch (error) {
            errorMessage = error instanceof Error ? error.message : "Failed to install tool module.";
        } finally {
            busy = false;
        }
    }

    async function deleteSelected() {
        if (!selectedPath) return;

        try {
            busy = true;
            await invoke("delete_workspace_entry", {
                root: workspacePath,
                relativePath: selectedPath,
            });
            selectedPath = null;
            selectedIsDir = false;
            fileContent = "";
            originalContent = "";
            await refreshEntries();
        } catch (error) {
            errorMessage = error instanceof Error ? error.message : "Failed to delete entry.";
        } finally {
            busy = false;
        }
    }

    $effect(() => {
        workspacePath;
        installedTools = [...(initialTools ?? [])];
        void refreshEntries();
        void loadInstalledTools();
    });
</script>

<div class="w-full h-full min-h-0 flex flex-row overflow-hidden bg-white dark:bg-zinc-900">
    <aside class={`workspace-sidebar h-full bg-zinc-50 border-r border-zinc-200/70 dark:bg-zinc-950 dark:border-zinc-800/60 flex flex-col transition-[width,min-width] duration-200 ${sidebarCollapsed ? "w-[54px] min-w-[54px]" : "w-[300px] min-w-[270px]"}`}>
            <div class="px-3 pt-3 pb-2">
                <div class="flex items-center justify-between gap-2">
                    {#if !sidebarCollapsed}
                        <p class="text-[11px] font-bold uppercase tracking-[0.06em] text-zinc-500 dark:text-zinc-400">Explorer</p>
                    {/if}
                    <AppButton
                        size="sm"
                        variant="ghost"
                        class="px-2 text-zinc-500 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-100"
                        onclick={() => (sidebarCollapsed = !sidebarCollapsed)}
                    >
                        {sidebarCollapsed ? ">" : "<"}
                    </AppButton>
                </div>
            </div>

            {#if !sidebarCollapsed}
            <div class="px-3 pb-2">
                <p class="mb-1 text-[10px] font-semibold uppercase tracking-[0.06em] text-zinc-500 dark:text-zinc-400">Files</p>
                <div class="min-h-0 max-h-[52vh] overflow-auto rounded-md border border-zinc-200/80 bg-white/80 p-1.5 dark:border-zinc-800/70 dark:bg-zinc-900/50">
                    {#if loading}
                        <p class="px-2 py-2 text-xs text-zinc-500 dark:text-zinc-400">Loading files...</p>
                    {:else if entries.length === 0}
                        <p class="px-2 py-2 text-xs text-zinc-500 dark:text-zinc-400">No files yet.</p>
                    {:else}
                        <div class="space-y-0.5">
                            {#each entries as entry (entry.relative_path)}
                                <button
                                    type="button"
                                    class={`flex w-full items-center justify-between rounded-md px-2 py-1.5 text-left text-xs transition ${selectedPath === entry.relative_path ? "bg-teal-100 text-teal-900 dark:bg-teal-950/70 dark:text-teal-200" : "text-zinc-700 hover:bg-zinc-100 dark:text-zinc-200 dark:hover:bg-zinc-900"}`}
                                    onclick={() => void openEntry(entry)}
                                    style={`padding-left: ${0.5 + pathDepth(entry.relative_path) * 0.85}rem;`}
                                >
                                    <span class="truncate">{entry.is_dir ? "📁" : "📄"} {displayName(entry.relative_path)}</span>
                                </button>
                            {/each}
                        </div>
                    {/if}
                </div>
            </div>

            <div class="border-t border-zinc-200/80 px-3 pt-3 pb-3 dark:border-zinc-800/80">
                <p class="mb-2 text-[10px] font-semibold uppercase tracking-[0.06em] text-zinc-500 dark:text-zinc-400">Tools</p>
                <div class="space-y-2">
                    {#if installedTools.length === 0}
                        <p class="rounded-md border border-zinc-200/80 bg-zinc-100/70 px-2 py-2 text-[11px] text-zinc-600 dark:border-zinc-800/70 dark:bg-zinc-900/60 dark:text-zinc-300">
                            No tool modules installed yet.
                        </p>
                    {:else}
                        <div class="space-y-1.5">
                            {#each installedTools as tool (tool.id)}
                                <div class="rounded-md border border-zinc-200/80 bg-zinc-100/70 dark:border-zinc-800/70 dark:bg-zinc-900/60 overflow-hidden">
                                    <button
                                        type="button"
                                        class="w-full flex items-center justify-between gap-2 px-2 py-1.5 text-left hover:bg-zinc-200/70 dark:hover:bg-zinc-800/70"
                                        onclick={() => toggleToolExpanded(tool.id)}
                                    >
                                        <div class="min-w-0">
                                            <p class="truncate text-xs font-semibold text-zinc-900 dark:text-zinc-100">{tool.name}</p>
                                            <p class="truncate text-[10px] uppercase tracking-[0.05em] text-zinc-500 dark:text-zinc-400">{tool.kind}</p>
                                        </div>
                                        <span class="text-zinc-500 dark:text-zinc-400 shrink-0">
                                            {#if isToolExpanded(tool.id)}
                                                <ChevronDown size={12} strokeWidth={2} />
                                            {:else}
                                                <ChevronRight size={12} strokeWidth={2} />
                                            {/if}
                                        </span>
                                    </button>

                                    {#if isToolExpanded(tool.id)}
                                        <div class="border-t border-zinc-200/80 dark:border-zinc-800/80 px-2 py-2 space-y-2">
                                            <p class="text-[10px] text-zinc-500 dark:text-zinc-400 truncate">
                                                {toolStoragePath(tool)}
                                            </p>
                                            <div class="grid grid-cols-2 gap-1.5">
                                                <AppButton size="sm" variant="ghost" disabled={busy} onclick={() => void openToolDataFile(tool)}>
                                                    Open Data
                                                </AppButton>
                                                <AppButton size="sm" variant="ghost" disabled={busy} onclick={() => void revealToolInExplorer(tool)}>
                                                    Explore
                                                </AppButton>
                                            </div>
                                            <AppButton
                                                size="sm"
                                                variant="secondary"
                                                class="w-full bg-rose-600 text-white hover:bg-rose-700 dark:bg-rose-700 dark:hover:bg-rose-600"
                                                disabled={busy}
                                                onclick={() => void removeTool(tool)}
                                            >
                                                Remove Module
                                            </AppButton>
                                        </div>
                                    {/if}
                                </div>
                            {/each}
                        </div>
                    {/if}

                    <div class="pt-1">
                        <p class="mb-1 text-[10px] font-semibold uppercase tracking-[0.06em] text-zinc-500 dark:text-zinc-400">Add Module</p>
                        {#if missingToolKinds.length === 0}
                            <p class="text-[11px] text-zinc-500 dark:text-zinc-400">All core modules are already added.</p>
                        {:else}
                            <div class="space-y-2">
                                {#each missingToolKinds as kind (kind)}
                                    <button
                                        type="button"
                                        class="w-full rounded-md border border-zinc-300/80 bg-white px-2 py-1.5 text-left text-xs transition hover:border-teal-500 hover:bg-teal-50 dark:border-zinc-700/80 dark:bg-zinc-900 dark:hover:border-teal-700 dark:hover:bg-teal-950/40"
                                        disabled={busy}
                                        onclick={() => void installTool(kind)}
                                    >
                                        <p class="font-semibold text-zinc-900 dark:text-zinc-100">{toolLabel(kind)}</p>
                                        <p class="text-[10px] text-zinc-500 dark:text-zinc-400">{toolDescription(kind)}</p>
                                    </button>
                                {/each}
                            </div>
                        {/if}
                    </div>
                </div>
            </div>
            {:else}
                <div class="px-2 pb-3 flex flex-col gap-2">
                    <AppButton size="sm" variant="ghost" title="Refresh" disabled={busy} onclick={() => void refreshEntries()}>
                        R
                    </AppButton>
                    <AppButton
                        size="sm"
                        variant="ghost"
                        title="Add First Module"
                        disabled={busy || missingToolKinds.length === 0}
                        onclick={() => {
                            const first = missingToolKinds[0];
                            if (first) {
                                void installTool(first);
                            }
                        }}
                    >
                        +
                    </AppButton>
                </div>
            {/if}
    </aside>

    <main class="h-full flex-1 min-w-0 flex flex-col">
            <div class="border-b border-zinc-200/70 dark:border-zinc-800/70 px-3 py-2 flex flex-wrap items-center justify-between gap-2">
                <div class="min-w-0">
                    <p class="truncate text-sm font-semibold text-zinc-900 dark:text-zinc-100">
                        {workspaceName}
                    </p>
                    <p class="truncate text-xs text-zinc-500 dark:text-zinc-400">{selectedPath ?? "No file selected"}</p>
                </div>
                <div class="flex items-center gap-2">
                    <AppButton size="sm" variant="secondary" onclick={() => onBack?.()}>
                        Back
                    </AppButton>
                    <AppButton
                        size="sm"
                        variant="secondary"
                        disabled={!selectedPath || busy}
                        class="bg-rose-600 text-white hover:bg-rose-700 dark:bg-rose-700 dark:hover:bg-rose-600"
                        onclick={() => void deleteSelected()}
                    >
                        Delete
                    </AppButton>
                    <AppButton
                        size="sm"
                        variant="primary"
                        disabled={!selectedPath || selectedIsDir || busy || !selectedIsDirty()}
                        onclick={() => void saveCurrentFile()}
                    >
                        Save
                    </AppButton>
                </div>
            </div>

            <div class="min-h-0 flex-1 overflow-auto bg-zinc-50/35 dark:bg-zinc-950/45">
            {#if errorMessage}
                <div class="m-3 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/60 dark:text-amber-300">
                    {errorMessage}
                </div>
            {/if}

            {#if !selectedPath}
                <div class="h-full px-5 py-8 text-sm text-zinc-600 dark:text-zinc-300 flex items-center">
                    Select a file from the left to start editing.
                </div>
            {:else if selectedIsDir}
                <div class="h-full px-5 py-8 text-sm text-zinc-600 dark:text-zinc-300 flex items-center">
                    This is a folder. Select a file to open it in the editor.
                </div>
            {:else}
                <textarea
                    class="h-full min-h-[360px] w-full resize-none border-0 bg-transparent p-4 font-mono text-sm text-zinc-900 outline-none dark:text-zinc-100"
                    bind:value={fileContent}
                    spellcheck={false}
                ></textarea>
            {/if}
            </div>
        </main>
</div>
