//! The worker pool used while a document is opened.
//!
//! Capturing a page shares nothing with other pages (every page gets its own interpreter state),
//! so the pages of a document are spread over the available cores. Results keep page order.

use std::sync::OnceLock;

use rayon::ThreadPool;
use rayon::prelude::*;

/// Stack of a worker. Capturing recurses through nested Form `XObjects` (the engine follows up
/// to 50 levels); the 2 MiB default of spawned threads is tight for that.
const WORKER_STACK: usize = 16 * 1024 * 1024;

/// The pool shared by every document, created on first use. `None` when threads cannot be
/// spawned; callers then run on the current thread.
fn pool() -> Option<&'static ThreadPool> {
    static POOL: OnceLock<Option<ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .thread_name(|i| format!("pdfxray-capture-{i}"))
            .stack_size(WORKER_STACK)
            .build()
            // Without the pool the pages are captured one after another: slower, same result.
            .ok()
    })
    .as_ref()
}

/// `(0..count).map(f).collect()` with the calls spread over the pool.
pub(crate) fn map_pages<T: Send>(count: usize, f: impl Fn(usize) -> T + Sync + Send) -> Vec<T> {
    match pool() {
        Some(pool) => pool.install(|| (0..count).into_par_iter().map(&f).collect()),
        None => (0..count).map(f).collect(),
    }
}

/// Runs `f` on a pool thread, so `rayon::join` calls inside it use the pool (and its stack size)
/// rather than rayon's global one. Runs `f` right here when the pool is unavailable.
pub(crate) fn install<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    match pool() {
        Some(pool) => pool.install(f),
        None => f(),
    }
}
