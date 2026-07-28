//! T-091 — selected-stack updater runtime surface.
//!
//! Per the 2026-05-03 team-lead Q3 ruling (`feedback_compiles_clean_stub_during_multi_agent_coord.md`,
//! sanctioned), the initial updater slice shipped IPC scaffolding first. The
//! selected-stack manifest check is now implemented with a minimal WinHTTP/file
//! loader; local/file and remote HTTP(S) artifact staged downloads are real;
//! local NSIS installer launch is real; artifact SHA-256 integrity verification
//! is real when a manifest supplies a digest; manifests that carry a
//! Tauri/minisign signature are verified against the embedded BentoDesk public
//! key before the staged artifact becomes installable.
//!
//! ## What is implemented today
//!
//! - [`UpdateInfo`] DTO (serde-derived) — same shape as 1.x.
//! - [`UpdateEvent`] enum — replaces 1.x's named-string `update:available` /
//!   `update:progress` / `update:ready` / `update:error` events; emitted on a
//!   `crossbeam_channel::Sender<UpdateEvent>` instead of `app.emit(name, payload)`.
//! - [`UpdaterError`] hand-rolled enum (spec §8.1) including load-bearing
//!   visible errors for invalid manifests, failed downloads, and failed
//!   authenticity/integrity checks.
//! - [`Updater`] struct with public `check()` / `download()` / `install()` /
//!   `skip_version()` / `current_skipped()` entry points so callers compile
//!   against the final v2.x surface.
//! - Manifest check through `BENTODESK_UPDATE_MANIFEST_URL`, supporting
//!   `http://`, `https://`, `file://`, and plain filesystem paths. The parser
//!   accepts both selected-stack flat artifact fields and Tauri v2 static
//!   `platforms.windows-x86_64` artifact entries.
//! - Local/file and remote HTTP(S) `.exe` artifact staging plus optional
//!   manifest-supplied SHA-256 artifact verification before the staged artifact
//!   becomes installable. If the manifest also carries a signature, the staged
//!   artifact is streamed through minisign verification using the embedded
//!   BentoDesk updater public key.
//! - NSIS `/S` launch for the install step. The shell quits after a successful
//!   launch so the installer can replace files.
//! - Background one-shot and recurring automatic checks, using the same
//!   manifest check path and typed event channel as visible checks.
//! - [`check_interval_hours`] preserved verbatim — it's pure-logic settings
//!   resolution for the selected-stack scheduler.

pub mod event;

use crossbeam_channel::Sender;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(windows)]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_ALG_HANDLE, BCRYPT_HASH_HANDLE, BCRYPT_SHA256_ALGORITHM, BCryptCloseAlgorithmProvider,
    BCryptCreateHash, BCryptDestroyHash, BCryptFinishHash, BCryptHashData,
    BCryptOpenAlgorithmProvider,
};

pub use event::{UpdateEvent, UpdateProgress};

const MANIFEST_ENV: &str = "BENTODESK_UPDATE_MANIFEST_URL";
const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;
const SHA256_DIGEST_BYTES: usize = 32;
const SHA256_HEX_CHARS: usize = SHA256_DIGEST_BYTES * 2;
const TAURI_WINDOWS_X64_PLATFORM: &str = "windows-x86_64";
const BENTODESK_UPDATE_MINISIGN_PUBLIC_KEY: &str = "untrusted comment: minisign public key: 9570FD4569459FCC\n\
     RWTMn0VpRf1wlZ03vKFJk4OIlzHm7l63qP5+rnDEy8lP8iBKQIbU/MaZ";

// ─── DTOs ────────────────────────────────────────────────────────────

/// Thin DTO returned to the UI when a new version is available.
///
/// Same shape as 1.x. `String` (not `SmolStr`) on the public-display fields
/// because changelog `body` text routinely exceeds 22 bytes; `version` and
/// `current_version` use `SmolStr` per spec §10 because they are short
/// `MAJOR.MINOR.PATCH` strings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: SmolStr,
    pub current_version: SmolStr,
    pub date: Option<SmolStr>,
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Update-check cadence the user picked in settings. 1.x used this enum
/// inside `AppSettings.updates.check_frequency`; native keeps the same
/// variant names so on-disk settings round-trip unchanged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateCheckFrequency {
    Daily,
    #[default]
    Weekly,
    Manual,
}

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ──────────────

/// Errors surfaced by the updater module.
#[derive(Debug)]
pub enum UpdaterError {
    /// Event channel send failed — receiver dropped. Mutation succeeded;
    /// only the notification was lost.
    EventChannelClosed,
    /// The configured manifest URL/path could not be fetched.
    FetchFailed(String),
    /// The configured manifest text was not a valid updater manifest.
    InvalidManifest(String),
    /// The staged installer could not be launched.
    InstallFailed(String),
    /// Artifact integrity verification failed before the installer became ready.
    VerificationFailed(String),
    /// The configured manifest source uses an unsupported URL scheme.
    UnsupportedManifestSource(String),
}

impl core::fmt::Display for UpdaterError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EventChannelClosed => f.write_str("updater event channel receiver dropped"),
            Self::FetchFailed(message) => write!(f, "updater manifest fetch failed: {message}"),
            Self::InvalidManifest(message) => write!(f, "invalid updater manifest: {message}"),
            Self::InstallFailed(message) => write!(f, "updater install failed: {message}"),
            Self::VerificationFailed(message) => {
                write!(f, "updater artifact verification failed: {message}")
            }
            Self::UnsupportedManifestSource(source) => write!(
                f,
                "unsupported updater manifest source '{source}' (expected http://, https://, file://, or a filesystem path)"
            ),
        }
    }
}

impl core::error::Error for UpdaterError {}

// ─── Public Updater surface ──────────────────────────────────────────

/// Updater driver. Holds the event channel and the user's skip preference.
///
/// Constructed once by `bentodesk-app::dispatcher` and held across the
/// process lifetime. The IPC `Command::Updater*` variants forward to the
/// `&self` methods here.
pub struct Updater {
    event_tx: Sender<UpdateEvent>,
    skipped_version: parking_lot_skipped_slot::SkipSlot,
    manifest_source: Option<SmolStr>,
    minisign_public_key: SmolStr,
    pending_update: Arc<Mutex<Option<UpdateInfo>>>,
    staged_artifact: Arc<Mutex<Option<PathBuf>>>,
}

impl Updater {
    /// Construct a fresh updater.
    pub fn new(event_tx: Sender<UpdateEvent>) -> Self {
        let manifest_source = std::env::var(MANIFEST_ENV)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(SmolStr::new);
        Self::with_manifest_source(event_tx, manifest_source)
    }

    /// Construct an updater with an explicit manifest source. Tests and
    /// controlled deployments use this to avoid hidden global environment
    /// state; production `new()` reads [`MANIFEST_ENV`].
    pub fn with_manifest_source(
        event_tx: Sender<UpdateEvent>,
        manifest_source: Option<SmolStr>,
    ) -> Self {
        Self::with_manifest_source_and_minisign_key(
            event_tx,
            manifest_source,
            SmolStr::new_static(BENTODESK_UPDATE_MINISIGN_PUBLIC_KEY),
        )
    }

    fn with_manifest_source_and_minisign_key(
        event_tx: Sender<UpdateEvent>,
        manifest_source: Option<SmolStr>,
        minisign_public_key: SmolStr,
    ) -> Self {
        Self {
            event_tx,
            skipped_version: parking_lot_skipped_slot::SkipSlot::new(),
            manifest_source,
            minisign_public_key,
            pending_update: Arc::new(Mutex::new(None)),
            staged_artifact: Arc::new(Mutex::new(None)),
        }
    }

    /// Check the configured endpoint for an available update.
    ///
    /// Returns `Ok(None)` when no manifest source is configured, when the
    /// manifest version is not newer than the current build, or when the user
    /// has skipped the manifest version.
    ///
    /// Network checks use WinHTTP for `http://` / `https://` sources. Local
    /// `file://` and plain path sources exist for internal channels and tests.
    pub fn check(&self) -> Result<Option<UpdateInfo>, UpdaterError> {
        let Some(manifest_text) = self.load_manifest_text()? else {
            tracing::info!(
                "updater::check: no {MANIFEST_ENV} configured; treating update channel as absent"
            );
            return Ok(None);
        };
        let current_version = pkg_version();
        let info = parse_update_manifest(&manifest_text, current_version.clone())?;
        if !version_is_newer(info.version.as_str(), current_version.as_str()) {
            *self
                .pending_update
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
            return Ok(None);
        }
        if self.current_skipped().as_deref() == Some(info.version.as_str()) {
            tracing::info!(
                "Update {} available but user asked to skip it",
                info.version
            );
            *self
                .pending_update
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
            return Ok(None);
        }
        *self
            .pending_update
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(info.clone());
        Ok(Some(info))
    }

    pub fn download(&self) -> Result<(), UpdaterError> {
        let info = {
            self.pending_update
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }
        .ok_or_else(|| UpdaterError::InvalidManifest("no pending update to download".to_owned()))?;
        let Some(source) = info.artifact_url.as_deref() else {
            let error =
                UpdaterError::InvalidManifest("manifest is missing artifact_url/url".to_owned());
            let _ = self.event_tx.send(UpdateEvent::Error {
                kind: SmolStr::new_static("download"),
                message: error.to_string(),
            });
            return Err(error);
        };
        if let Err(error) = validate_manifest_integrity_policy(&info) {
            let _ = self.event_tx.send(UpdateEvent::Error {
                kind: SmolStr::new_static("verify"),
                message: error.to_string(),
            });
            return Err(error);
        }
        let stage_path = staged_artifact_path(&info.version, source)?;
        *self
            .staged_artifact
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        copy_artifact_to_stage(source, &stage_path, &self.event_tx)?;
        if let Err(error) =
            verify_staged_artifact(&info, &stage_path, self.minisign_public_key.as_str())
        {
            let _ = std::fs::remove_file(&stage_path);
            let _ = self.event_tx.send(UpdateEvent::Error {
                kind: SmolStr::new_static("verify"),
                message: error.to_string(),
            });
            return Err(error);
        }
        *self
            .staged_artifact
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(stage_path);
        self.event_tx
            .send(UpdateEvent::Ready { info })
            .map_err(|_| UpdaterError::EventChannelClosed)
    }

    /// Install the staged update and restart the process.
    ///
    /// The selected-stack shell owns the actual app quit/restart decision.
    /// This backend method validates that a local/file `.exe` artifact was
    /// staged, launches it with the NSIS silent flag (`/S`), and emits
    /// [`UpdateEvent::Installing`]. The shell responds by quitting the message
    /// loop so the installer can replace files.
    pub fn install(&self) -> Result<(), UpdaterError> {
        self.install_with_launcher(launch_nsis_installer)
    }

    fn install_with_launcher<F>(&self, launch: F) -> Result<(), UpdaterError>
    where
        F: FnOnce(&Path) -> Result<(), UpdaterError>,
    {
        let info = self
            .pending_update
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| {
                UpdaterError::InvalidManifest("no pending update to install".to_owned())
            })?;
        let staged_path = self
            .staged_artifact
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| {
                UpdaterError::InvalidManifest("no staged artifact to install".to_owned())
            })?;
        validate_staged_installer(&staged_path)?;
        launch(&staged_path)?;
        self.event_tx
            .send(UpdateEvent::Installing { info })
            .map_err(|_| UpdaterError::EventChannelClosed)
    }

    /// Persist `version` as "skipped" — subsequent [`check`] calls return
    /// `Ok(None)` for that exact build.
    ///
    /// This entry IS implemented today (no I/O surface required — the slot
    /// is in-memory; settings persistence is the dispatcher's job).
    pub fn skip_version(&self, version: SmolStr) {
        tracing::info!("updater: user requested to skip {version}");
        self.skipped_version.set(Some(version));
    }

    /// Snapshot of the currently skipped version, if any.
    pub fn current_skipped(&self) -> Option<SmolStr> {
        self.skipped_version.get()
    }

    /// Snapshot of the currently staged artifact path, if a local/file
    /// download completed successfully.
    pub fn staged_artifact(&self) -> Option<PathBuf> {
        self.staged_artifact
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Fire-and-forget background check.
    ///
    /// Spawns a standard OS thread, runs the same manifest check path as the
    /// visible `CheckForUpdates` command, stores the pending update in the
    /// shared updater state, and emits a typed event for the UI pump to drain.
    pub fn spawn_background_check(&self) {
        self.spawn_background_check_loop("bentodesk-updater-check", None, Some(1));
    }

    /// Fire-and-forget recurring background checks.
    ///
    /// The first check runs immediately, then subsequent checks run after
    /// `interval` until the process exits. This mirrors the selected-stack
    /// process lifetime: no async runtime, no hidden Tauri scheduler, and no
    /// fake timer state outside the backend updater.
    pub fn spawn_recurring_background_check(&self, interval: Duration) {
        self.spawn_background_check_loop("bentodesk-updater-scheduler", Some(interval), None);
    }

    #[cfg(test)]
    fn spawn_recurring_background_check_for_test(&self, interval: Duration, max_runs: usize) {
        self.spawn_background_check_loop(
            "bentodesk-updater-scheduler-test",
            Some(interval),
            Some(max_runs.max(1)),
        );
    }

    fn clone_for_background_worker(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
            skipped_version: self.skipped_version.clone(),
            manifest_source: self.manifest_source.clone(),
            minisign_public_key: self.minisign_public_key.clone(),
            pending_update: Arc::clone(&self.pending_update),
            staged_artifact: Arc::clone(&self.staged_artifact),
        }
    }

    fn spawn_background_check_loop(
        &self,
        thread_name: &'static str,
        interval: Option<Duration>,
        max_runs: Option<usize>,
    ) {
        let worker = self.clone_for_background_worker();
        match std::thread::Builder::new()
            .name(thread_name.to_owned())
            .spawn(move || {
                let mut completed_runs = 0usize;
                loop {
                    worker.run_background_check_once();
                    completed_runs = completed_runs.saturating_add(1);
                    if max_runs.is_some_and(|limit| completed_runs >= limit) {
                        break;
                    }
                    let Some(interval) = interval else {
                        break;
                    };
                    std::thread::sleep(interval);
                }
            }) {
            Ok(_handle) => {}
            Err(error) => {
                let _ = self.event_tx.send(UpdateEvent::Error {
                    kind: SmolStr::new_static("check"),
                    message: format!("background updater thread spawn failed: {error}"),
                });
            }
        }
    }

    fn run_background_check_once(&self) {
        match self.check() {
            Ok(Some(info)) => {
                let _ = self.event_tx.send(UpdateEvent::Available { info });
            }
            Ok(None) => {}
            Err(error) => {
                let _ = self.event_tx.send(UpdateEvent::Error {
                    kind: SmolStr::new_static("check"),
                    message: error.to_string(),
                });
            }
        }
    }

    fn load_manifest_text(&self) -> Result<Option<String>, UpdaterError> {
        let Some(source) = self.manifest_source.as_ref() else {
            return Ok(None);
        };
        let source = source.as_str().trim();
        if source.is_empty() {
            return Ok(None);
        }
        if source.starts_with("http://") || source.starts_with("https://") {
            return fetch_manifest_winhttp(source).map(Some);
        }
        let path = if let Some(rest) = source.strip_prefix("file://") {
            PathBuf::from(rest)
        } else if source.contains("://") {
            return Err(UpdaterError::UnsupportedManifestSource(source.to_owned()));
        } else {
            PathBuf::from(source)
        };
        let text = std::fs::read_to_string(&path)
            .map_err(|error| UpdaterError::FetchFailed(format!("{}: {error}", path.display())))?;
        if text.len() > MAX_MANIFEST_BYTES {
            return Err(UpdaterError::FetchFailed(format!(
                "{} exceeds {} bytes",
                path.display(),
                MAX_MANIFEST_BYTES
            )));
        }
        Ok(Some(text))
    }
}

mod artifact;
mod http;

use artifact::*;
use http::*;

/// Hours between automatic update checks for the chosen frequency. Returns
/// `None` for `Manual`. Unchanged from 1.x.
pub fn check_interval_hours(freq: UpdateCheckFrequency) -> Option<u64> {
    match freq {
        UpdateCheckFrequency::Daily => Some(24),
        UpdateCheckFrequency::Weekly => Some(24 * 7),
        UpdateCheckFrequency::Manual => None,
    }
}

/// Currently running build version. Reads `CARGO_PKG_VERSION`, which is
/// substituted by Cargo at compile time (workspace-pinned, spec §15).
///
/// Returned as `SmolStr` so it inlines on the stack (≤22 bytes is the
/// inline budget and `MAJOR.MINOR.PATCH-channel` always fits).
pub fn pkg_version() -> SmolStr {
    SmolStr::new_static(env!("CARGO_PKG_VERSION"))
}

// ─── Internal: thread-safe optional skip slot ─────────────────────────
//
// We avoid pulling `parking_lot` (forbidden by §8) and keep this internal so
// the `parking_lot_skipped_slot` namespace is just a documentation hint.

mod parking_lot_skipped_slot {
    use std::sync::{Arc, Mutex};

    use smol_str::SmolStr;

    /// `Mutex<Option<SmolStr>>` newtype with poisoning recovery (spec §11).
    #[derive(Clone)]
    pub struct SkipSlot {
        inner: Arc<Mutex<Option<SmolStr>>>,
    }

    impl SkipSlot {
        pub fn new() -> Self {
            Self {
                inner: Arc::new(Mutex::new(None)),
            }
        }

        pub fn set(&self, v: Option<SmolStr>) {
            let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            *guard = v;
        }

        pub fn get(&self) -> Option<SmolStr> {
            let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        }
    }
}

#[cfg(test)]
mod tests;
