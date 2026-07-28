//! T-089 — time-machine timeline for desktop layout history.
//!
//! Provides a ring buffer of auto-captured checkpoints plus an unlimited
//! list of manual-pinned checkpoints. The dispatcher's write paths call
//! [`hook::TimelineHook::record_change`] AFTER a successful mutation,
//! which coalesces rapid bursts via a 500 ms debounce window.
//!
//! ## Storage layout
//!
//! `<state_dir>/timeline/checkpoint-{YYYYMMDDTHHMMSSmmmZ-XXXXXXXX}.json`.
//!
//! ## What changed vs 1.x
//!
//! - **Q1**: `chrono::Utc::now()` and the `chrono::DateTime` parser
//!   replaced by [`crate::time::now_compact_rfc3339`]. The id suffix
//!   formerly emitted by `uuid::Uuid::new_v4()[..8]` is now an
//!   8-char-hex epoch-nanos suffix.
//! - **Tauri removal**: every entry is decoupled from `AppHandle` and
//!   `AppState`. The hook receives a snapshot via an injected
//!   `SnapshotProvider` closure and emits `TimelineEvent::Updated` on a
//!   `crossbeam_channel::Sender` instead of `app.emit("timeline_updated", id)`.
//! - **Spec §8.1**: hand-rolled [`checkpoint::CheckpointError`] replaces
//!   `BentoDeskError`.

pub mod checkpoint;
pub mod hook;
pub mod ring_buffer;

pub use checkpoint::{
    Checkpoint, CheckpointError, CheckpointMeta, CheckpointStore, DeltaSummary, compute_delta,
    new_checkpoint_id,
};
pub use hook::{
    COALESCE_MAX_WINDOW, DEBOUNCE_WINDOW, SIGNIFICANT_ITEM_THRESHOLD, SnapshotProvider,
    TimelineEvent, TimelineHook, on_significant_change,
};
pub use ring_buffer::{
    AutoCoalesceMode, DEFAULT_AUTO_CAPACITY, DEFAULT_PERSISTED_COALESCE_WINDOW_SECS, TimelineBuffer,
};
