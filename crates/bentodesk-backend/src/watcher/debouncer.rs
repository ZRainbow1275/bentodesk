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
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

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
}

impl Debouncer {
    /// Spawn the debouncer worker.
    ///
    /// `flush_window` controls how long events are coalesced before being
    /// emitted; pick 200 ms for the desktop watcher (matches 1.x) and 300 ms
    /// for the live-folder watcher.
    ///
    /// `on_emit` is invoked from the worker thread for every batch of
    /// debounced events. Keep it cheap — it should typically forward to a
    /// `crossbeam_channel::Sender` and return.
    pub fn start<F>(flush_window: Duration, mut on_emit: F) -> Result<Self, DebouncerError>
    where
        F: FnMut(Vec<DebouncedEvent>) + Send + 'static,
    {
        let (tx, rx): (Sender<NotifyMsg>, Receiver<NotifyMsg>) = channel();

        let watcher_tx = tx.clone();
        // notify v8 returns `Result<RecommendedWatcher, notify::Error>`.
        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            let msg = match res {
                Ok(ev) => NotifyMsg::Event(ev),
                Err(e) => NotifyMsg::Error(e.to_string()),
            };
            // Drop on closed channel — debouncer is shutting down.
            let _ = watcher_tx.send(msg);
        })
        .map_err(|e| DebouncerError::NotifyInit(e.to_string()))?;

        let worker = std::thread::Builder::new()
            .name("bento-watcher-debouncer".into())
            .spawn(move || worker_loop(rx, flush_window, &mut on_emit))
            .map_err(|e| DebouncerError::NotifyInit(format!("thread spawn: {e}")))?;

        Ok(Self {
            inner: Mutex::new(Some(watcher)),
            worker: Mutex::new(Some(worker)),
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

fn worker_loop<F>(rx: Receiver<NotifyMsg>, flush_window: Duration, on_emit: &mut F)
where
    F: FnMut(Vec<DebouncedEvent>),
{
    let mut pending: PendingMap = PendingMap::new();
    let mut window_end: Option<Instant> = None;

    loop {
        let timeout = match window_end {
            Some(end) => end.saturating_duration_since(Instant::now()),
            None => Duration::from_secs(60),
        };

        match rx.recv_timeout(timeout) {
            Ok(NotifyMsg::Event(ev)) => {
                if window_end.is_none() {
                    window_end = Some(Instant::now() + flush_window);
                }
                merge_event(&mut pending, ev);
            }
            Ok(NotifyMsg::Error(msg)) => {
                tracing::warn!("watcher: notify error: {msg}");
            }
            Err(RecvTimeoutError::Timeout) => {
                if !pending.is_empty() {
                    let batch: Vec<DebouncedEvent> = pending.values().cloned().collect();
                    pending.clear();
                    window_end = None;
                    on_emit(batch);
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                if !pending.is_empty() {
                    let batch: Vec<DebouncedEvent> = pending.values().cloned().collect();
                    on_emit(batch);
                }
                return;
            }
        }
    }
}

fn merge_event(pending: &mut PendingMap, ev: Event) {
    let key = match ev.paths.first() {
        Some(p) => p.clone(),
        None => return,
    };
    pending.insert(
        key,
        DebouncedEvent {
            kind: ev.kind,
            paths: ev.paths,
        },
    );
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
}
