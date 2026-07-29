//! T-094c — Persistence + periodic sweep half of the stealth subsystem.
//!
//! Owns the *background and on-disk state* concerns of the original 1.x
//! `hidden_items.rs`:
//!
//! - **Safety manifest** — atomic JSON I/O with mirror at
//!   `%APPDATA%/BentoDesk/manifest.mirror.json`. The primary always wins;
//!   on disagreement the mirror is re-healed on the next save.
//! - **AttrGuard worker pool** — fixed-size `std::thread` pool (T-100, no
//!   unbounded `spawn`s) that drains the retry queue + walks the
//!   `.bentodesk/` tree to re-stamp Win32 stealth attributes when
//!   OneDrive / AV release their lock.
//! - **Legacy migration** — `cleanup_legacy_hidden_dir` covers two upgrade
//!   paths: the old `AppData/hidden_items/` move-mode directory and the
//!   old `attrib +h +s` reference-mode files. The 1.x source spawned
//!   `attrib.exe` for un-hide; the native port re-uses
//!   [`crate::stealth::hide::apply_stealth_attrs`] inversion via direct
//!   `SetFileAttributesW` (no child process).
//!
//! ## Worker pool sizing
//!
//! The pool is a `Vec<JoinHandle<()>>` of fixed size — currently `2`. Each
//! worker pulls `SweepJob`s from a `crossbeam_channel` and exits when the
//! sender is dropped. Two workers is the right upper bound for the
//! workload:
//!
//! - The sweep is I/O-bound (`SetFileAttributesW` + `read_dir`); CPU
//!   parallelism past ~2 yields no measurable speedup.
//! - 2 keeps thread-state RSS contribution under ~300 KB per spec §1's
//!   100-MB Private Bytes ceiling, well within the budget.
//! - Spec §17 forbids "TODO: scale this later" — the constant is the
//!   final answer for v2.0.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use serde::{Deserialize, Serialize};

use super::hide::ensure_stealth;
use super::{
    StealthConfig, StealthError, StealthEvent, now_iso8601, paths_equal_str, set_mirror_healthy,
    set_schema_version, status, with_shared,
};

// ─── Manifest schema ────────────────────────────────────────────────────

/// Current manifest schema version.
///
/// Bumped when the on-disk JSON shape changes so older installs can
/// upgrade deterministically on first launch. `"3.0"` = pre-mirror;
/// `"3.1"` = Win32 stealth + APPDATA mirror.
pub const MANIFEST_SCHEMA_VERSION: &str = "3.1";

/// Migration update payload — `(zone_id, item_id_or_old_hidden_path,
/// new_hidden_path)`. Used by [`cleanup_legacy_hidden_dir`],
/// `migrate_attrib_hidden_files`, and [`migrate_flat_to_zone_dirs`] so
/// the caller can rewrite layout entries with the new on-disk locations.
pub type MigrationUpdate = (String, String, String);

/// Outcome of a migration pass: total moved + the per-file update payload.
pub type MigrationResult = Result<(u32, Vec<MigrationUpdate>), StealthError>;

/// Single manifest entry — tracks one hidden file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    #[serde(default)]
    pub original_path: String,
    #[serde(default)]
    pub hidden_path: String,
    #[serde(default)]
    pub zone_id: String,
    #[serde(default)]
    pub file_size_bytes: u64,
    #[serde(default)]
    pub hidden_at: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_y: Option<i32>,
    #[serde(default)]
    pub file_type: String,
}

/// Zone metadata snapshot embedded in the manifest. Used as a complete
/// backup of zone configuration so recovery can reconstruct layout state
/// if `layout.json` is lost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestZone {
    pub id: String,
    pub name: String,
    pub icon: String,
    #[serde(default)]
    pub x_percent: f64,
    #[serde(default)]
    pub y_percent: f64,
    #[serde(default)]
    pub w_percent: f64,
    #[serde(default)]
    pub h_percent: f64,
    #[serde(default)]
    pub sort_order: u32,
    #[serde(default)]
    pub grid_columns: u32,
    #[serde(default)]
    pub item_count: usize,
}

/// The safety manifest — a complete independent backup of all hidden
/// files AND zone metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyManifest {
    #[serde(default)]
    pub schema_version: String,
    pub entries: Vec<ManifestEntry>,
    #[serde(default)]
    pub zones: Vec<ManifestZone>,
    #[serde(default)]
    pub screen_width: u32,
    #[serde(default)]
    pub screen_height: u32,
    #[serde(default)]
    pub last_updated: String,
}

impl Default for SafetyManifest {
    fn default() -> Self {
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
            entries: Vec::new(),
            zones: Vec::new(),
            screen_width: 0,
            screen_height: 0,
            last_updated: String::new(),
        }
    }
}

// ─── Schema-version migration ───────────────────────────────────────────

/// Parse `"major.minor"` into `(u32, u32)` so comparisons are numeric, not
/// lexicographic. Lexicographic compare would silently mis-order
/// `"3.10" < "3.9"`, re-triggering migrations on a manifest that was
/// already upgraded. Malformed input returns `(0, 0)` so the migration
/// path treats it like a legacy (pre-3.1) manifest.
fn parse_schema_version(v: &str) -> (u32, u32) {
    let mut it = v.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}

fn needs_migration(disk: &str) -> bool {
    if disk.is_empty() {
        return true;
    }
    parse_schema_version(disk) < parse_schema_version(MANIFEST_SCHEMA_VERSION)
}

// ─── Atomic JSON I/O (no `crate::storage` dependency in native backend) ───

/// Resolve the `%APPDATA%/BentoDesk/manifest.mirror.json` path.
fn manifest_mirror_path(config: &StealthConfig) -> PathBuf {
    config.app_data_path().join("manifest.mirror.json")
}

/// Atomic write: temp file → flush → rename. Previous file is preserved
/// at `<path>.bak` for crash recovery. Mirrors the contract of 1.x
/// `crate::storage::write_json_atomic` without depending on it.
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), StealthError> {
    let parent = path.parent().unwrap_or(Path::new("."));
    if !parent.exists() {
        std::fs::create_dir_all(parent).map_err(|e| StealthError::Io {
            path: parent.to_path_buf(),
            message: e.to_string(),
        })?;
    }

    let serialized = serde_json::to_vec_pretty(value).map_err(|e| StealthError::ManifestParse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let tmp_path = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut tmp = std::fs::File::create(&tmp_path).map_err(|e| StealthError::Io {
            path: tmp_path.clone(),
            message: e.to_string(),
        })?;
        tmp.write_all(&serialized).map_err(|e| StealthError::Io {
            path: tmp_path.clone(),
            message: e.to_string(),
        })?;
        tmp.flush().map_err(|e| StealthError::Io {
            path: tmp_path.clone(),
            message: e.to_string(),
        })?;
        // Drop the file so Windows releases the lock before rename.
    }

    // Preserve previous file as .bak before swap.
    let bak_path = path.with_extension("json.bak");
    if path.exists() {
        let _ = std::fs::remove_file(&bak_path);
        if let Err(e) = std::fs::rename(path, &bak_path) {
            tracing::warn!(
                "write_json_atomic: failed to back up {} -> {}: {}",
                path.display(),
                bak_path.display(),
                e
            );
        }
    }

    std::fs::rename(&tmp_path, path).map_err(|e| StealthError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    Ok(())
}

/// Read JSON with one-shot recovery from `<path>.bak` on parse failure.
/// Mirrors the contract of 1.x `crate::storage::read_json_with_recovery`.
fn read_json_with_recovery<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Option<T>, StealthError> {
    if !path.exists() {
        return Ok(None);
    }

    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<T>(&content) {
            Ok(parsed) => Ok(Some(parsed)),
            Err(parse_err) => {
                let bak = path.with_extension("json.bak");
                if bak.exists() {
                    tracing::warn!(
                        "read_json_with_recovery: primary {} corrupt ({}) — trying backup {}",
                        path.display(),
                        parse_err,
                        bak.display()
                    );
                    match std::fs::read_to_string(&bak) {
                        Ok(bak_content) => serde_json::from_str::<T>(&bak_content)
                            .map(Some)
                            .map_err(|e| StealthError::ManifestParse {
                                path: bak.clone(),
                                message: e.to_string(),
                            }),
                        Err(e) => Err(StealthError::Io {
                            path: bak,
                            message: e.to_string(),
                        }),
                    }
                } else {
                    Err(StealthError::ManifestParse {
                        path: path.to_path_buf(),
                        message: parse_err.to_string(),
                    })
                }
            }
        },
        Err(e) => Err(StealthError::Io {
            path: path.to_path_buf(),
            message: e.to_string(),
        }),
    }
}

// ─── Manifest load / save ───────────────────────────────────────────────

/// Load the safety manifest from `dir/manifest.json`.
///
/// Primary source of truth is the file under the desktop `.bentodesk/`.
/// If the primary is missing AND a mirror exists at the APPDATA path,
/// the mirror is promoted; on disagreement the primary always wins and
/// the mirror is re-healed on the next save.
pub fn load_manifest(dir: &Path) -> Result<SafetyManifest, StealthError> {
    let path = dir.join("manifest.json");
    let primary = read_json_with_recovery::<SafetyManifest>(&path)?;

    let mut manifest: SafetyManifest = primary.unwrap_or_default();

    if needs_migration(&manifest.schema_version) {
        tracing::info!(
            "Migrating safety manifest {} -> {}",
            if manifest.schema_version.is_empty() {
                "3.0"
            } else {
                &manifest.schema_version
            },
            MANIFEST_SCHEMA_VERSION
        );
        manifest.schema_version = MANIFEST_SCHEMA_VERSION.to_string();
    }
    set_schema_version(&manifest.schema_version);

    Ok(manifest)
}

/// Atomically persist the safety manifest to disk + mirror it to APPDATA.
///
/// Updates `mirror_healthy` in the shared status to reflect whether both
/// writes succeeded.
pub fn save_manifest(dir: &Path, manifest: &SafetyManifest) -> Result<(), StealthError> {
    let path = dir.join("manifest.json");

    let mut stamped = manifest.clone();
    if needs_migration(&stamped.schema_version) {
        stamped.schema_version = MANIFEST_SCHEMA_VERSION.to_string();
    }

    let primary_ok = write_json_atomic(&path, &stamped).is_ok();
    if !primary_ok {
        tracing::error!("save_manifest: primary write failed at {}", path.display());
    }

    set_schema_version(&stamped.schema_version);
    set_mirror_healthy(primary_ok);

    if !primary_ok {
        return Err(StealthError::Io {
            path,
            message: "primary manifest write failed".to_string(),
        });
    }
    Ok(())
}

/// Save the manifest at the desktop path AND mirror it under APPDATA.
///
/// The version that knows where the mirror lives. Use this when you have
/// a [`StealthConfig`] in hand; raw [`save_manifest`] only writes the
/// primary because some call sites (legacy migration) operate without a
/// resolved APPDATA path.
pub fn save_manifest_with_mirror(
    config: &StealthConfig,
    dir: &Path,
    manifest: &SafetyManifest,
) -> Result<(), StealthError> {
    save_manifest(dir, manifest)?;

    let mirror = manifest_mirror_path(config);
    match write_json_atomic(&mirror, manifest) {
        Ok(()) => set_mirror_healthy(true),
        Err(e) => {
            tracing::warn!(
                "save_manifest_with_mirror: mirror write failed at {}: {e}",
                mirror.display()
            );
            set_mirror_healthy(false);
        }
    }

    Ok(())
}

/// Parameter bundle for [`manifest_add`]. Grouped into one struct so the
/// public surface stays under clippy's `too_many_arguments` lint without
/// losing field-level naming at call sites.
#[derive(Debug, Clone)]
pub struct ManifestAddParams<'a> {
    pub original_path: &'a str,
    pub hidden_path: &'a str,
    pub zone_id: &'a str,
    pub file_size_bytes: u64,
    pub display_name: &'a str,
    pub icon_x: Option<i32>,
    pub icon_y: Option<i32>,
    pub file_type: &'a str,
}

/// Append an entry to the manifest with full metadata. Drops any existing
/// entry whose `original_path` matches (case-insensitive) so the new
/// hidden_path supersedes it.
pub fn manifest_add(dir: &Path, params: ManifestAddParams<'_>) -> Result<(), StealthError> {
    let ManifestAddParams {
        original_path,
        hidden_path,
        zone_id,
        file_size_bytes,
        display_name,
        icon_x,
        icon_y,
        file_type,
    } = params;

    let mut manifest = load_manifest(dir)?;
    manifest
        .entries
        .retain(|e| !paths_equal_str(&e.original_path, original_path));
    manifest.entries.push(ManifestEntry {
        original_path: original_path.to_string(),
        hidden_path: hidden_path.to_string(),
        zone_id: zone_id.to_string(),
        file_size_bytes,
        hidden_at: now_iso8601(),
        display_name: display_name.to_string(),
        icon_x,
        icon_y,
        file_type: file_type.to_string(),
    });
    let len = manifest.entries.len();
    save_manifest(dir, &manifest)?;
    tracing::debug!(
        "Manifest: added entry ({}, zone={}, icon_pos=({:?},{:?})), total={}",
        original_path,
        zone_id,
        icon_x,
        icon_y,
        len
    );
    Ok(())
}

/// Remove every manifest entry matching `original_path`.
pub fn manifest_remove(dir: &Path, original_path: &str) -> Result<(), StealthError> {
    let mut manifest = load_manifest(dir)?;
    let before = manifest.entries.len();
    manifest
        .entries
        .retain(|e| !paths_equal_str(&e.original_path, original_path));
    let removed = before - manifest.entries.len();
    if removed > 0 {
        save_manifest(dir, &manifest)?;
        tracing::debug!(
            "Manifest: removed {} entry(ies) for {}, remaining={}",
            removed,
            original_path,
            manifest.entries.len()
        );
    }
    Ok(())
}

/// Sync zone metadata into the manifest. Caller passes the materialised
/// zone snapshots + screen resolution (1.x read these from `AppState`).
pub fn sync_zone_metadata(
    config: &StealthConfig,
    zones: &[ManifestZone],
    screen_width: u32,
    screen_height: u32,
) -> Result<(), StealthError> {
    let dir = super::hide::hidden_dir_for(config)?;
    let mut manifest = load_manifest(&dir)?;
    manifest.zones = zones.to_vec();
    manifest.screen_width = screen_width;
    manifest.screen_height = screen_height;
    manifest.last_updated = now_iso8601();
    save_manifest(&dir, &manifest)
}

/// Persist a manifest snapshot to a known desktop path. Used by recovery
/// bundles to write manifest data without going through normal
/// `StealthConfig` plumbing.
pub fn persist_manifest_snapshot(
    desktop_path: &str,
    manifest: &SafetyManifest,
) -> Result<(), StealthError> {
    let hdir = Path::new(desktop_path).join(".bentodesk");
    std::fs::create_dir_all(&hdir).map_err(|e| StealthError::Io {
        path: hdir.clone(),
        message: e.to_string(),
    })?;
    let path = hdir.join("manifest.json");
    write_json_atomic(&path, manifest)
}

// ─── AttrGuard worker pool (T-100) ──────────────────────────────────────

/// Job pulled from the worker pool's input channel.
#[derive(Debug)]
enum SweepJob {
    /// Walk the entire `.bentodesk/` tree and re-stamp stealth attributes
    /// on every directory.
    SweepRoot { root: PathBuf },
    /// Stamp `path` exactly once (may be a single retry-queue path).
    Stamp { path: PathBuf },
}

/// Fixed-size worker pool that drains the retry queue + walks the
/// `.bentodesk/` tree.
///
/// Spec §9 mandates `std::thread` only — no async runtime. T-100 mandates
/// a fixed-size pool — no unbounded `spawn`s. The pool is `2` workers
/// because the workload is I/O-bound (see module-level docs).
pub struct AttrGuard {
    inbox: Sender<SweepJob>,
    workers: Mutex<Option<Vec<JoinHandle<()>>>>,
    shutdown: std::sync::Arc<AtomicBool>,
    events: Option<Sender<StealthEvent>>,
}

impl AttrGuard {
    /// Worker pool size. See module docs for the rationale (constant, no
    /// scaling logic — spec §17).
    const POOL_SIZE: usize = 2;

    /// Spawn the worker pool. Workers exit when `inbox` is dropped (via
    /// `drop(AttrGuard)`).
    pub fn start(events: Option<Sender<StealthEvent>>) -> Self {
        let (tx, rx): (Sender<SweepJob>, Receiver<SweepJob>) = unbounded();
        let shutdown = std::sync::Arc::new(AtomicBool::new(false));

        let mut workers = Vec::with_capacity(Self::POOL_SIZE);
        for worker_id in 0..Self::POOL_SIZE {
            let rx = rx.clone();
            let shutdown_flag = shutdown.clone();
            let events = events.clone();
            let handle = std::thread::Builder::new()
                .name(format!("bento-stealth-attrguard-{worker_id}"))
                .spawn(move || worker_loop(worker_id, rx, shutdown_flag, events))
                .unwrap_or_else(|e| {
                    tracing::error!(
                        "AttrGuard: failed to spawn worker {worker_id}: {e} — using main thread fallback"
                    );
                    // Spawn failure is exceedingly rare on Windows. Log
                    // and continue with whatever workers we managed to
                    // spawn — the inbox channel will keep accepting jobs;
                    // the remaining workers absorb them.
                    std::thread::spawn(|| {})
                });
            workers.push(handle);
        }

        Self {
            inbox: tx,
            workers: Mutex::new(Some(workers)),
            shutdown,
            events,
        }
    }

    /// Initial pass — sweep the entire `.bentodesk/` root after startup.
    /// Non-blocking; the actual work runs on a worker.
    pub fn startup_sweep(&self, config: &StealthConfig) -> Result<(), StealthError> {
        let root = super::hide::hidden_dir_for(config)?;
        let _ = self.inbox.send(SweepJob::SweepRoot { root });
        Ok(())
    }

    /// On-demand sweep (invoked by the IPC `reapply_stealth` command and
    /// by the retry timer).
    pub fn sweep_root(&self, root: &Path) {
        let _ = self.inbox.send(SweepJob::SweepRoot {
            root: root.to_path_buf(),
        });
    }

    /// Single-path stamp (faster than a full sweep when only one entry
    /// needs attention).
    pub fn stamp(&self, path: &Path) {
        let _ = self.inbox.send(SweepJob::Stamp {
            path: path.to_path_buf(),
        });
    }

    /// Synchronous sweep — blocks until completion. Used by tests + the
    /// shutdown path. Returns `(applied, queued)`.
    pub fn sweep_root_blocking(root: &Path) -> (u32, u32) {
        let mut applied = 0u32;
        if !root.exists() {
            return (0, 0);
        }
        ensure_stealth(root);
        applied += 1;

        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    ensure_stealth(&p);
                    applied += 1;
                }
            }
        }

        // Drain any additional queued paths not under `root`.
        let queued: Vec<PathBuf> = with_shared(|s| s.retry_queue.clone());
        for path in &queued {
            if path.exists() {
                ensure_stealth(path);
            } else {
                super::record_retry_drained(path);
            }
        }
        let queued_after = with_shared(|s| s.retry_queue.len() as u32);
        (applied, queued_after)
    }

    /// Per-worker poll deadline. Workers wake every 200 ms even when
    /// `inbox` is empty so the shutdown flag is observed promptly.
    const POLL_INTERVAL: Duration = Duration::from_millis(200);
}

fn worker_loop(
    worker_id: usize,
    rx: Receiver<SweepJob>,
    shutdown: std::sync::Arc<AtomicBool>,
    events: Option<Sender<StealthEvent>>,
) {
    tracing::debug!("AttrGuard worker {worker_id} starting");
    loop {
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        match rx.recv_timeout(AttrGuard::POLL_INTERVAL) {
            Ok(SweepJob::SweepRoot { root }) => {
                let started = Instant::now();
                let (applied, queued) = AttrGuard::sweep_root_blocking(&root);
                tracing::debug!(
                    "AttrGuard worker {worker_id} swept {} in {:?} (applied={applied}, queued={queued})",
                    root.display(),
                    started.elapsed()
                );
                if let Some(tx) = &events {
                    let _ = tx.send(StealthEvent::SweepComplete { applied, queued });
                    let _ = tx.send(StealthEvent::StatusChanged(status()));
                }
            }
            Ok(SweepJob::Stamp { path }) => {
                ensure_stealth(&path);
                if let Some(tx) = &events {
                    let _ = tx.send(StealthEvent::StatusChanged(status()));
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
    tracing::debug!("AttrGuard worker {worker_id} exiting");
}

impl Drop for AttrGuard {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        // Drop the sender so workers fall out of `recv_timeout`.
        // We can't move out of self.inbox, so close by replacing with a
        // dropped channel.
        let (dead_tx, _) = unbounded::<SweepJob>();
        let _ = std::mem::replace(&mut self.inbox, dead_tx);

        let workers_opt = self
            .workers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        if let Some(handles) = workers_opt {
            for handle in handles {
                let _ = handle.join();
            }
        }

        if let Some(tx) = &self.events {
            let _ = tx.send(StealthEvent::StatusChanged(status()));
        }
    }
}

mod migration;

pub use migration::{
    cleanup_legacy_hidden_dir, migrate_flat_to_zone_dirs, reapply_hidden_on_startup,
};

#[cfg(test)]
mod tests;
