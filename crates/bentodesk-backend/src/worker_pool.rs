//! T-100 — Fixed-size `std::thread` worker pool for bounded concurrency on
//! Win32 shell-cache calls (icon extraction, COM-heavy enumeration).
//!
//! ## Why bounded?
//!
//! `SHGetFileInfoW` / `SHGetImageList` / `IShellFolder::*` walk the Shell
//! cache and may hit COM activation, registry queries, and slow icon
//! handlers (`.lnk` resolvers in particular). Spinning up an unbounded
//! `std::thread::spawn` per call risks:
//!
//! 1. Thread-stack inflation (each Rust thread defaults to 2 MiB; 50 icons
//!    in flight = 100 MiB just in stacks — blows the spec §1 100 MB cap).
//! 2. Shell-cache contention — too many concurrent `SHGetFileInfoW` calls
//!    serialise behind the Shell mutex anyway, so extra threads add latency
//!    without throughput.
//!
//! A small fixed pool (default 4) caps both costs while still keeping the
//! UI thread free of synchronous blocking calls.
//!
//! ## Why hand-rolled?
//!
//! Spec §8 forbids `rayon`, `tokio`, `async-std`, `smol`, `crossbeam-deque`,
//! and every other "thread-pool" crate. The pool we need is ~80 LOC of
//! `std::thread::spawn` + `crossbeam_channel::unbounded` — zero churn vs.
//! pulling a 4 kLOC dep.
//!
//! ## Threading model
//!
//! - The pool spawns N workers up-front in [`WorkerPool::new`].
//! - Each worker loops on `recv()` against a single `crossbeam_channel`.
//! - [`WorkerPool::submit`] sends a `Box<dyn FnOnce() + Send>` over the
//!   channel; the next idle worker pops it.
//! - [`WorkerPool::shutdown`] drops the sender (channel disconnects),
//!   joins all workers. Idempotent — `Drop` calls it too.
//!
//! Spec §9: zero async runtime; all blocking is synchronous on worker
//! threads which is what the Shell APIs expect anyway.

use std::sync::Mutex;
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender, unbounded};

/// Default number of workers when [`WorkerPool::new`] is called without an
/// explicit size. Picked to match the rule-of-thumb that 4 concurrent
/// `SHGetFileInfoW` calls saturate the Shell cache mutex on a typical
/// Windows install — more workers don't help, fewer leave the UI waiting.
pub const DEFAULT_POOL_SIZE: usize = 4;

/// A boxed closure dispatched to a worker thread.
type Job = Box<dyn FnOnce() + Send + 'static>;

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ──────────────

/// Errors surfaced by the worker pool.
#[derive(Debug)]
pub enum WorkerPoolError {
    /// `submit` was called after the pool was shut down (sender dropped).
    PoolShutDown,
    /// A worker thread panicked while joining (should never happen in
    /// production — we deny `panic` cluster-wide via clippy).
    JoinFailed { thread_index: usize },
}

impl core::fmt::Display for WorkerPoolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PoolShutDown => f.write_str("worker pool already shut down"),
            Self::JoinFailed { thread_index } => {
                write!(f, "worker thread {thread_index} failed to join")
            }
        }
    }
}

impl core::error::Error for WorkerPoolError {}

// ─── Pool ────────────────────────────────────────────────────────────

/// Fixed-size worker pool.
///
/// Construct with [`WorkerPool::new`] (or [`WorkerPool::with_size`] for a
/// non-default count), submit jobs via [`WorkerPool::submit`], optionally
/// call [`WorkerPool::shutdown`] for an explicit drain — `Drop` does the
/// same automatically.
pub struct WorkerPool {
    sender: Mutex<Option<Sender<Job>>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl WorkerPool {
    /// Spawn a pool sized at [`DEFAULT_POOL_SIZE`].
    pub fn new() -> Self {
        Self::with_size(DEFAULT_POOL_SIZE)
    }

    /// Spawn a pool with `size` workers. `size` is clamped to at least 1
    /// (a zero-worker pool would block forever on submit).
    pub fn with_size(size: usize) -> Self {
        let size = size.max(1);
        let (tx, rx) = unbounded::<Job>();
        let mut workers = Vec::with_capacity(size);

        for index in 0..size {
            let rx_clone: Receiver<Job> = rx.clone();
            let rx_fallback: Receiver<Job> = rx.clone();
            let handle = match std::thread::Builder::new()
                .name(format!("bentodesk-worker-{index}"))
                .spawn(move || worker_loop(index, rx_clone))
            {
                Ok(h) => h,
                Err(err) => {
                    // `Builder::spawn` only fails when the OS rejects the
                    // named thread; fall back to plain `spawn` so the pool
                    // stays alive at the cost of an unnamed thread.
                    tracing::warn!(
                        "worker pool: failed to spawn named thread {index}: {err}; falling back",
                    );
                    std::thread::spawn(move || worker_loop(index, rx_fallback))
                }
            };
            workers.push(handle);
        }

        Self {
            sender: Mutex::new(Some(tx)),
            workers: Mutex::new(workers),
        }
    }

    /// Submit a job to the pool.
    ///
    /// The job runs on the next idle worker thread. Returns
    /// [`WorkerPoolError::PoolShutDown`] if the pool has been shut down.
    pub fn submit<F>(&self, job: F) -> Result<(), WorkerPoolError>
    where
        F: FnOnce() + Send + 'static,
    {
        let guard = match self.sender.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let Some(sender) = guard.as_ref() else {
            return Err(WorkerPoolError::PoolShutDown);
        };
        sender
            .send(Box::new(job))
            .map_err(|_| WorkerPoolError::PoolShutDown)
    }

    /// Number of worker threads owned by this pool.
    ///
    /// Returns 0 after [`WorkerPool::shutdown`]; otherwise equals the
    /// constructor argument (clamped to ≥1).
    pub fn size(&self) -> usize {
        let guard = match self.workers.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.len()
    }

    /// Drain the pool: drop the sender so workers see channel disconnect,
    /// then join every worker handle. Idempotent.
    pub fn shutdown(&self) -> Result<(), WorkerPoolError> {
        // Drop the sender first so workers see EOF on the channel.
        {
            let mut guard = match self.sender.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            *guard = None;
        }

        // Join every worker. We swap the Vec out so we don't hold the lock
        // across joins (which could deadlock if a worker tried to call back
        // into the pool — currently they don't, but the swap is cheap and
        // makes the lock-discipline obvious).
        let workers: Vec<JoinHandle<()>> = {
            let mut guard = match self.workers.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            std::mem::take(&mut *guard)
        };

        for (index, handle) in workers.into_iter().enumerate() {
            if handle.join().is_err() {
                return Err(WorkerPoolError::JoinFailed {
                    thread_index: index,
                });
            }
        }

        Ok(())
    }
}

impl Default for WorkerPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        if let Err(err) = self.shutdown() {
            tracing::warn!("worker pool drop: shutdown failed: {err}");
        }
    }
}

/// Worker thread loop. Pops jobs and runs them until the channel disconnects.
fn worker_loop(index: usize, rx: Receiver<Job>) {
    tracing::trace!("worker pool: thread {index} started");
    while let Ok(job) = rx.recv() {
        // We deliberately let the closure run to completion without
        // catching panics. `panic = "abort"` (spec §7) means a panicking
        // job aborts the whole process anyway, and clippy `panic = deny`
        // forbids any panic in the codebase. If a Win32 binding inside
        // the closure ever panics, that's a bug we want to surface
        // immediately — not swallow.
        job();
    }
    tracing::trace!("worker pool: thread {index} exiting");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    #[test]
    fn default_pool_has_default_size() {
        let pool = WorkerPool::new();
        assert_eq!(pool.size(), DEFAULT_POOL_SIZE);
    }

    #[test]
    fn explicit_size_clamped_to_at_least_one() {
        let pool = WorkerPool::with_size(0);
        assert_eq!(pool.size(), 1);
    }

    #[test]
    fn submit_runs_job_on_worker_thread() {
        let pool = WorkerPool::with_size(2);
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..16 {
            let c = Arc::clone(&counter);
            pool.submit(move || {
                c.fetch_add(1, Ordering::Relaxed);
            })
            .expect("submit");
        }

        // Drain so we know all jobs ran.
        pool.shutdown().expect("shutdown");
        assert_eq!(counter.load(Ordering::Relaxed), 16);
    }

    #[test]
    fn submit_after_shutdown_errors() {
        let pool = WorkerPool::with_size(1);
        pool.shutdown().expect("shutdown");

        let result = pool.submit(|| {});
        assert!(matches!(result, Err(WorkerPoolError::PoolShutDown)));
    }

    #[test]
    fn pool_uses_multiple_threads_concurrently() {
        // Two workers, two slow jobs — total time should be ~one slow-job
        // worth, not two. Margin generous to keep the test stable on
        // loaded CI runners.
        let pool = WorkerPool::with_size(2);
        let start = Instant::now();
        for _ in 0..2 {
            pool.submit(|| {
                std::thread::sleep(Duration::from_millis(120));
            })
            .expect("submit");
        }
        pool.shutdown().expect("shutdown");
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(220),
            "expected concurrent execution, got {elapsed:?}",
        );
    }

    #[test]
    fn shutdown_is_idempotent() {
        let pool = WorkerPool::with_size(1);
        pool.shutdown().expect("first shutdown");
        pool.shutdown().expect("second shutdown is noop");
    }

    #[test]
    fn drop_drains_pending_jobs() {
        let counter = Arc::new(AtomicUsize::new(0));
        {
            let pool = WorkerPool::with_size(1);
            for _ in 0..8 {
                let c = Arc::clone(&counter);
                pool.submit(move || {
                    c.fetch_add(1, Ordering::Relaxed);
                })
                .expect("submit");
            }
            // Pool dropped here — Drop calls shutdown which waits for all
            // queued jobs to drain.
        }
        assert_eq!(counter.load(Ordering::Relaxed), 8);
    }
}
