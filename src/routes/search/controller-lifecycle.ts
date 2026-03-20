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
        }
    };

    window.addEventListener("bolt-save-filter", onSave);
    window.addEventListener("bolt-load-filter", onLoad);
    window.addEventListener("bolt-streaming-mode-changed", onStreamingModeChange);

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
        runtime.streamWorkerRef.current?.terminate();
        runtime.streamWorkerRef.current = null;
        runtime.streamCompletionResolvers.clear();
    };
}
