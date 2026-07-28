//! T-082 — Filesystem watcher (lift-verbatim from 1.x
//! `src-tauri/src/watcher/`).
//!
//! Two watchers ship:
//!
//! - [`desktop_watcher`] — watches one or more *Desktop* directories
//!   (User / Public / OneDrive overrides) and pushes [`FileChangedEvent`]s
//!   for create / modify / delete (plus rename pairs).
//! - [`live_folder`]    — binds an arbitrary user-chosen folder to a
//!   [`bentodesk_zone::ZoneId`]; emits a
//!   [`live_folder::ZoneRefreshEvent`] whenever any file under the folder
//!   changes so the zone view can re-scan the folder contents.
//!
//! ## What changed vs 1.x
//!
//! | 1.x                                               | native                                                                     |
//! |---------------------------------------------------|--------------------------------------------------------------------------|
//! | `notify_debouncer_full::Debouncer`                | hand-rolled [`Debouncer`] (spec §8 forbids `notify-debouncer-full`)      |
//! | `tauri::AppHandle::emit("file_changed", payload)` | `crossbeam_channel::Sender<FileChangedEvent>` injected by the caller      |
//! | `tauri::State<AppState>` for path overrides       | caller passes `&[PathBuf]` of directories to watch                        |
//! | `crate::desktop_sources::all_desktop_dirs`        | out of T-082 scope — caller (eventually T-093) supplies the source list   |
//! | string `zone_id`                                  | typed [`bentodesk_zone::ZoneId`] (u64)                                  |
//!
//! ## Threading model
//!
//! `notify` v7 invokes its event-handler closure on its own background thread
//! (one per platform-specific backend). We feed those events into a
//! `std::sync::mpsc` channel, then a dedicated [`std::thread`] pulls them out
//! and runs the debounce window. The debouncer thread is the only place we
//! convert raw notify events into [`FileChangedEvent`] and forward over the
//! caller's [`crossbeam_channel::Sender`]. Spec §9: zero async runtime.

pub mod debouncer;
pub mod desktop_watcher;
pub mod live_folder;

pub use debouncer::{Debouncer, DebouncerError};
pub use desktop_watcher::{
    ChangeKind, DesktopWatcher, FileChangedEvent, WatcherError, setup_file_watcher,
};
pub use live_folder::{
    BLACKLISTED_PREFIXES, LiveFolderError, ZoneRefreshEvent, bind, ensure_initialised, unbind,
    validate_folder,
};
