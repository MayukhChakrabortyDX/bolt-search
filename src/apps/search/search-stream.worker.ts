type FileEntry = {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: string;
};

type StreamingProgressPayload = {
  startedFolders: string[];
  finishedFolders: string[];
  entries: FileEntry[];
  scannedFolders: number;
  totalResults: number;
};

type StreamingCompletedPayload = {
  scannedFolders: number;
  totalResults: number;
  truncated: boolean;
};

type StreamWorkerInput =
  | { type: "configure"; payload: { debounceMs: number } }
  | { type: "reset"; runId: number }
  | { type: "flush"; runId: number }
  | { type: "progress"; runId: number; payload: StreamingProgressPayload }
  | { type: "completed"; runId: number; payload: StreamingCompletedPayload };

type StreamWorkerOutput =
  | { type: "batched-progress"; runId: number; payload: StreamingProgressPayload }
  | { type: "completed"; runId: number; payload: StreamingCompletedPayload };

const MIN_DEBOUNCE_MS = 50;
const MAX_DEBOUNCE_MS = 2_000;

let debounceMs = 200;
let flushTimer: ReturnType<typeof setTimeout> | null = null;

let pendingStartedFolders = new Set<string>();
let pendingFinishedFolders = new Set<string>();
let pendingEntries: FileEntry[] = [];
let latestScannedFolders = 0;
let latestTotalResults = 0;
let activeRunId = 0;

function resetBuffers() {
  pendingStartedFolders = new Set<string>();
  pendingFinishedFolders = new Set<string>();
  pendingEntries = [];
  latestScannedFolders = 0;
  latestTotalResults = 0;
}

function clearTimer() {
  if (!flushTimer) return;
  clearTimeout(flushTimer);
  flushTimer = null;
}

function flushProgress(runId: number) {
  if (
    pendingStartedFolders.size === 0 &&
    pendingFinishedFolders.size === 0 &&
    pendingEntries.length === 0
  ) {
    return;
  }

  const message: StreamWorkerOutput = {
    type: "batched-progress",
    runId,
    payload: {
      startedFolders: [...pendingStartedFolders],
      finishedFolders: [...pendingFinishedFolders],
      entries: pendingEntries,
      scannedFolders: latestScannedFolders,
      totalResults: latestTotalResults,
    },
  };

  postMessage(message);
  resetBuffers();
}

function scheduleFlush() {
  if (flushTimer) return;
  flushTimer = setTimeout(() => {
    flushTimer = null;
    flushProgress(activeRunId);
  }, debounceMs);
}

self.onmessage = (event: MessageEvent<StreamWorkerInput>) => {
  const message = event.data;

  switch (message.type) {
    case "configure": {
      const nextDebounce = Number(message.payload.debounceMs);
      if (Number.isFinite(nextDebounce)) {
        debounceMs = Math.max(MIN_DEBOUNCE_MS, Math.min(MAX_DEBOUNCE_MS, Math.round(nextDebounce)));
      }
      break;
    }

    case "reset": {
      activeRunId = message.runId;
      clearTimer();
      resetBuffers();
      break;
    }

    case "flush": {
      if (message.runId !== activeRunId) {
        break;
      }

      clearTimer();
      flushProgress(activeRunId);
      break;
    }

    case "progress": {
      if (message.runId !== activeRunId) {
        break;
      }

      for (const folder of message.payload.startedFolders) {
        if (folder) {
          pendingStartedFolders.add(folder);
          pendingFinishedFolders.delete(folder);
        }
      }

      for (const folder of message.payload.finishedFolders) {
        if (folder) {
          pendingFinishedFolders.add(folder);
          pendingStartedFolders.delete(folder);
        }
      }

      if (message.payload.entries.length > 0) {
        pendingEntries.push(...message.payload.entries);
      }

      latestScannedFolders = message.payload.scannedFolders;
      latestTotalResults = message.payload.totalResults;
      scheduleFlush();
      break;
    }

    case "completed": {
      if (message.runId !== activeRunId) {
        break;
      }

      clearTimer();
      flushProgress(activeRunId);
      postMessage({ type: "completed", runId: activeRunId, payload: message.payload } as StreamWorkerOutput);
      break;
    }
  }
};
