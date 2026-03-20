use std::sync::atomic::{AtomicU64, Ordering};

const MIN_WORKERS: usize = 2;
const MAX_WORKERS: usize = 16;
const MIN_BATCH_SIZE: usize = 8;
const MAX_BATCH_SIZE: usize = 48;

static ACTIVE_SEARCH_RUN_ID: AtomicU64 = AtomicU64::new(0);
static CANCEL_REQUEST_RUN_ID: AtomicU64 = AtomicU64::new(0);
static INTERNAL_RUN_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(crate) fn resolve_run_id(run_id: Option<u64>) -> u64 {
    run_id.unwrap_or_else(|| INTERNAL_RUN_ID_COUNTER.fetch_add(1, Ordering::AcqRel))
}

pub(crate) fn mark_run_started(run_id: u64) {
    CANCEL_REQUEST_RUN_ID.store(0, Ordering::Release);
    ACTIVE_SEARCH_RUN_ID.store(run_id, Ordering::Release);
}

pub(crate) fn mark_run_finished(run_id: u64) {
    let _ = ACTIVE_SEARCH_RUN_ID.compare_exchange(run_id, 0, Ordering::AcqRel, Ordering::Acquire);

    if CANCEL_REQUEST_RUN_ID.load(Ordering::Acquire) == run_id {
        CANCEL_REQUEST_RUN_ID.store(0, Ordering::Release);
    }
}

pub(crate) fn is_run_cancelled(run_id: u64) -> bool {
    CANCEL_REQUEST_RUN_ID.load(Ordering::Acquire) == run_id
        || ACTIVE_SEARCH_RUN_ID.load(Ordering::Acquire) != run_id
}

pub(crate) fn scheduler_tuning(root_count: usize) -> (usize, usize) {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(6);

    let workers = cores.clamp(MIN_WORKERS, MAX_WORKERS);

    let root_factor = root_count.clamp(1, 8);
    let batch_size = (workers * root_factor).clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE);

    (workers, batch_size)
}

pub(crate) fn active_run_id() -> u64 {
    ACTIVE_SEARCH_RUN_ID.load(Ordering::Acquire)
}

pub(crate) fn request_cancel(run_id: u64) {
    CANCEL_REQUEST_RUN_ID.store(run_id, Ordering::Release);
}
