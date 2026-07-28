//! T-094 — Stealth-mode hidden-items subsystem (lift-verbatim from 1.x
//! `src-tauri/src/hidden_items.rs`).
//!
//! Manages "Desktop Subfolder Mode": files added to a zone are physically
//! moved into a hidden `.bentodesk/{zone_id}/` directory on the same drive
//! as the user's Desktop. Win32 `SetFileAttributesW` stamps the bundle
//! `HIDDEN | SYSTEM | NOT_CONTENT_INDEXED` so Explorer treats it as
//! "superhidden" and Windows Search ignores its contents. A safety
//! manifest at `.bentodesk/manifest.json` (mirrored at
//! `%APPDATA%/BentoDesk/manifest.mirror.json`) tracks every hidden file
//! so a layout-loss disaster still has a recovery path.
//!
//! ## Submodule split (master plan §11 R8)
//!
//! The 1.x `hidden_items.rs` is 2,797 LOC and would blow past spec §15's
//! 800-LOC ceiling per file. R8 splits it three ways along the natural
//! verb boundaries of the subsystem so each leaf file stays under the cap:
//!
//! - [`hide`] — T-094a — *putting* a file into the hidden tree
//!   (`hide_file`, directory resolution, `ensure_stealth`, Win32 attribute
//!   application).
//! - [`restore`] — T-094b — *taking* a file back out (`restore_file`,
//!   `restore_zone_items_with_dirs`, `reconcile_zone_items_with_dirs`,
//!   `restore_all_hidden`, `verify_references`).
//! - [`sync`] — T-094c — periodic upkeep + manifest I/O + legacy
//!   migration + the `AttrGuard` worker pool that drains the retry queue
//!   from a fixed-size `std::thread` pool (T-100, no unbounded spawns).
//!
//! ## Differences from 1.x
//!
//! - **No Tauri.** The 1.x module took `&AppHandle` to fetch
//!   `desktop_path` from `AppState.settings` and `layout` from
//!   `AppState.layout`. Per master plan §2 the native build is single-process
//!   with no Tauri, so every public function takes either a [`StealthConfig`]
//!   (cheap-to-clone, owned strings) or explicit `&Path` arguments. The
//!   caller (`bentodesk-app::backend_bridge`) owns settings/layout state.
//! - **`crossbeam_channel` events.** Where 1.x called `handle.emit("...")`
//!   to push status updates to the webview, this crate exposes
//!   [`StealthEvent`] over a `crossbeam_channel::Sender` parameter.
//! - **No `chrono`.** Spec §8 whitelist excludes `chrono` for this crate;
//!   timestamps use a local RFC-3339 formatter built on
//!   `std::time::SystemTime` (see [`now_iso8601`]).
//! - **No `uuid`.** Filename collision suffixes use a 64-bit
//!   `SystemTime`-derived hash rendered as 8-char base16 — equivalent
//!   uniqueness for the use case (avoiding collisions inside one zone
//!   subdirectory).
//! - **Hand-rolled error enum.** Spec §8.1 forbids `thiserror`;
//!   [`StealthError`] is a plain `enum` with `impl Display + Error`.
//! - **No `attrib.exe` spawn.** Legacy `attrib +h +s` migration removes
//!   the bit by re-running `SetFileAttributesW` with the relevant flags
//!   cleared — the 1.x `Command::new("attrib")` path is dropped (spawning
//!   processes is an arch-violation on a single-process app).

pub mod hide;
pub mod restore;
pub mod sync;

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::time::now_rfc3339;

pub use hide::{
    apply_stealth_attrs, ensure_stealth, hidden_dir_for, hide_file, zone_hidden_dir_for,
};
pub use restore::{
    ReconcileReport, RestoreSkippedItem, RestoreSkippedReason, RestoreZoneItemsReport,
    reconcile_zone_items_with_dirs, restore_all_hidden, restore_file, restore_file_tracked,
    restore_zone_items_with_dirs, verify_references,
};
pub use sync::{
    AttrGuard, MANIFEST_SCHEMA_VERSION, ManifestEntry, ManifestZone, SafetyManifest,
    cleanup_legacy_hidden_dir, load_manifest, manifest_add, manifest_remove,
    persist_manifest_snapshot, save_manifest, sync_zone_metadata,
};

// ─── Win32 constants (kept in a single place for the whole subsystem) ───

/// `FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED`.
///
/// Combined under one constant so [`hide`] and [`sync`] cannot drift apart
/// on which bits constitute "stealth".
#[cfg(windows)]
pub(crate) const STEALTH_ATTRS: u32 = 0x0000_0002 // FILE_ATTRIBUTE_HIDDEN
    | 0x0000_0004                                  // FILE_ATTRIBUTE_SYSTEM
    | 0x0000_2000; // FILE_ATTRIBUTE_NOT_CONTENT_INDEXED

/// Win32 `GetFileAttributesW` sentinel for "call failed".
#[cfg(windows)]
pub(crate) const INVALID_FILE_ATTRIBUTES: u32 = u32::MAX;

// ─── Public configuration / status types ────────────────────────────────

/// Cheap-to-clone configuration handed to every entry point.
///
/// In 1.x these values lived on `AppState.settings` + `AppState.app_data_dir`
/// and were fetched on every call via `app_handle.state::<AppState>()`. The
/// native port hoists them into one struct so callers can decide once where
/// `desktop_path` and `app_data_dir` come from (typically read from a
/// `RwLock<Settings>` and cloned).
///
/// Both paths are stored as `String` (not `PathBuf`) so the struct is
/// `serde::Serialize/Deserialize` without the `serde[std]` feature flag —
/// native's workspace serde dep is `default-features = false` per spec §6
/// minimal-RSS guidance, and `PathBuf` requires that feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthConfig {
    /// Absolute path to the user's Desktop directory (where `.bentodesk/`
    /// is created as a child).
    pub desktop_path: SmolStr,
    /// `%APPDATA%/BentoDesk/` (or platform equivalent). Used for the
    /// manifest mirror and legacy migration scans.
    pub app_data_dir: SmolStr,
}

impl StealthConfig {
    /// Convenience: `app_data_dir` as a `Path` for std::fs interop.
    pub fn app_data_path(&self) -> &std::path::Path {
        std::path::Path::new(self.app_data_dir.as_str())
    }
}

/// Snapshot of the stealth subsystem state, produced by [`status`] and
/// pushed over the [`StealthEvent`] channel whenever it changes.
///
/// The shape is `serde::Serialize + Deserialize` per master plan §11 ΔB
/// ruling so a future v2.x scripting hook can re-introduce serialization
/// without breaking compat. At runtime the single-process build never
/// serializes it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StealthStatus {
    /// `true` once the most recent `ensure_stealth` call succeeded with
    /// no paths queued for retry.
    pub applied: bool,
    /// Last OS error message (rendered Win32 GetLastError) when an apply
    /// attempt failed. `None` on a clean state.
    pub last_error: Option<String>,
    /// Number of paths queued for retry (typically OneDrive holds a lock).
    pub retry_count: u32,
    /// Schema version of the safety manifest loaded from disk.
    pub schema_version: SmolStr,
    /// `true` when primary and mirror manifest are byte-identical after
    /// the last save/load cycle.
    pub mirror_healthy: bool,
}

/// Events broadcast over the [`crossbeam_channel`] handed to entry points.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StealthEvent {
    /// Status snapshot — emitted whenever [`StealthStatus`] mutates.
    StatusChanged(StealthStatus),
    /// A `hide_file` call succeeded; payload is `(original_path, hidden_path)`.
    Hidden { original: String, hidden: String },
    /// A `restore_file` call succeeded; payload is `(original_path, hidden_path)`.
    Restored { original: String, hidden: String },
    /// `restore_all_hidden` finished.
    RestoreAllComplete { total: u32 },
    /// Periodic sweep finished (`AttrGuard::sweep`).
    SweepComplete { applied: u32, queued: u32 },
}

/// Errors surfaced by the stealth subsystem.
///
/// Hand-rolled per spec §8.1 — no `thiserror`. Variants carry enough
/// structure for the renderer to render a meaningful message without
/// exposing raw `std::io` types across the public API. Paths are owned
/// `PathBuf` (the variant is `Debug` only — never serialized — so the
/// `serde[std]` feature is not required).
#[derive(Debug)]
pub enum StealthError {
    /// `desktop_path` was empty or relative — refusing to resolve hidden
    /// dir against the process cwd.
    InvalidDesktopPath { value: String },
    /// File-system I/O failure with the underlying message.
    Io { path: PathBuf, message: String },
    /// `SetFileAttributesW` failed; payload is the raw Win32 error code.
    Win32Attribute { path: PathBuf, win32_error: u32 },
    /// Manifest parse failure (corrupt JSON, missing required field).
    ManifestParse { path: PathBuf, message: String },
    /// Asked to restore a file that does not exist in the hidden mirror.
    RestoreSourceMissing { path: PathBuf },
    /// Refused to hide a file that is not on the desktop / does not exist.
    HideSourceMissing { path: PathBuf },
}

impl core::fmt::Display for StealthError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidDesktopPath { value } => {
                write!(f, "invalid desktop_path '{}' — must be absolute", value)
            }
            Self::Io { path, message } => {
                write!(f, "stealth io error at {}: {}", path.display(), message)
            }
            Self::Win32Attribute { path, win32_error } => write!(
                f,
                "SetFileAttributesW failed for {}: GetLastError={}",
                path.display(),
                win32_error
            ),
            Self::ManifestParse { path, message } => {
                write!(f, "manifest parse error at {}: {}", path.display(), message)
            }
            Self::RestoreSourceMissing { path } => {
                write!(f, "restore source missing: {}", path.display())
            }
            Self::HideSourceMissing { path } => {
                write!(f, "hide source missing: {}", path.display())
            }
        }
    }
}

impl core::error::Error for StealthError {}

// ─── Shared state (exists for the whole stealth subsystem) ───────────────

/// Internal state shared across [`hide`] / [`restore`] / [`sync`] modules.
///
/// Wraps the public [`StealthStatus`] and the deduplicated retry queue used
/// by the AttrGuard worker pool. Mutex-poisoning recovers via
/// `unwrap_or_else(|e| e.into_inner())` — see the helpers below.
#[derive(Debug)]
pub(crate) struct SharedState {
    pub(crate) status: StealthStatus,
    /// Paths whose stealth attributes could not be applied. Drained by
    /// `AttrGuard::sweep` on each tick.
    pub(crate) retry_queue: Vec<PathBuf>,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            status: StealthStatus {
                applied: false,
                last_error: None,
                retry_count: 0,
                schema_version: SmolStr::new_static(MANIFEST_SCHEMA_VERSION),
                mirror_healthy: true,
            },
            retry_queue: Vec::new(),
        }
    }
}

pub(crate) fn shared() -> &'static Mutex<SharedState> {
    static STATE: OnceLock<Mutex<SharedState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(SharedState::default()))
}

/// Acquire the shared state. Recovers from poisoning by taking the inner
/// guard — the recorded status is a best-effort observability surface, not
/// a correctness invariant.
pub(crate) fn with_shared<R, F: FnOnce(&mut SharedState) -> R>(f: F) -> R {
    let mut guard = shared()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}

/// Read-only snapshot of the current status.
pub fn status() -> StealthStatus {
    with_shared(|s| s.status.clone())
}

pub(crate) fn record_success() {
    with_shared(|s| {
        s.status.applied = true;
        s.status.last_error = None;
    });
}

pub(crate) fn record_failure(err: &str, path: &std::path::Path) {
    with_shared(|s| {
        s.status.applied = false;
        s.status.last_error = Some(err.to_string());
        if !s.retry_queue.iter().any(|p| p == path) {
            s.retry_queue.push(path.to_path_buf());
        }
        s.status.retry_count = s.retry_queue.len() as u32;
    });
}

pub(crate) fn record_retry_drained(path: &std::path::Path) {
    with_shared(|s| {
        s.retry_queue.retain(|p| p != path);
        s.status.retry_count = s.retry_queue.len() as u32;
        if s.retry_queue.is_empty() {
            s.status.applied = true;
        }
    });
}

pub(crate) fn set_mirror_healthy(healthy: bool) {
    with_shared(|s| s.status.mirror_healthy = healthy);
}

pub(crate) fn set_schema_version(version: &str) {
    with_shared(|s| s.status.schema_version = SmolStr::new(version));
}

// ─── Time + uniqueness helpers (no chrono / uuid) ───────────────────────

/// RFC-3339 UTC timestamp delegating to the shared `crate::time::now_rfc3339`
/// helper (Wave 5d T-Q1, hand-rolled in lieu of `chrono`). Wrapped here so
/// stealth/* internals don't have to thread a free-function import through
/// every call site.
pub(crate) fn now_iso8601() -> String {
    now_rfc3339()
}

/// 8-char hex suffix for filename collision disambiguation. Not a UUID — a
/// `SystemTime` + atomic counter blend that is collision-resistant within a
/// single zone subdirectory (the only scope it matters in).
pub(crate) fn unique_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let bumped = COUNTER.fetch_add(1, Ordering::Relaxed);

    // FNV-1a-style mix of nanos + counter; 8 hex chars = 32 bits is plenty
    // for per-directory uniqueness.
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for byte in nanos
        .to_le_bytes()
        .iter()
        .chain(bumped.to_le_bytes().iter())
    {
        h ^= *byte as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{:08x}", (h ^ (h >> 32)) as u32)
}

// ─── Path helpers used by all three submodules ──────────────────────────

/// Case-insensitive Windows-style path comparison.
pub(crate) fn paths_equal_str(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    norm(a) == norm(b)
}

/// Strip the Windows extended-length path prefix (`\\?\`).
pub(crate) fn strip_unc_prefix(p: &std::path::Path) -> std::path::PathBuf {
    let s = p.to_string_lossy();
    s.strip_prefix(r"\\?\")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| p.to_path_buf())
}

/// Compare two paths after canonicalization + UNC strip. Falls back to
/// `false` when canonicalization fails (e.g. either path does not exist).
pub(crate) fn paths_match(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => strip_unc_prefix(&ca) == strip_unc_prefix(&cb),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_iso8601_delegates_to_time_helper() {
        let s = now_iso8601();
        assert!(s.ends_with('Z'));
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
        assert_eq!(&s[10..11], "T");
    }

    #[test]
    fn unique_suffix_is_8_hex_chars() {
        let s = unique_suffix();
        assert_eq!(s.len(), 8);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn unique_suffix_does_not_collide_under_burst() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(seen.insert(unique_suffix()));
        }
    }

    #[test]
    fn paths_equal_str_case_and_separator_insensitive() {
        assert!(paths_equal_str(r"C:\Foo\Bar", "c:/foo/bar"));
        assert!(paths_equal_str(r"D:\X", r"d:\x"));
        assert!(!paths_equal_str(r"C:\foo", r"C:\bar"));
    }

    #[test]
    fn strip_unc_prefix_known_inputs() {
        assert_eq!(
            strip_unc_prefix(std::path::Path::new(r"\\?\C:\Users\Desktop")),
            std::path::PathBuf::from(r"C:\Users\Desktop")
        );
        assert_eq!(
            strip_unc_prefix(std::path::Path::new(r"C:\Users\Desktop")),
            std::path::PathBuf::from(r"C:\Users\Desktop")
        );
    }
}
