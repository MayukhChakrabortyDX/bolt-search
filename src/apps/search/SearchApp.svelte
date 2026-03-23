<script lang="ts">
    import SearchSidebar from "./components/SearchSidebar.svelte";
    import SearchResultsPanel from "./components/SearchResultsPanel.svelte";
    import { POPULAR_EXTENSION_OPTIONS, SIZE_UNIT_OPTIONS } from "./search/page-types";
    import { createSearchController } from "./search/controller.svelte";
    import { analyzeSearchForm, parseExtensionTokens } from "./search/form-utils";
    import { formToFilters } from "./search/form-mapping";
    import {
        buildResultTree,
        findTreeNodeByPath,
        flattenVisibleRows,
    } from "./search/tree-utils";
    import type { DriveScanRow, GroupedResultBucket } from "./search/page-types";

    const controller = createSearchController();
    const state = controller.state;

    const searchFilters = $derived(formToFilters(state.searchForm));
    const selectedExtensionTokens = $derived(
        parseExtensionTokens(state.searchForm.extensionInput),
    );
    const driveOptions = $derived([
        { value: "ALL", label: "Global (all drives)" },
        ...state.availableRoots.map((root) => ({ value: root, label: root })),
    ]);

    const validationIssues = $derived(
        analyzeSearchForm(state.searchForm, {
            enforceFolderScopeSelection: state.enforceFolderScopeValidation,
        }),
    );
    const hasContradiction = $derived(validationIssues.length > 0);

    const activeScanningFolders = $derived(
        state.activeRunMode === "streaming"
            ? Object.keys(state.scanningFolders).filter((path) => state.scanningFolders[path])
            : state.intentEnabled
              ? Object.keys(state.intentLoadingFolders).filter(
                    (path) => state.intentLoadingFolders[path],
                )
            : [],
    );
    const intentDiscoveredFolders = $derived(
        state.intentEnabled
            ? Object.keys(state.intentKnownFolders).filter(
                  (path) => state.intentKnownFolders[path],
              )
            : [],
    );
    const resultTree = $derived(
        buildResultTree(state.results, activeScanningFolders, intentDiscoveredFolders),
    );
    const treeRows = $derived(
        flattenVisibleRows(resultTree, state.openDirectories),
    );
    const focusTargetNode = $derived.by(() => {
        if (state.layoutMode !== "focus") {
            return null;
        }

        if (state.focusedFolderPath) {
            const node = findTreeNodeByPath(resultTree, state.focusedFolderPath);
            if (node?.isDir) {
                return node;
            }
        }

        return null;
    });
    const focusEntries = $derived.by(() => {
        if (state.layoutMode !== "focus") {
            return [];
        }

        if (focusTargetNode) {
            return focusTargetNode.children;
        }

        return resultTree;
    });

    function parentGroupLabel(path: string): string {
        const normalized = path.replace(/\\/g, "/").replace(/\/+/g, "/");
        const parts = normalized.split("/").filter(Boolean);
        if (parts.length <= 1) {
            return "/";
        }
        return parts.slice(0, -1).join("/");
    }

    const groupedBuckets = $derived.by(() => {
        const buckets: GroupedResultBucket[] = [];
        const byKey = new Map<string, GroupedResultBucket>();

        for (const entry of state.results) {
            const key = parentGroupLabel(entry.path);
            let bucket = byKey.get(key);
            if (!bucket) {
                bucket = {
                    key,
                    label: key,
                    entries: [],
                };
                byKey.set(key, bucket);
                buckets.push(bucket);
            }
            bucket.entries.push(entry);
        }

        return buckets;
    });

    const driveScanTotal = $derived(
        Object.values(state.displayedDriveScanCounts).reduce(
            (sum, value) => sum + value,
            0,
        ),
    );
    const driveScanRows = $derived.by(() => {
        const labels = [...state.driveScanOrder.slice(0, 4)];
        while (labels.length < 4) {
            labels.push("");
        }

        return labels.map((label, index): DriveScanRow => {
            const active = label.length > 0;
            const scanned = active ? (state.displayedDriveScanCounts[label] ?? 0) : 0;

            return {
                label: active ? label : `Drive ${index + 1}`,
                scanned,
                active,
            };
        });
    });

    const searchDurationLabel = $derived.by(() => {
        if (state.searching) {
            return formatDuration(state.searchElapsedMs);
        }
        if (state.lastSearchDurationMs !== null) {
            return formatDuration(state.lastSearchDurationMs);
        }
        return "";
    });

    function formatDuration(ms: number): string {
        if (ms < 1000) {
            return `${ms} ms`;
        }

        const totalSeconds = ms / 1000;
        if (totalSeconds < 60) {
            return `${totalSeconds.toFixed(1)} s`;
        }

        const minutes = Math.floor(totalSeconds / 60);
        const seconds = (totalSeconds % 60).toFixed(1).padStart(4, "0");
        return `${minutes}m ${seconds}s`;
    }
</script>

<div class="w-full h-full flex flex-row bg-white dark:bg-zinc-900">
    <SearchSidebar
        searching={state.searching}
        searched={state.searched}
        resultsLength={state.results.length}
        {hasContradiction}
        {validationIssues}
        searchForm={state.searchForm}
        showAdvanced={state.showAdvanced}
        {selectedExtensionTokens}
        {driveOptions}
        popularExtensionOptions={POPULAR_EXTENSION_OPTIONS}
        sizeUnitOptions={SIZE_UNIT_OPTIONS}
        onSearch={controller.search}
        onStop={controller.stopSearch}
        onClearResults={controller.clearSearchResults}
        onResetForm={controller.resetSearchForm}
        onSetShowAdvanced={(next) => {
            state.showAdvanced = next;
        }}
        onEnsureDriveScopeSelection={controller.ensureDriveScopeSelection}
        onPickScopeFolders={controller.pickScopeFolders}
        onRemoveScopeFolder={controller.removeScopeFolder}
        onTogglePopularExtension={controller.togglePopularExtension}
        onNormalizeExtensionInput={controller.normalizeExtensionInput}
        onRemoveExtensionToken={controller.removeExtensionToken}
        displayPath={controller.displayPath}
    />

    <SearchResultsPanel
        layoutMode={state.layoutMode}
        searching={state.searching}
        searched={state.searched}
        searchStatus={state.searchStatus}
        resultsLength={state.results.length}
        {driveScanTotal}
        {searchDurationLabel}
        {driveScanRows}
        {treeRows}
        rowIndentClass={controller.rowIndentClass}
        displayPath={controller.displayPath}
        isFolderScanning={controller.isFolderScanning}
        isFolderEmpty={controller.isFolderEmpty}
        focusedFolderPath={focusTargetNode ? focusTargetNode.path : null}
        focusEntries={focusEntries}
        groupedBuckets={groupedBuckets}
        toggleDirectory={controller.toggleDirectory}
        focusFolder={controller.focusFolder}
        openInExplorer={controller.openInExplorer}
    />
</div>
