import { invoke } from "@tauri-apps/api/core";
import {
    WORKER_UI_DEBOUNCE_MS,
    type StreamWorkerInput,
    type StreamWorkerOutput,
} from "./page-types";
import { resolvePreferredDrive } from "./form-utils";
import type { SearchControllerState, SearchRuntimeDeps } from "./controller-types";
import { applyCompleted, applyStreamingProgress } from "./controller-runtime";

type LifecycleDeps = {
    state: SearchControllerState;
    runtime: SearchRuntimeDeps;
    saveFilter: () => Promise<void>;
    loadFilter: () => Promise<void>;
};

export function attachControllerLifecycle({
    state,
    runtime,
    saveFilter,
    loadFilter,
}: LifecycleDeps): () => void {
    const onSave = () => {
        void saveFilter();
    };
    const onLoad = () => {
        void loadFilter();
    };
    const onStreamingModeChange = (event: Event) => {
        const customEvent = event as CustomEvent<{ enabled?: unknown }>;
        if (typeof customEvent.detail?.enabled === "boolean") {
            state.streamingEnabled = customEvent.detail.enabled;
            if (state.streamingEnabled) {
                state.intentEnabled = false;
            }
        }
    };
    const onIntentModeChange = (event: Event) => {
        const customEvent = event as CustomEvent<{ enabled?: unknown }>;
        if (typeof customEvent.detail?.enabled === "boolean") {
            if (state.layoutMode === "group" && customEvent.detail.enabled) {
                state.intentEnabled = false;
                return;
            }
            state.intentEnabled = customEvent.detail.enabled;
            if (state.intentEnabled) {
                state.streamingEnabled = false;
            }
        }
    };
    const onLayoutModeChange = (event: Event) => {
        const customEvent = event as CustomEvent<{ mode?: unknown }>;
        const mode = customEvent.detail?.mode;
        state.layoutMode =
            mode === "focus" || mode === "group" ? mode : "default";
        if (state.layoutMode === "group") {
            state.intentEnabled = false;
        }
    };

    window.addEventListener("bolt-save-filter", onSave);
    window.addEventListener("bolt-load-filter", onLoad);
    window.addEventListener("bolt-streaming-mode-changed", onStreamingModeChange);
    window.addEventListener("bolt-intent-mode-changed", onIntentModeChange);
    window.addEventListener("bolt-layout-mode-changed", onLayoutModeChange);

    runtime.streamWorkerRef.current = new Worker(
        new URL("../search-stream.worker.ts", import.meta.url),
        {
            type: "module",
        },
    );

    runtime.streamWorkerRef.current.postMessage({
        type: "configure",
        payload: { debounceMs: WORKER_UI_DEBOUNCE_MS },
    } as StreamWorkerInput);

    runtime.streamWorkerRef.current.onmessage = (
        event: MessageEvent<StreamWorkerOutput>,
    ) => {
        if (event.data.runId !== state.activeRunId) {
            return;
        }

        if (event.data.type === "batched-progress") {
            const allowEntries = state.activeRunMode === "streaming";
            applyStreamingProgress(state, runtime, event.data.payload, allowEntries);
            return;
        }

        applyCompleted(state, runtime, event.data.runId, event.data.payload);
    };

    const storedStreaming = localStorage.getItem("bolt-search-streaming-enabled");
    state.streamingEnabled = !(storedStreaming === "0" || storedStreaming === "false");
    const storedIntent = localStorage.getItem("bolt-search-intent-enabled");
    state.intentEnabled = storedIntent === "1" || storedIntent === "true";
    if (state.intentEnabled) {
        state.streamingEnabled = false;
    }
    const storedLayoutMode = localStorage.getItem("bolt-search-layout-mode");
    state.layoutMode =
        storedLayoutMode === "focus" || storedLayoutMode === "group"
            ? storedLayoutMode
            : "default";
    if (state.layoutMode === "group") {
        state.intentEnabled = false;
    }

    void (async () => {
        try {
            state.availableRoots = await invoke<string[]>("list_search_roots");
            if (state.searchForm.scopeMode === "drive") {
                const current = state.searchForm.scopeDrive.trim().toUpperCase();
                if (!current || current === "ALL") {
                    state.searchForm.scopeDrive = resolvePreferredDrive(
                        state.availableRoots,
                    );
                }
            }
        } catch {
            state.availableRoots = [];
        } finally {
            window.dispatchEvent(new CustomEvent("bolt-ui-ready"));
        }
    })();

    return () => {
        window.removeEventListener("bolt-save-filter", onSave);
        window.removeEventListener("bolt-load-filter", onLoad);
        window.removeEventListener(
            "bolt-streaming-mode-changed",
            onStreamingModeChange,
        );
        window.removeEventListener("bolt-intent-mode-changed", onIntentModeChange);
        window.removeEventListener("bolt-layout-mode-changed", onLayoutModeChange);
        runtime.streamWorkerRef.current?.terminate();
        runtime.streamWorkerRef.current = null;
        runtime.streamCompletionResolvers.clear();
    };
}
