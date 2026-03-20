use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static THREAD_POOL_CACHE: OnceLock<Mutex<Vec<(usize, Arc<rayon::ThreadPool>)>>> =
    OnceLock::new();

pub(crate) fn get_thread_pool(workers: usize) -> Result<Arc<rayon::ThreadPool>, String> {
    let cache = THREAD_POOL_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = cache
        .lock()
        .map_err(|_| "Thread pool cache lock poisoned".to_string())?;

    if let Some((_, pool)) = guard.iter().find(|(w, _)| *w == workers) {
        return Ok(pool.clone());
    }

    let pool = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|e| e.to_string())?,
    );

    guard.push((workers, pool.clone()));
    Ok(pool)
}

pub(crate) fn claim_result_budget(remaining: &AtomicUsize, max_budget: usize) -> usize {
    let mut current = remaining.load(Ordering::Acquire);

    loop {
        if current == 0 {
            return 0;
        }

        let grant = current.min(max_budget.max(1));

        match remaining.compare_exchange_weak(
            current,
            current - grant,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return grant,
            Err(next) => current = next,
        }
    }
}
