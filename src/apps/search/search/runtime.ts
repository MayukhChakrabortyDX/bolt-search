import { Channel, invoke } from "@tauri-apps/api/core";
import type {
    FileEntry,
    ScopedQuery,
    SearchRunMode,
    StreamWorkerInput,
    StreamingCompletedEvent,
    StreamingProgressEvent,
    StreamingSearchEvent,
} from "./page-types";

type RuntimeContext = {
    getActiveRunId: () => number;
    getActiveRunMode: () => SearchRunMode;
    getStreamWorker: () => Worker | null;
    streamCompletionResolvers: Map<number, () => void>;
    applyStreamingProgress: (
        payload: StreamingProgressEvent["data"],
        allowEntries: boolean,
    ) => void;
    applyCompleted: (payload: StreamingCompletedEvent["data"]) => void;
    maxResults: number;
};

function postToWorker(
    worker: Worker,
    message: StreamWorkerInput,
): void {
    worker.postMessage(message);
}

function createCompletionPromise(
    context: RuntimeContext,
    runId: number,
): Promise<void> | null {
    const worker = context.getStreamWorker();
    if (!worker) {
        return null;
    }

    return new Promise<void>((resolve) => {
        context.streamCompletionResolvers.set(runId, resolve);
    });
}

function resetWorkerRun(context: RuntimeContext, runId: number): void {
    const worker = context.getStreamWorker();
    if (!worker) {
        return;
    }

    postToWorker(worker, {
        type: "reset",
        runId,
    });
}

function handleProgressMessage(
    context: RuntimeContext,
    runId: number,
    payload: StreamingProgressEvent["data"],
    allowEntries: boolean,
): void {
    const worker = context.getStreamWorker();
    if (worker) {
        postToWorker(worker, {
            type: "progress",
            runId,
            payload,
        });
        return;
    }

    context.applyStreamingProgress(payload, allowEntries);
}

function handleCompletedMessage(
    context: RuntimeContext,
    runId: number,
    payload: StreamingCompletedEvent["data"],
): void {
    const worker = context.getStreamWorker();
    if (worker) {
        postToWorker(worker, {
            type: "completed",
            runId,
            payload,
        });
        return;
    }

    context.applyCompleted(payload);
}

function setupEventChannel(
    context: RuntimeContext,
    runId: number,
    allowEntries: boolean,
): Channel<StreamingSearchEvent> {
    const onEvent = new Channel<StreamingSearchEvent>();

    onEvent.onmessage = (message) => {
        if (runId !== context.getActiveRunId()) {
            return;
        }

        switch (message.event) {
            case "progress": {
                const payload = allowEntries
                    ? message.data
                    : { ...message.data, entries: [] as FileEntry[] };
                handleProgressMessage(context, runId, payload, allowEntries);
                break;
            }

            case "completed": {
                handleCompletedMessage(context, runId, message.data);
                break;
            }
        }
    };

    return onEvent;
}

export async function scanRootsStreaming(
    rootsToScan: string[],
    scopedQuery: ScopedQuery,
    runId: number,
    context: RuntimeContext,
): Promise<void> {
    const completionPromise = createCompletionPromise(context, runId);
    resetWorkerRun(context, runId);
    const onEvent = setupEventChannel(context, runId, true);

    try {
        await invoke("search_streaming", {
            query: scopedQuery,
            roots: rootsToScan,
            limit: context.maxResults,
            runId,
            onEvent,
        });

        if (completionPromise) {
            await completionPromise;
        }
    } catch (error) {
        context.streamCompletionResolvers.delete(runId);
        throw error;
    }
}

export async function scanRootsBatchWithProgress(
    rootsToScan: string[],
    scopedQuery: ScopedQuery,
    runId: number,
    context: RuntimeContext,
): Promise<FileEntry[]> {
    const completionPromise = createCompletionPromise(context, runId);
    resetWorkerRun(context, runId);
    const onEvent = setupEventChannel(context, runId, false);

    try {
        const finalResults = await invoke<FileEntry[]>("search_with_progress", {
            query: scopedQuery,
            roots: rootsToScan,
            limit: context.maxResults,
            runId,
            onEvent,
        });

        if (completionPromise) {
            await completionPromise;
        }

        return finalResults;
    } catch (error) {
        context.streamCompletionResolvers.delete(runId);
        throw error;
    }
}
