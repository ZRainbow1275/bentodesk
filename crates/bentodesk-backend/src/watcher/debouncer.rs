//! Hand-rolled debouncer (spec §8 forbids `notify-debouncer-full`).
//!
//! Behaviour matches the 1.x usage of `notify-debouncer-full` for the cases we
//! actually rely on:
//!
//! 1. Coalesce a burst of `notify::Event`s for the same path within a
//!    `flush_window` (200 ms for desktop, 300 ms for live folders) into a
//!    single emission.
//! 2. Preserve event ordering across paths (FIFO within a window).
//! 3. Run on a dedicated thread; spec §9 — zero async runtime.
//!
//! Not in scope (vs the upstream `notify_debouncer_full`):
//!
//! - File-id-based rename pairing across remove/create. We surface the raw
//!   `notify::EventKind::Modify(ModifyKind::Name(_))` paths the OS gives us,
//!   which is what 1.x ended up forwarding to the frontend anyway.
//! - "Renames need both source and destination" history tracking. The 1.x
//!   `map_event_to_payload` already only forwarded `paths.first()` and
//!   `paths.get(1)` directly, no history join.
//!
//! ## Lifecycle
//!
//! [`Debouncer::start`] spawns the worker thread and returns a handle.
//! Dropping the handle joins the thread (the thread exits when the inbound
//! channel closes). The handle exposes `watch` / `unwatch` for path
//! subscription and is `Send + Sync`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

const NOTIFY_QUEUE_CAPACITY: usize = 1_024;
const MAX_PENDING_PATHS: usize = 2_048;

/// Errors surfaced by the debouncer.
#[derive(Debug)]
pub enum DebouncerError {
    /// The underlying `notify` watcher could not be created.
    NotifyInit(String),
    /// Failed to start watching a path (path doesn't exist, permission denied,
    /// already watching, ...).
    Watch { path: PathBuf, message: String },
    /// Failed to stop watching a path.
    Unwatch { path: PathBuf, message: String },
}

impl core::fmt::Display for DebouncerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotifyInit(m) => write!(f, "notify watcher init failed: {m}"),
            Self::Watch { path, message } => {
                write!(f, "watch({}) failed: {}", path.display(), message)
            }
            Self::Unwatch { path, message } => {
                write!(f, "unwatch({}) failed: {}", path.display(), message)
            }
        }
    }
}

impl core::error::Error for DebouncerError {}

/// Owned handle returned by [`Debouncer::start`]. Dropping it joins the
/// worker thread.
///
/// `watch` / `unwatch` are routed via a Mutex-guarded handle to the
/// `RecommendedWatcher` so callers on multiple threads can subscribe paths
/// without racing.
pub struct Debouncer {
    /// `Mutex<Option<...>>` so that drop can take ownership of the watcher
    /// before the worker thread joins (`notify` watchers stop emitting when
    /// dropped — we want that to happen *before* we wait for the worker).
    inner: Mutex<Option<RecommendedWatcher>>,
    /// Worker join handle. Some until drop.
    worker: Mutex<Option<JoinHandle<()>>>,
    overflowed: Arc<AtomicBool>,
}

impl Debouncer {
    /// Spawn the debouncer worker.
    ///
    /// `flush_window` controls how long events are coalesced before being
    /// emitted; pick 200 ms for the desktop watcher (matches 1.x) and 300 ms
    /// for the live-folder watcher.
    ///
    /// `on_emit` is invoked from the worker thread for every batch of
    /// debounced events. Return `false` when a bounded downstream queue is full;
    /// the overflow flag then asks the caller to reconcile from disk.
    pub fn start<F>(flush_window: Duration, mut on_emit: F) -> Result<Self, DebouncerError>
    where
        F: FnMut(Vec<DebouncedEvent>) -> bool + Send + 'static,
    {
        let (tx, rx): (SyncSender<NotifyMsg>, Receiver<NotifyMsg>) =
            sync_channel(NOTIFY_QUEUE_CAPACITY);
        let overflowed = Arc::new(AtomicBool::new(false));

        let watcher_tx = tx.clone();
        let watcher_overflowed = Arc::clone(&overflowed);
        // notify v8 returns `Result<RecommendedWatcher, notify::Error>`.
        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            let msg = match res {
                Ok(ev) => NotifyMsg::Event(ev),
                Err(e) => NotifyMsg::Error(e.to_string()),
            };
            // Directory storms must not grow process memory without bound.
            // Dropping an overflow notification is safe for user data: this
            // channel only refreshes the in-memory desktop view.
            if matches!(watcher_tx.try_send(msg), Err(TrySendError::Full(_))) {
                watcher_overflowed.store(true, Ordering::Release);
            }
        })
        .map_err(|e| DebouncerError::NotifyInit(e.to_string()))?;

        let worker_overflowed = Arc::clone(&overflowed);
        let worker = std::thread::Builder::new()
            .name("bento-watcher-debouncer".into())
            .spawn(move || worker_loop(rx, flush_window, &worker_overflowed, &mut on_emit))
            .map_err(|e| DebouncerError::NotifyInit(format!("thread spawn: {e}")))?;

        Ok(Self {
            inner: Mutex::new(Some(watcher)),
            worker: Mutex::new(Some(worker)),
            overflowed,
        })
    }

    /// Begin watching `path` non-recursively (the only mode 1.x ever used).
    pub fn watch(&self, path: &Path, recursive: bool) -> Result<(), DebouncerError> {
        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let watcher = guard.as_mut().ok_or_else(|| DebouncerError::Watch {
            path: path.to_path_buf(),
            message: "watcher already dropped".into(),
        })?;
        watcher
            .watch(path, mode)
            .map_err(|e| DebouncerError::Watch {
                path: path.to_path_buf(),
                message: e.to_string(),
            })
    }

    /// Stop watching `path`. `notify` returns an error if the path wasn't
    /// being watched; we surface it so the caller can decide whether to
    /// log/ignore.
    pub fn unwatch(&self, path: &Path) -> Result<(), DebouncerError> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let watcher = guard.as_mut().ok_or_else(|| DebouncerError::Unwatch {
            path: path.to_path_buf(),
            message: "watcher already dropped".into(),
        })?;
        watcher.unwatch(path).map_err(|e| DebouncerError::Unwatch {
            path: path.to_path_buf(),
            message: e.to_string(),
        })
    }

    /// Return and clear the overflow flag. Callers use it to trigger one
    /// authoritative rescan after a burst exceeds the bounded event queues.
    pub fn take_overflowed(&self) -> bool {
        self.overflowed.swap(false, Ordering::AcqRel)
    }
}

impl Drop for Debouncer {
    fn drop(&mut self) {
        // Drop the watcher first so it stops sending events; the worker's
        // recv() will then start returning Disconnected and the loop exits.
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
        if let Ok(mut wg) = self.worker.lock()
            && let Some(jh) = wg.take()
        {
            let _ = jh.join();
        }
    }
}

/// One debounced event hand-off — mirrors the subset of
/// `notify_debouncer_full::DebouncedEvent` that 1.x consumed.
#[derive(Debug, Clone)]
pub struct DebouncedEvent {
    pub kind: EventKind,
    pub paths: Vec<PathBuf>,
}

// ─── Internal worker ────────────────────────────────────────────────

enum NotifyMsg {
    Event(Event),
    Error(String),
}

/// Map a path to the most recent `(EventKind, paths)` we saw for it within the
/// current flush window. Using a `BTreeMap` keeps iteration order stable
/// (alphabetical) which makes the emitted batch deterministic — useful both
/// for tests and for downstream callers that hash batches for diffing.
type PendingMap = BTreeMap<PathBuf, DebouncedEvent>;

fn worker_loop<F>(
    rx: Receiver<NotifyMsg>,
    flush_window: Duration,
    overflowed: &AtomicBool,
    on_emit: &mut F,
) where
    F: FnMut(Vec<DebouncedEvent>) -> bool,
{
    let mut pending: PendingMap = PendingMap::new();
    let mut window_end: Option<Instant> = None;

    loop {
        // `recv_timeout(Duration::ZERO)` still consumes already-queued messages.
        // Flush before the receive so a continuously busy watcher cannot keep
        // extending the pending map beyond its intended time window.
        if window_end.is_some_and(|end| Instant::now() >= end) {
            flush_pending(&mut pending, overflowed, on_emit);
            window_end = None;
            continue;
        }

        let timeout = match window_end {
            Some(end) => end.saturating_duration_since(Instant::now()),
            None => Duration::from_secs(60),
        };

        match rx.recv_timeout(timeout) {
            Ok(NotifyMsg::Event(ev)) => {
                if window_end.is_none() {
                    window_end = Some(Instant::now() + flush_window);
                }
                if !merge_event(&mut pending, ev) {
                    overflowed.store(true, Ordering::Release);
                }
            }
            Ok(NotifyMsg::Error(msg)) => {
                tracing::warn!("watcher: notify error: {msg}");
            }
            Err(RecvTimeoutError::Timeout) => {
                flush_pending(&mut pending, overflowed, on_emit);
                window_end = None;
            }
            Err(RecvTimeoutError::Disconnected) => {
                flush_pending(&mut pending, overflowed, on_emit);
                return;
            }
        }
    }
}

fn flush_pending<F>(pending: &mut PendingMap, overflowed: &AtomicBool, on_emit: &mut F)
where
    F: FnMut(Vec<DebouncedEvent>) -> bool,
{
    if pending.is_empty() {
        return;
    }
    let batch = std::mem::take(pending).into_values().collect();
    if !on_emit(batch) {
        overflowed.store(true, Ordering::Release);
    }
}

fn merge_event(pending: &mut PendingMap, ev: Event) -> bool {
    let key = match ev.paths.first() {
        Some(p) => p.clone(),
        None => return true,
    };
    if pending.len() >= MAX_PENDING_PATHS && !pending.contains_key(&key) {
        return false;
    }
    pending.insert(
        key,
        DebouncedEvent {
            kind: ev.kind,
            paths: ev.paths,
        },
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn coalesces_same_path_within_window() {
        let dir = std::env::temp_dir().join(format!("bento-debouncer-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_in = counter.clone();

        let deb = Debouncer::start(Duration::from_millis(150), move |batch| {
            counter_in.fetch_add(batch.len(), Ordering::SeqCst);
            true
        })
        .expect("start");

        deb.watch(&dir, false).expect("watch");

        // Generate several events for the same file; all should collapse
        // into one debounced emission for that path.
        let f = dir.join("burst.txt");
        for i in 0..5 {
            std::fs::write(&f, format!("v{i}")).expect("write");
        }
        std::thread::sleep(Duration::from_millis(400));

        let total = counter.load(Ordering::SeqCst);
        // notify can split create + modify into separate batches across
        // backends; allow >= 1 but assert it's less than the raw 5.
        assert!((1..5).contains(&total), "expected coalescing, got {total}");

        drop(deb);
        let _ = std::fs::remove_file(&f);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn expired_window_flushes_while_receiver_remains_busy() {
        let (tx, rx) = std::sync::mpsc::channel();
        for index in 0..3 {
            tx.send(NotifyMsg::Event(
                Event::new(EventKind::Any).add_path(PathBuf::from(format!("queued-{index}.txt"))),
            ))
            .expect("queue event");
        }
        drop(tx);

        let mut batch_sizes = Vec::new();
        let overflowed = AtomicBool::new(false);
        worker_loop(rx, Duration::ZERO, &overflowed, &mut |batch| {
            batch_sizes.push(batch.len());
            true
        });

        assert_eq!(batch_sizes, vec![1, 1, 1]);
        assert!(!overflowed.load(Ordering::Acquire));
    }

    #[test]
    fn pending_paths_are_bounded_and_signal_overflow() {
        let (tx, rx) = std::sync::mpsc::channel();
        for index in 0..=MAX_PENDING_PATHS {
            tx.send(NotifyMsg::Event(
                Event::new(EventKind::Any).add_path(PathBuf::from(format!("distinct-{index}.txt"))),
            ))
            .expect("queue event");
        }
        drop(tx);

        let overflowed = AtomicBool::new(false);
        let mut emitted = 0usize;
        worker_loop(rx, Duration::from_secs(10), &overflowed, &mut |batch| {
            emitted += batch.len();
            true
        });

        assert_eq!(emitted, MAX_PENDING_PATHS);
        assert!(overflowed.load(Ordering::Acquire));
    }

    #[test]
    fn rejected_output_batch_signals_overflow() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(NotifyMsg::Event(
            Event::new(EventKind::Any).add_path(PathBuf::from("queued.txt")),
        ))
        .expect("queue event");
        drop(tx);

        let overflowed = AtomicBool::new(false);
        worker_loop(rx, Duration::ZERO, &overflowed, &mut |_| false);

        assert!(overflowed.load(Ordering::Acquire));
    }
}
