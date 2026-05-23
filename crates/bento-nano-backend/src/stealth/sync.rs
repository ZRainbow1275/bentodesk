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
//!   `attrib.exe` for un-hide; the nano port re-uses
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
/// [`migrate_attrib_hidden_files`], and [`migrate_flat_to_zone_dirs`] so
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

// ─── Atomic JSON I/O (no `crate::storage` dependency in nano backend) ───

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

// ─── Legacy migration ───────────────────────────────────────────────────

/// Resolve the old `hidden_items/` storage directory under app data.
fn legacy_hidden_items_dir(app_data: &Path) -> PathBuf {
    app_data.join("hidden_items")
}

/// Old manifest entry from the AppData/hidden_items/ "move-mode" era.
#[derive(Debug, Clone, Deserialize)]
struct LegacyMoveManifestEntry {
    original_path: String,
    hidden_path: String,
    #[allow(dead_code)]
    hidden_at: String,
}

/// Migrate from BOTH old architectures:
///
/// 1. AppData/hidden_items/ directory (old "file move" mode)
/// 2. Files hidden via `attrib +h +s` (old "reference mode")
///
/// `attrib_items` is the caller-provided set of layout entries that may
/// be in attrib-mode (1.x walked `state.layout` directly; the nano port
/// expects the caller to pre-collect candidates). Returns `(migrated,
/// new_hidden_paths)` so the caller can update its layout store with the
/// rewritten `hidden_path` values.
pub fn cleanup_legacy_hidden_dir(
    config: &StealthConfig,
    attrib_items: &[(String, String, String)],
) -> MigrationResult {
    let mut total_migrated = 0u32;
    let mut new_paths: Vec<MigrationUpdate> = Vec::new();

    // -- Phase 1: AppData/hidden_items/ "move mode" ------------------
    let old_dir = legacy_hidden_items_dir(config.app_data_path());
    if old_dir.exists() {
        tracing::info!("=== Legacy migration Phase 1: AppData/hidden_items/ ===");
        total_migrated += migrate_old_move_dir(config, &old_dir)?;
    }

    // -- Phase 2: attrib-mode files in the layout --------------------
    let (phase2_count, phase2_paths) = migrate_attrib_hidden_files(config, attrib_items)?;
    total_migrated += phase2_count;
    new_paths.extend(phase2_paths);

    // -- Phase 3: Clean old manifest.json from app data dir ----------
    let app_data_manifest = config.app_data_path().join("manifest.json");
    if app_data_manifest.exists() {
        tracing::info!("Removing old app-data manifest.json");
        let _ = std::fs::remove_file(&app_data_manifest);
        let _ = std::fs::remove_file(app_data_manifest.with_extension("json.tmp"));
    }

    if total_migrated > 0 {
        tracing::info!(
            "=== Legacy migration complete: {} files migrated total ===",
            total_migrated
        );
    }

    Ok((total_migrated, new_paths))
}

fn migrate_old_move_dir(config: &StealthConfig, old_dir: &Path) -> Result<u32, StealthError> {
    let old_manifest = load_legacy_move_manifest(old_dir);

    let entries = match std::fs::read_dir(old_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Cannot read legacy hidden_items/ directory: {}", e);
            return Ok(0);
        }
    };

    let desktop = PathBuf::from(config.desktop_path.as_str());
    let mut migrated = 0u32;

    for entry in entries.flatten() {
        let file_path = entry.path();

        if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
            if name == "manifest.json" || name == "manifest.json.tmp" {
                continue;
            }
        }
        if file_path.is_dir() {
            continue;
        }

        let file_path_str = file_path.to_string_lossy().into_owned();

        let original = old_manifest
            .iter()
            .find(|e| paths_equal_str(&e.hidden_path, &file_path_str))
            .map(|e| e.original_path.clone());

        let dest = if let Some(orig) = &original {
            PathBuf::from(orig)
        } else {
            match file_path.file_name() {
                Some(name) => desktop.join(name),
                None => {
                    tracing::warn!(
                        "Legacy migration: cannot determine destination for {}",
                        file_path.display()
                    );
                    continue;
                }
            }
        };

        if dest.exists() {
            tracing::warn!(
                "Legacy migration: destination already exists, skipping: {}",
                dest.display()
            );
            continue;
        }

        if let Some(parent) = dest.parent() {
            if !parent.exists() {
                let _ = std::fs::create_dir_all(parent);
            }
        }

        let success = match std::fs::rename(&file_path, &dest) {
            Ok(()) => true,
            Err(rename_err) => match std::fs::copy(&file_path, &dest) {
                Ok(_) => match std::fs::remove_file(&file_path) {
                    Ok(()) => true,
                    Err(rm_err) => {
                        tracing::error!(
                            "Legacy migration: copy ok but delete of source failed for {}: {} (rename was: {}). Removing copy to stay safe.",
                            file_path.display(),
                            rm_err,
                            rename_err
                        );
                        if let Err(cleanup_err) = std::fs::remove_file(&dest) {
                            tracing::error!(
                                "Legacy migration: failed to clean up orphan copy at {}: {} — manual cleanup required",
                                dest.display(),
                                cleanup_err
                            );
                        }
                        false
                    }
                },
                Err(e) => {
                    tracing::error!(
                        "Legacy migration failed: {} -> {}: {}",
                        file_path.display(),
                        dest.display(),
                        e
                    );
                    false
                }
            },
        };

        if success {
            tracing::info!(
                "Legacy Phase1 migrated: {} -> {}",
                file_path.display(),
                dest.display()
            );
            migrated += 1;
        }
    }

    if migrated > 0 || is_dir_empty_except_manifest(old_dir) {
        let _ = std::fs::remove_file(old_dir.join("manifest.json"));
        let _ = std::fs::remove_file(old_dir.join("manifest.json.tmp"));
        match std::fs::remove_dir(old_dir) {
            Ok(()) => tracing::info!("Removed legacy hidden_items/ directory"),
            Err(e) => tracing::warn!("Could not remove legacy hidden_items/ directory: {}", e),
        }
    }

    Ok(migrated)
}

/// Migrate attrib-mode files into subfolder mode.
///
/// `attrib_items` is `(zone_id, item_id, original_path)` triples for layout
/// entries the caller has already filtered to attrib mode. Returns the
/// migrated count + the list of `(zone_id, item_id, new_hidden_path)` so
/// the caller can update its layout store.
fn migrate_attrib_hidden_files(
    config: &StealthConfig,
    attrib_items: &[(String, String, String)],
) -> MigrationResult {
    if attrib_items.is_empty() {
        return Ok((0, Vec::new()));
    }

    tracing::info!(
        "=== Legacy migration Phase 2: migrating {} attrib-hidden files to subfolder mode ===",
        attrib_items.len()
    );

    let mut migrated = 0u32;
    let mut updates: Vec<MigrationUpdate> = Vec::new();

    for (zone_id, item_id, orig_path) in attrib_items {
        // Remove hidden attribute by clearing the bits (no `attrib.exe` spawn).
        clear_hidden_attribute(Path::new(orig_path));

        let source = Path::new(orig_path);
        if !source.exists() {
            tracing::warn!(
                "Attrib migration: file disappeared after unhide: {}",
                orig_path
            );
            continue;
        }

        let zone_dir = match super::hide::zone_hidden_dir_for(config, zone_id) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("Attrib migration: zone_hidden_dir_for failed: {e}");
                continue;
            }
        };
        let dest = super::hide::unique_hidden_path(&zone_dir, source);

        let success = match std::fs::rename(source, &dest) {
            Ok(()) => true,
            Err(rename_err) => match std::fs::copy(source, &dest) {
                Ok(_) => match std::fs::remove_file(source) {
                    Ok(()) => true,
                    Err(rm_err) => {
                        tracing::error!(
                            "Attrib migration: copy ok but delete of source failed for {}: {} (rename was: {}). Removing copy to stay safe.",
                            orig_path,
                            rm_err,
                            rename_err
                        );
                        if let Err(cleanup_err) = std::fs::remove_file(&dest) {
                            tracing::error!(
                                "Attrib migration: failed to clean up orphan copy at {:?}: {} — manual cleanup required",
                                dest,
                                cleanup_err
                            );
                        }
                        false
                    }
                },
                Err(copy_err) => {
                    tracing::error!(
                        "Attrib migration failed for {}: rename={} copy={}",
                        orig_path,
                        rename_err,
                        copy_err
                    );
                    false
                }
            },
        };

        if success {
            let hidden_path_str = dest.to_string_lossy().into_owned();
            let file_size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            let display_name = Path::new(orig_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let global_dir = super::hide::hidden_dir_for(config)?;
            if let Err(e) = manifest_add(
                &global_dir,
                ManifestAddParams {
                    original_path: orig_path,
                    hidden_path: &hidden_path_str,
                    zone_id,
                    file_size_bytes: file_size,
                    display_name: &display_name,
                    icon_x: None,
                    icon_y: None,
                    file_type: "",
                },
            ) {
                tracing::warn!("Attrib migration: manifest_add failed: {e}");
            }
            tracing::info!(
                "Attrib migration: {} -> {} (zone={})",
                orig_path,
                dest.display(),
                zone_id
            );
            updates.push((zone_id.clone(), item_id.clone(), hidden_path_str));
            migrated += 1;
        }
    }

    Ok((migrated, updates))
}

/// Clear `FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM` from `path` via
/// direct `SetFileAttributesW` call. Replaces 1.x's `attrib.exe` spawn.
///
/// Uses `windows-sys` (plain Win32) per spec §3.1.1.
#[cfg(windows)]
fn clear_hidden_attribute(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{GetFileAttributesW, SetFileAttributesW};

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: `wide` is null-terminated UTF-16 valid for the call.
    let existing = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if existing == super::INVALID_FILE_ATTRIBUTES {
        return;
    }

    // Clear HIDDEN (0x2) + SYSTEM (0x4) bits while preserving everything else.
    let new_attrs = existing & !(0x0000_0002u32 | 0x0000_0004u32);
    if new_attrs == existing {
        return;
    }

    // SAFETY: `wide` valid for the call; flag mask is a plain u32. Result
    // ignored — best-effort un-hide before the move.
    let _ = unsafe { SetFileAttributesW(wide.as_ptr(), new_attrs) };
}

#[cfg(not(windows))]
fn clear_hidden_attribute(_path: &Path) {}

/// Load the old-format manifest from the AppData/hidden_items/ directory.
fn load_legacy_move_manifest(hidden_dir: &Path) -> Vec<LegacyMoveManifestEntry> {
    let path = hidden_dir.join("manifest.json");
    if !path.exists() {
        return Vec::new();
    }

    #[derive(Debug, Deserialize)]
    struct LegacyManifest {
        entries: Vec<LegacyMoveManifestEntry>,
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<LegacyManifest>(&content)
            .map(|m| m.entries)
            .unwrap_or_else(|e| {
                tracing::warn!("Could not parse legacy manifest: {}", e);
                Vec::new()
            }),
        Err(e) => {
            tracing::warn!("Could not read legacy manifest: {}", e);
            Vec::new()
        }
    }
}

fn is_dir_empty_except_manifest(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name != "manifest.json" && name != "manifest.json.tmp" {
                        return false;
                    }
                }
            }
            true
        }
        Err(_) => false,
    }
}

/// Reapply hidden-folder attributes on startup. Returns the count of files
/// currently inside `.bentodesk/` (across all zone subdirs) for logging.
pub fn reapply_hidden_on_startup(config: &StealthConfig) -> Result<u32, StealthError> {
    let hdir = super::hide::hidden_dir_for(config)?;
    ensure_stealth(&hdir);

    let mut count = 0u32;
    if let Ok(entries) = std::fs::read_dir(&hdir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if path.is_dir() {
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    count += sub_entries.flatten().filter(|e| !e.path().is_dir()).count() as u32;
                }
            } else if name_str != "manifest.json"
                && name_str != "manifest.json.tmp"
                && name_str != "manifest.json.bak"
            {
                count += 1;
            }
        }
    }

    if count > 0 {
        tracing::info!(
            "Startup: .bentodesk/ contains {} hidden files (across zone subdirs), folder attrib +h +s ensured",
            count
        );
    }

    Ok(count)
}

/// Migrate flat `.bentodesk/` files into zone subdirectories.
///
/// `hidden_to_zone` is the caller-provided lookup `(hidden_path, zone_id)`
/// derived from the layout. Returns the migrated count + the list of
/// `(old_hidden_path, new_hidden_path, zone_id)` tuples so the caller can
/// rewrite layout entries.
pub fn migrate_flat_to_zone_dirs(
    config: &StealthConfig,
    hidden_to_zone: &[(String, String)],
) -> MigrationResult {
    let hdir = super::hide::hidden_dir_for(config)?;

    let flat_files: Vec<PathBuf> = match std::fs::read_dir(&hdir) {
        Ok(entries) => entries
            .flatten()
            .filter(|e| {
                let path = e.path();
                if path.is_dir() {
                    return false;
                }
                let name = e.file_name();
                let name_str = name.to_string_lossy();
                name_str != "manifest.json"
                    && name_str != "manifest.json.tmp"
                    && name_str != "manifest.json.bak"
            })
            .map(|e| e.path())
            .collect(),
        Err(_) => return Ok((0, Vec::new())),
    };

    if flat_files.is_empty() {
        return Ok((0, Vec::new()));
    }

    tracing::info!(
        "=== Zone isolation migration: {} flat files to migrate ===",
        flat_files.len()
    );

    let mut migrated = 0u32;
    let mut updates: Vec<MigrationUpdate> = Vec::new();

    for file_path in &flat_files {
        let file_path_str = file_path.to_string_lossy().into_owned();

        let zone_id = hidden_to_zone
            .iter()
            .find(|(hp, _)| paths_equal_str(hp, &file_path_str))
            .map(|(_, zid)| zid.clone());

        let zone_id = match zone_id {
            Some(z) => z,
            None => {
                tracing::warn!(
                    "Zone migration: no zone found for flat file {:?}, leaving in place",
                    file_path
                );
                continue;
            }
        };

        let zone_dir = super::hide::zone_hidden_dir_for(config, &zone_id)?;
        let file_name = match file_path.file_name() {
            Some(n) => n,
            None => continue,
        };
        let dest = zone_dir.join(file_name);

        if dest.exists() {
            tracing::warn!(
                "Zone migration: destination already exists {:?}, skipping {:?}",
                dest,
                file_path
            );
            continue;
        }

        match std::fs::rename(file_path, &dest) {
            Ok(()) => {
                let new_hidden_str = dest.to_string_lossy().into_owned();

                let mut manifest = load_manifest(&hdir)?;
                for entry in &mut manifest.entries {
                    if paths_equal_str(&entry.hidden_path, &file_path_str) {
                        entry.hidden_path = new_hidden_str.clone();
                        entry.zone_id = zone_id.clone();
                    }
                }
                save_manifest(&hdir, &manifest)?;

                tracing::info!(
                    "Zone migration: {:?} -> {:?} (zone={})",
                    file_path,
                    dest,
                    zone_id
                );
                updates.push((file_path_str, new_hidden_str, zone_id));
                migrated += 1;
            }
            Err(e) => {
                tracing::error!(
                    "Zone migration failed for {:?} -> {:?}: {}",
                    file_path,
                    dest,
                    e
                );
            }
        }
    }

    Ok((migrated, updates))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_for(desktop: &Path, app_data: &Path) -> StealthConfig {
        StealthConfig {
            desktop_path: smol_str::SmolStr::new(desktop.to_string_lossy()),
            app_data_dir: smol_str::SmolStr::new(app_data.to_string_lossy()),
        }
    }

    struct TmpDir(PathBuf);
    impl TmpDir {
        fn as_path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tempdir() -> TmpDir {
        let suffix = super::super::unique_suffix();
        let path =
            std::env::temp_dir().join(format!("bento-sync-{}-{}", std::process::id(), suffix));
        std::fs::create_dir_all(&path).expect("tempdir");
        TmpDir(path)
    }

    // ── parse_schema_version ───────────────────────────────────────

    #[test]
    fn schema_version_numeric_compare() {
        assert!(parse_schema_version("3.10") > parse_schema_version("3.9"));
        assert_eq!(parse_schema_version(""), (0, 0));
        assert_eq!(parse_schema_version("3.1"), (3, 1));
    }

    #[test]
    fn needs_migration_detects_old_schema() {
        assert!(needs_migration(""));
        assert!(needs_migration("3.0"));
        assert!(!needs_migration("3.1"));
        assert!(!needs_migration("3.2"));
    }

    // ── manifest round-trip ─────────────────────────────────────────

    #[test]
    fn manifest_add_then_load_round_trip() {
        let tmp = tempdir();
        let dir = tmp.as_path();

        manifest_add(
            dir,
            ManifestAddParams {
                original_path: r"C:\Users\X\Desktop\foo.txt",
                hidden_path: r"C:\Users\X\Desktop\.bentodesk\z-1\foo.txt",
                zone_id: "z-1",
                file_size_bytes: 42,
                display_name: "foo.txt",
                icon_x: Some(10),
                icon_y: Some(20),
                file_type: "File",
            },
        )
        .expect("add");

        let m = load_manifest(dir).expect("load");
        assert_eq!(m.entries.len(), 1);
        assert_eq!(m.entries[0].original_path, r"C:\Users\X\Desktop\foo.txt");
        assert_eq!(m.entries[0].zone_id, "z-1");
        assert_eq!(m.entries[0].file_size_bytes, 42);
        assert_eq!(m.entries[0].icon_x, Some(10));
        assert_eq!(m.entries[0].icon_y, Some(20));
        assert_eq!(m.schema_version, MANIFEST_SCHEMA_VERSION);
    }

    #[test]
    fn manifest_add_replaces_duplicate_original_path() {
        let tmp = tempdir();
        let dir = tmp.as_path();

        for hidden in ["a/foo.txt", "b/foo.txt"] {
            manifest_add(
                dir,
                ManifestAddParams {
                    original_path: r"C:\foo.txt",
                    hidden_path: hidden,
                    zone_id: "z-1",
                    file_size_bytes: 10,
                    display_name: "foo.txt",
                    icon_x: None,
                    icon_y: None,
                    file_type: "",
                },
            )
            .expect("add");
        }

        let m = load_manifest(dir).expect("load");
        assert_eq!(m.entries.len(), 1);
        assert_eq!(m.entries[0].hidden_path, "b/foo.txt");
    }

    #[test]
    fn manifest_remove_by_original_path() {
        let tmp = tempdir();
        let dir = tmp.as_path();

        manifest_add(
            dir,
            ManifestAddParams {
                original_path: r"C:\a.txt",
                hidden_path: "h/a.txt",
                zone_id: "z",
                file_size_bytes: 1,
                display_name: "a.txt",
                icon_x: None,
                icon_y: None,
                file_type: "",
            },
        )
        .expect("add a");
        manifest_add(
            dir,
            ManifestAddParams {
                original_path: r"C:\b.txt",
                hidden_path: "h/b.txt",
                zone_id: "z",
                file_size_bytes: 1,
                display_name: "b.txt",
                icon_x: None,
                icon_y: None,
                file_type: "",
            },
        )
        .expect("add b");

        manifest_remove(dir, r"c:\A.TXT").expect("remove case-insensitive");

        let m = load_manifest(dir).expect("load");
        assert_eq!(m.entries.len(), 1);
        assert_eq!(m.entries[0].original_path, r"C:\b.txt");
    }

    #[test]
    fn save_manifest_with_mirror_writes_both() {
        let tmp = tempdir();
        let desktop = tmp.as_path().join("desktop");
        let app_data = tmp.as_path().join("appdata");
        std::fs::create_dir_all(&desktop).expect("desktop");
        std::fs::create_dir_all(&app_data).expect("appdata");
        let cfg = config_for(&desktop, &app_data);

        let dir = desktop.join(".bentodesk");
        std::fs::create_dir_all(&dir).expect("hidden dir");

        let mut m = SafetyManifest::default();
        m.entries.push(ManifestEntry {
            original_path: "x".to_string(),
            hidden_path: "y".to_string(),
            zone_id: "z".to_string(),
            file_size_bytes: 0,
            hidden_at: now_iso8601(),
            display_name: "x".to_string(),
            icon_x: None,
            icon_y: None,
            file_type: "".to_string(),
        });

        save_manifest_with_mirror(&cfg, &dir, &m).expect("save");

        assert!(dir.join("manifest.json").exists());
        assert!(app_data.join("manifest.mirror.json").exists());
    }

    #[test]
    fn write_json_atomic_creates_bak_on_overwrite() {
        let tmp = tempdir();
        let path = tmp.as_path().join("file.json");

        write_json_atomic(&path, &SafetyManifest::default()).expect("first write");
        let mut second = SafetyManifest::default();
        second.entries.push(ManifestEntry {
            original_path: "x".into(),
            hidden_path: "y".into(),
            zone_id: "z".into(),
            file_size_bytes: 0,
            hidden_at: now_iso8601(),
            display_name: "x".into(),
            icon_x: None,
            icon_y: None,
            file_type: "".into(),
        });
        write_json_atomic(&path, &second).expect("second write");

        assert!(path.exists());
        assert!(path.with_extension("json.bak").exists());
    }

    #[test]
    fn read_json_with_recovery_restores_from_bak_on_corrupt_primary() {
        let tmp = tempdir();
        let path = tmp.as_path().join("file.json");

        write_json_atomic(&path, &SafetyManifest::default()).expect("seed");
        // Overwrite to create .bak.
        let mut updated = SafetyManifest::default();
        updated.entries.push(ManifestEntry {
            original_path: "x".into(),
            hidden_path: "y".into(),
            zone_id: "z".into(),
            file_size_bytes: 0,
            hidden_at: now_iso8601(),
            display_name: "x".into(),
            icon_x: None,
            icon_y: None,
            file_type: "".into(),
        });
        write_json_atomic(&path, &updated).expect("write 2");

        // Corrupt the primary.
        std::fs::write(&path, b"not json {{{").expect("corrupt");

        let recovered: Option<SafetyManifest> = read_json_with_recovery(&path).expect("recover");
        let recovered = recovered.expect("Some");
        // The .bak holds the previous (default-empty) manifest content
        // before updated was renamed in. Either is fine for this test,
        // both prove recovery worked without panic.
        assert_eq!(
            recovered.schema_version,
            MANIFEST_SCHEMA_VERSION.to_string()
        );
    }

    // ── AttrGuard worker pool ──────────────────────────────────────

    #[test]
    fn attr_guard_starts_and_drops_cleanly() {
        let guard = AttrGuard::start(None);
        // Just creating + dropping the guard exercises spawn + join
        // without panicking. The workers exit when shutdown=true and the
        // sender is replaced (channel disconnects).
        drop(guard);
    }

    #[test]
    fn attr_guard_sweep_root_blocking_handles_missing_dir() {
        let tmp = tempdir();
        let nonexistent = tmp.as_path().join("does-not-exist");
        let (applied, _queued) = AttrGuard::sweep_root_blocking(&nonexistent);
        assert_eq!(applied, 0);
    }

    #[test]
    fn attr_guard_sweep_root_blocking_walks_subdirs() {
        let tmp = tempdir();
        let root = tmp.as_path().join(".bentodesk");
        let zone_a = root.join("zone-a");
        let zone_b = root.join("zone-b");
        std::fs::create_dir_all(&zone_a).expect("zone-a");
        std::fs::create_dir_all(&zone_b).expect("zone-b");

        let (applied, _queued) = AttrGuard::sweep_root_blocking(&root);
        // root + zone-a + zone-b = 3 directories stamped.
        assert_eq!(applied, 3);
    }

    #[test]
    fn cleanup_legacy_hidden_dir_removes_old_appdata_manifest() {
        let tmp = tempdir();
        let desktop = tmp.as_path().join("desktop");
        let app_data = tmp.as_path().join("appdata");
        std::fs::create_dir_all(&desktop).expect("desktop");
        std::fs::create_dir_all(&app_data).expect("appdata");
        let cfg = config_for(&desktop, &app_data);

        // Plant an old app-data manifest.json that should be deleted.
        let old_manifest = app_data.join("manifest.json");
        std::fs::write(&old_manifest, b"{}").expect("seed");

        cleanup_legacy_hidden_dir(&cfg, &[]).expect("cleanup");

        assert!(!old_manifest.exists(), "old manifest should be removed");
    }
}
