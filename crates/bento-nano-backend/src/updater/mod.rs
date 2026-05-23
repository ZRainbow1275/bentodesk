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
//! - Manifest check through `BENTODESK_NANO_UPDATE_MANIFEST_URL`, supporting
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

const MANIFEST_ENV: &str = "BENTODESK_NANO_UPDATE_MANIFEST_URL";
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
/// inside `AppSettings.updates.check_frequency`; nano keeps the same
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
/// Constructed once by `bento-nano-app::dispatcher` and held across the
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
        self.spawn_background_check_loop("bentodesk-nano-updater-check", None, Some(1));
    }

    /// Fire-and-forget recurring background checks.
    ///
    /// The first check runs immediately, then subsequent checks run after
    /// `interval` until the process exits. This mirrors the selected-stack
    /// process lifetime: no async runtime, no hidden Tauri scheduler, and no
    /// fake timer state outside the backend updater.
    pub fn spawn_recurring_background_check(&self, interval: Duration) {
        self.spawn_background_check_loop("bentodesk-nano-updater-scheduler", Some(interval), None);
    }

    #[cfg(test)]
    fn spawn_recurring_background_check_for_test(&self, interval: Duration, max_runs: usize) {
        self.spawn_background_check_loop(
            "bentodesk-nano-updater-scheduler-test",
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

// ─── Pure helpers (preserved verbatim from 1.x) ──────────────────────

#[derive(Debug, Deserialize)]
struct RawManifest {
    version: Option<String>,
    current_version: Option<String>,
    date: Option<String>,
    pub_date: Option<String>,
    body: Option<String>,
    notes: Option<String>,
    artifact_url: Option<String>,
    download_url: Option<String>,
    url: Option<String>,
    sha256: Option<String>,
    artifact_sha256: Option<String>,
    signature: Option<String>,
    platforms: Option<BTreeMap<String, RawPlatformManifest>>,
}

#[derive(Debug, Deserialize)]
struct RawPlatformManifest {
    url: Option<String>,
    sha256: Option<String>,
    artifact_sha256: Option<String>,
    signature: Option<String>,
}

fn parse_update_manifest(
    text: &str,
    fallback_current_version: SmolStr,
) -> Result<UpdateInfo, UpdaterError> {
    let raw: RawManifest = serde_json::from_str(text)
        .map_err(|error| UpdaterError::InvalidManifest(error.to_string()))?;
    let version = raw
        .version
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UpdaterError::InvalidManifest("missing non-empty version".to_owned()))?;
    let current_version = raw
        .current_version
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(SmolStr::new)
        .unwrap_or(fallback_current_version);
    let date = raw
        .date
        .or(raw.pub_date)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(SmolStr::new);
    let body = raw
        .body
        .or(raw.notes)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let platform = raw
        .platforms
        .as_ref()
        .and_then(|platforms| platforms.get(TAURI_WINDOWS_X64_PLATFORM));
    let platform_artifact_url = platform
        .and_then(|value| value.url.as_ref())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let platform_artifact_sha256 = platform
        .and_then(|value| value.artifact_sha256.as_ref().or(value.sha256.as_ref()))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let platform_signature = platform
        .and_then(|value| value.signature.as_ref())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let artifact_url = platform_artifact_url.or_else(|| {
        raw.artifact_url
            .or(raw.download_url)
            .or(raw.url)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    });
    let artifact_sha256 = platform_artifact_sha256.or_else(|| {
        raw.artifact_sha256
            .or(raw.sha256)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    });
    let signature = platform_signature.or_else(|| {
        raw.signature
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    });
    Ok(UpdateInfo {
        version: SmolStr::new(version),
        current_version,
        date,
        body,
        artifact_url,
        artifact_sha256,
        signature,
    })
}

fn staged_artifact_path(version: &str, source: &str) -> Result<PathBuf, UpdaterError> {
    let mut dir = std::env::temp_dir();
    dir.push("bentodesk-nano-update");
    std::fs::create_dir_all(&dir)
        .map_err(|error| UpdaterError::FetchFailed(format!("{}: {error}", dir.display())))?;
    let mut file_name = String::from("bentodesk-nano-");
    file_name.push_str(&sanitize_path_component(version));
    file_name.push_str(".update");
    if source.ends_with(".exe") {
        file_name.push_str(".exe");
    } else if source.ends_with(".msi") {
        file_name.push_str(".msi");
    }
    dir.push(file_name);
    Ok(dir)
}

fn sanitize_path_component(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    if out.is_empty() {
        out.push_str("unknown");
    }
    out
}

fn artifact_source_path(source: &str) -> Result<PathBuf, UpdaterError> {
    let source = source.trim();
    if source.contains("://") && !source.starts_with("file://") {
        return Err(UpdaterError::UnsupportedManifestSource(source.to_owned()));
    }
    if let Some(rest) = source.strip_prefix("file://") {
        Ok(PathBuf::from(rest))
    } else {
        Ok(PathBuf::from(source))
    }
}

fn copy_artifact_to_stage(
    source: &str,
    stage_path: &PathBuf,
    event_tx: &Sender<UpdateEvent>,
) -> Result<(), UpdaterError> {
    let source = source.trim();
    if source.starts_with("http://") || source.starts_with("https://") {
        return copy_http_artifact_to_stage_winhttp(source, stage_path, event_tx);
    }
    let source_path = artifact_source_path(source)?;
    let mut input = File::open(&source_path).map_err(|error| {
        UpdaterError::FetchFailed(format!("{}: {error}", source_path.display()))
    })?;
    let total_bytes = input.metadata().ok().map(|metadata| metadata.len());
    let mut output = File::create(stage_path)
        .map_err(|error| UpdaterError::FetchFailed(format!("{}: {error}", stage_path.display())))?;
    let mut buffer = [0u8; DOWNLOAD_BUFFER_BYTES];
    let mut written = 0u64;
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
        written += count as u64;
        emit_download_progress(event_tx, written, total_bytes)?;
    }
    output
        .flush()
        .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
    Ok(())
}

fn emit_download_progress(
    event_tx: &Sender<UpdateEvent>,
    written: u64,
    total_bytes: Option<u64>,
) -> Result<(), UpdaterError> {
    event_tx
        .send(UpdateEvent::Progress {
            progress: UpdateProgress {
                chunk_len: written,
                total_bytes,
            },
        })
        .map_err(|_| UpdaterError::EventChannelClosed)
}

fn validate_manifest_integrity_policy(info: &UpdateInfo) -> Result<(), UpdaterError> {
    if info.artifact_sha256.is_some() || info.signature.is_some() {
        return Ok(());
    }
    tracing::warn!(
        target: "bentodesk::updater",
        version = %info.version,
        "updater manifest has no sha256/artifact_sha256 integrity field; allowing legacy/internal unsigned artifact"
    );
    Ok(())
}

fn verify_staged_artifact(
    info: &UpdateInfo,
    stage_path: &Path,
    minisign_public_key: &str,
) -> Result<(), UpdaterError> {
    if let Some(expected) = info.artifact_sha256.as_deref() {
        let expected = normalize_sha256_hex(expected)?;
        let actual = sha256_file(stage_path)?;
        let actual_hex = hex_encode(&actual);
        if actual_hex != expected {
            return Err(UpdaterError::VerificationFailed(format!(
                "sha256 mismatch for {}: expected {expected}, got {actual_hex}",
                stage_path.display()
            )));
        }
    }
    if let Some(signature) = info.signature.as_deref() {
        verify_minisign_signature(minisign_public_key, signature, stage_path)?;
    }
    Ok(())
}

fn verify_minisign_signature(
    public_key_text: &str,
    signature_text: &str,
    stage_path: &Path,
) -> Result<(), UpdaterError> {
    let public_key = PublicKey::decode(public_key_text.trim()).map_err(|error| {
        UpdaterError::VerificationFailed(format!(
            "embedded minisign public key is invalid: {error}"
        ))
    })?;
    let decoded_signature = decode_tauri_minisign_signature(signature_text)?;
    let signature = Signature::decode(decoded_signature.as_str()).map_err(|error| {
        UpdaterError::VerificationFailed(format!("minisign signature is invalid: {error}"))
    })?;
    let mut verifier = public_key.verify_stream(&signature).map_err(|error| {
        UpdaterError::VerificationFailed(format!(
            "minisign stream verification setup failed: {error}"
        ))
    })?;
    let mut file = File::open(stage_path)
        .map_err(|error| UpdaterError::FetchFailed(format!("{}: {error}", stage_path.display())))?;
    let mut buffer = [0u8; DOWNLOAD_BUFFER_BYTES];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
        if count == 0 {
            break;
        }
        verifier.update(&buffer[..count]);
    }
    verifier.finalize().map_err(|error| {
        UpdaterError::VerificationFailed(format!(
            "minisign signature mismatch for {}: {error}",
            stage_path.display()
        ))
    })
}

fn decode_tauri_minisign_signature(value: &str) -> Result<String, UpdaterError> {
    let trimmed = value.trim();
    if looks_like_minisign_signature(trimmed) {
        return Ok(trimmed.to_owned());
    }

    let decoded = decode_base64_signature(trimmed)?;
    let decoded_text = String::from_utf8(decoded).map_err(|error| {
        UpdaterError::VerificationFailed(format!(
            "minisign signature base64 payload is not UTF-8: {error}"
        ))
    })?;
    let decoded_trimmed = decoded_text.trim();
    if !looks_like_minisign_signature(decoded_trimmed) {
        return Err(UpdaterError::VerificationFailed(
            "minisign signature base64 payload is not a minisign signature".to_owned(),
        ));
    }
    Ok(decoded_trimmed.to_owned())
}

fn looks_like_minisign_signature(value: &str) -> bool {
    value.contains("untrusted comment:") && value.contains("\ntrusted comment:")
}

fn decode_base64_signature(value: &str) -> Result<Vec<u8>, UpdaterError> {
    let mut decoded = Vec::with_capacity(value.len().saturating_mul(3) / 4);
    let mut quartet = [0u8; 4];
    let mut quartet_len = 0usize;
    let mut saw_padding = false;

    for (index, byte) in value.bytes().enumerate() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        let sextet = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                saw_padding = true;
                64
            }
            _ => {
                return Err(UpdaterError::VerificationFailed(format!(
                    "minisign signature base64 decode failed at byte {index}: invalid character"
                )));
            }
        };
        if saw_padding && sextet != 64 {
            return Err(UpdaterError::VerificationFailed(
                "minisign signature base64 decode failed: non-padding character after padding"
                    .to_owned(),
            ));
        }

        quartet[quartet_len] = sextet;
        quartet_len += 1;
        if quartet_len == 4 {
            decode_base64_quartet(&quartet, &mut decoded)?;
            quartet_len = 0;
        }
    }

    if quartet_len != 0 {
        return Err(UpdaterError::VerificationFailed(
            "minisign signature base64 decode failed: incomplete quartet".to_owned(),
        ));
    }

    Ok(decoded)
}

fn decode_base64_quartet(quartet: &[u8; 4], decoded: &mut Vec<u8>) -> Result<(), UpdaterError> {
    if quartet[0] == 64 || quartet[1] == 64 {
        return Err(UpdaterError::VerificationFailed(
            "minisign signature base64 decode failed: padding in leading positions".to_owned(),
        ));
    }
    if quartet[2] == 64 {
        if quartet[3] != 64 {
            return Err(UpdaterError::VerificationFailed(
                "minisign signature base64 decode failed: invalid padding sequence".to_owned(),
            ));
        }
        decoded.push((quartet[0] << 2) | (quartet[1] >> 4));
        return Ok(());
    }

    decoded.push((quartet[0] << 2) | (quartet[1] >> 4));
    decoded.push(((quartet[1] & 0x0F) << 4) | (quartet[2] >> 2));
    if quartet[3] != 64 {
        decoded.push(((quartet[2] & 0x03) << 6) | quartet[3]);
    }
    Ok(())
}

fn normalize_sha256_hex(value: &str) -> Result<String, UpdaterError> {
    let trimmed = value.trim();
    let without_prefix = trimmed
        .strip_prefix("sha256:")
        .or_else(|| trimmed.strip_prefix("SHA256:"))
        .unwrap_or(trimmed);
    let mut out = String::with_capacity(SHA256_HEX_CHARS);
    for ch in without_prefix.chars() {
        if ch.is_ascii_hexdigit() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, ' ' | '\t' | '\r' | '\n' | ':' | '-') {
            continue;
        } else {
            return Err(UpdaterError::VerificationFailed(format!(
                "sha256 contains non-hex character '{ch}'"
            )));
        }
    }
    if out.len() != SHA256_HEX_CHARS {
        return Err(UpdaterError::VerificationFailed(format!(
            "sha256 must contain {SHA256_HEX_CHARS} hex characters, got {}",
            out.len()
        )));
    }
    Ok(out)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(windows)]
struct BCryptSha256Alg(BCRYPT_ALG_HANDLE);

#[cfg(windows)]
impl BCryptSha256Alg {
    fn open() -> Result<Self, UpdaterError> {
        let mut handle: BCRYPT_ALG_HANDLE = ptr::null_mut();
        // SAFETY: `phalgorithm` points to a stack out-parameter, the algorithm
        // identifier is the static null-terminated UTF-16 SHA256 identifier
        // exposed by windows-sys, and `pszimplementation = NULL` selects the
        // default CNG provider.
        let status = unsafe {
            BCryptOpenAlgorithmProvider(&mut handle, BCRYPT_SHA256_ALGORITHM, ptr::null(), 0)
        };
        if status != 0 {
            return Err(UpdaterError::VerificationFailed(format!(
                "BCryptOpenAlgorithmProvider(SHA256) returned NTSTATUS {status:#x}"
            )));
        }
        Ok(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for BCryptSha256Alg {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: handle is owned by this RAII wrapper after a successful
            // BCryptOpenAlgorithmProvider call and is closed exactly once here.
            unsafe {
                BCryptCloseAlgorithmProvider(self.0, 0);
            }
        }
    }
}

#[cfg(windows)]
struct BCryptSha256Hash(BCRYPT_HASH_HANDLE);

#[cfg(windows)]
impl BCryptSha256Hash {
    fn create(alg: &BCryptSha256Alg) -> Result<Self, UpdaterError> {
        let mut handle: BCRYPT_HASH_HANDLE = ptr::null_mut();
        // SAFETY: `alg.0` is a valid SHA-256 algorithm handle. The hash handle
        // out-parameter is a stack pointer. Passing NULL hash-object/secret is
        // the documented CNG path for provider-managed hash object storage and
        // an unkeyed SHA-256 hash.
        let status =
            unsafe { BCryptCreateHash(alg.0, &mut handle, ptr::null_mut(), 0, ptr::null(), 0, 0) };
        if status != 0 {
            return Err(UpdaterError::VerificationFailed(format!(
                "BCryptCreateHash(SHA256) returned NTSTATUS {status:#x}"
            )));
        }
        Ok(Self(handle))
    }

    fn update(&self, bytes: &[u8]) -> Result<(), UpdaterError> {
        // SAFETY: `self.0` is a live hash handle owned by this wrapper and
        // `bytes` is a valid immutable buffer for the duration of the call.
        let status = unsafe { BCryptHashData(self.0, bytes.as_ptr(), bytes.len() as u32, 0) };
        if status != 0 {
            return Err(UpdaterError::VerificationFailed(format!(
                "BCryptHashData(SHA256) returned NTSTATUS {status:#x}"
            )));
        }
        Ok(())
    }

    fn finish(&self) -> Result<[u8; SHA256_DIGEST_BYTES], UpdaterError> {
        let mut digest = [0u8; SHA256_DIGEST_BYTES];
        // SAFETY: `self.0` is a live hash handle and `digest` is a valid
        // mutable output buffer of the exact SHA-256 digest size.
        let status =
            unsafe { BCryptFinishHash(self.0, digest.as_mut_ptr(), digest.len() as u32, 0) };
        if status != 0 {
            return Err(UpdaterError::VerificationFailed(format!(
                "BCryptFinishHash(SHA256) returned NTSTATUS {status:#x}"
            )));
        }
        Ok(digest)
    }
}

#[cfg(windows)]
impl Drop for BCryptSha256Hash {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: handle is owned by this RAII wrapper after a successful
            // BCryptCreateHash call and is destroyed exactly once here.
            unsafe {
                BCryptDestroyHash(self.0);
            }
        }
    }
}

#[cfg(windows)]
fn sha256_file(path: &Path) -> Result<[u8; SHA256_DIGEST_BYTES], UpdaterError> {
    let alg = BCryptSha256Alg::open()?;
    let hash = BCryptSha256Hash::create(&alg)?;
    let mut file = File::open(path)
        .map_err(|error| UpdaterError::FetchFailed(format!("{}: {error}", path.display())))?;
    let mut buffer = [0u8; DOWNLOAD_BUFFER_BYTES];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count])?;
    }
    hash.finish()
}

#[cfg(not(windows))]
fn sha256_file(_path: &Path) -> Result<[u8; SHA256_DIGEST_BYTES], UpdaterError> {
    Err(UpdaterError::VerificationFailed(
        "SHA-256 artifact verification requires the selected Windows CNG backend".to_owned(),
    ))
}

fn validate_staged_installer(path: &Path) -> Result<(), UpdaterError> {
    if !path.is_file() {
        return Err(UpdaterError::FetchFailed(format!(
            "staged updater artifact is missing: {}",
            path.display()
        )));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("exe") {
        return Err(UpdaterError::InvalidManifest(format!(
            "staged updater artifact is not an NSIS .exe: {}",
            path.display()
        )));
    }
    Ok(())
}

fn launch_nsis_installer(path: &Path) -> Result<(), UpdaterError> {
    ProcessCommand::new(path)
        .arg("/S")
        .spawn()
        .map(|_| ())
        .map_err(|error| UpdaterError::InstallFailed(format!("{}: {error}", path.display())))
}

fn version_is_newer(candidate: &str, current: &str) -> bool {
    let candidate = candidate.trim().trim_start_matches('v');
    let current = current.trim().trim_start_matches('v');
    if candidate == current {
        return false;
    }
    match (parse_version_parts(candidate), parse_version_parts(current)) {
        (Some(candidate_parts), Some(current_parts)) => candidate_parts > current_parts,
        _ => candidate > current,
    }
}

fn parse_version_parts(value: &str) -> Option<[u64; 4]> {
    let mut parts = [0u64; 4];
    let mut seen = 0usize;
    for raw in value.split(['.', '-']) {
        if seen >= parts.len() {
            return None;
        }
        let token = raw.trim();
        if token.is_empty() {
            return None;
        }
        let digits = token
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if digits.is_empty() || digits.len() != token.len() {
            return None;
        }
        parts[seen] = digits.parse::<u64>().ok()?;
        seen += 1;
    }
    if seen == 0 {
        return None;
    }
    Some(parts)
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedHttpUrl {
    secure: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_http_url(source: &str) -> Result<ParsedHttpUrl, UpdaterError> {
    let (secure, rest) = if let Some(rest) = source.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = source.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err(UpdaterError::UnsupportedManifestSource(source.to_owned()));
    };
    let (host_port, path_tail) = rest.split_once('/').unwrap_or((rest, ""));
    if host_port.is_empty() {
        return Err(UpdaterError::UnsupportedManifestSource(source.to_owned()));
    }
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port_text)) if !host.contains(':') => {
            let port = port_text
                .parse::<u16>()
                .map_err(|_| UpdaterError::UnsupportedManifestSource(source.to_owned()))?;
            (host, port)
        }
        _ => (host_port, if secure { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err(UpdaterError::UnsupportedManifestSource(source.to_owned()));
    }
    Ok(ParsedHttpUrl {
        secure,
        host: host.to_owned(),
        port,
        path: format!("/{path_tail}"),
    })
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(core::iter::once(0)).collect()
}

#[cfg(windows)]
struct WinHttpHandle(*mut core::ffi::c_void);

#[cfg(windows)]
impl Drop for WinHttpHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Networking::WinHttp::WinHttpCloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
struct WinHttpRequest {
    _session: WinHttpHandle,
    _connect: WinHttpHandle,
    request: WinHttpHandle,
}

#[cfg(windows)]
fn winhttp_last_error(context: &str) -> UpdaterError {
    use windows_sys::Win32::Foundation::GetLastError;

    let code = unsafe { GetLastError() };
    UpdaterError::FetchFailed(format!("{context} failed (GetLastError={code})"))
}

#[cfg(windows)]
fn open_winhttp_get(source: &str) -> Result<WinHttpRequest, UpdaterError> {
    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
        WINHTTP_QUERY_STATUS_CODE, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
        WinHttpQueryHeaders, WinHttpReceiveResponse, WinHttpSendRequest,
    };

    let parsed = parse_http_url(source)?;
    let agent = wide_null("BentoDesk Nano Updater");
    let session = unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            ptr::null(),
            ptr::null(),
            0,
        )
    };
    if session.is_null() {
        return Err(winhttp_last_error("WinHttpOpen"));
    }
    let session = WinHttpHandle(session);

    let host = wide_null(&parsed.host);
    let connect = unsafe { WinHttpConnect(session.0, host.as_ptr(), parsed.port, 0) };
    if connect.is_null() {
        return Err(winhttp_last_error("WinHttpConnect"));
    }
    let connect = WinHttpHandle(connect);

    let verb = wide_null("GET");
    let path = wide_null(&parsed.path);
    let flags = if parsed.secure {
        WINHTTP_FLAG_SECURE
    } else {
        0
    };
    let request = unsafe {
        WinHttpOpenRequest(
            connect.0,
            verb.as_ptr(),
            path.as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            flags,
        )
    };
    if request.is_null() {
        return Err(winhttp_last_error("WinHttpOpenRequest"));
    }
    let request = WinHttpHandle(request);

    if unsafe { WinHttpSendRequest(request.0, ptr::null(), 0, ptr::null(), 0, 0, 0) } == 0 {
        return Err(winhttp_last_error("WinHttpSendRequest"));
    }
    if unsafe { WinHttpReceiveResponse(request.0, ptr::null_mut()) } == 0 {
        return Err(winhttp_last_error("WinHttpReceiveResponse"));
    }

    let mut status_code = 0u32;
    let mut status_len = core::mem::size_of::<u32>() as u32;
    let mut status_index = 0u32;
    if unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            &mut status_code as *mut u32 as *mut _,
            &mut status_len,
            &mut status_index,
        )
    } == 0
    {
        return Err(winhttp_last_error("WinHttpQueryHeaders"));
    }
    if !(200..300).contains(&status_code) {
        return Err(UpdaterError::FetchFailed(format!(
            "{source} returned HTTP {status_code}"
        )));
    }

    Ok(WinHttpRequest {
        _session: session,
        _connect: connect,
        request,
    })
}

#[cfg(windows)]
fn winhttp_content_length(request: &WinHttpRequest) -> Option<u64> {
    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER, WinHttpQueryHeaders,
    };

    let mut content_length = 0u32;
    let mut content_len = core::mem::size_of::<u32>() as u32;
    let mut content_index = 0u32;
    if unsafe {
        WinHttpQueryHeaders(
            request.request.0,
            WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            &mut content_length as *mut u32 as *mut _,
            &mut content_len,
            &mut content_index,
        )
    } == 0
    {
        return None;
    }
    Some(u64::from(content_length))
}

#[cfg(windows)]
fn fetch_manifest_winhttp(source: &str) -> Result<String, UpdaterError> {
    use windows_sys::Win32::Networking::WinHttp::{WinHttpQueryDataAvailable, WinHttpReadData};

    let request = open_winhttp_get(source)?;
    let mut bytes = Vec::<u8>::new();
    loop {
        let mut available = 0u32;
        if unsafe { WinHttpQueryDataAvailable(request.request.0, &mut available) } == 0 {
            return Err(winhttp_last_error("WinHttpQueryDataAvailable"));
        }
        if available == 0 {
            break;
        }
        let next_len = bytes.len().saturating_add(available as usize);
        if next_len > MAX_MANIFEST_BYTES {
            return Err(UpdaterError::FetchFailed(format!(
                "manifest exceeds {MAX_MANIFEST_BYTES} bytes"
            )));
        }
        let start = bytes.len();
        bytes.resize(next_len, 0);
        let mut read = 0u32;
        if unsafe {
            WinHttpReadData(
                request.request.0,
                bytes[start..].as_mut_ptr() as *mut _,
                available,
                &mut read,
            )
        } == 0
        {
            return Err(winhttp_last_error("WinHttpReadData"));
        }
        bytes.truncate(start + read as usize);
        if read == 0 {
            break;
        }
    }
    String::from_utf8(bytes)
        .map_err(|error| UpdaterError::InvalidManifest(format!("manifest is not UTF-8: {error}")))
}

#[cfg(not(windows))]
fn fetch_manifest_winhttp(source: &str) -> Result<String, UpdaterError> {
    Err(UpdaterError::UnsupportedManifestSource(source.to_owned()))
}

#[cfg(windows)]
fn copy_http_artifact_to_stage_winhttp(
    source: &str,
    stage_path: &Path,
    event_tx: &Sender<UpdateEvent>,
) -> Result<(), UpdaterError> {
    use windows_sys::Win32::Networking::WinHttp::WinHttpReadData;

    let request = open_winhttp_get(source)?;
    let total_bytes = winhttp_content_length(&request);
    let mut output = File::create(stage_path)
        .map_err(|error| UpdaterError::FetchFailed(format!("{}: {error}", stage_path.display())))?;
    let mut buffer = [0u8; DOWNLOAD_BUFFER_BYTES];
    let mut written = 0u64;
    loop {
        let mut read = 0u32;
        if unsafe {
            WinHttpReadData(
                request.request.0,
                buffer.as_mut_ptr() as *mut _,
                buffer.len() as u32,
                &mut read,
            )
        } == 0
        {
            let _ = std::fs::remove_file(stage_path);
            return Err(winhttp_last_error("WinHttpReadData"));
        }
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read as usize])
            .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
        written = written.saturating_add(u64::from(read));
        emit_download_progress(event_tx, written, total_bytes)?;
    }
    output
        .flush()
        .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
    Ok(())
}

#[cfg(not(windows))]
fn copy_http_artifact_to_stage_winhttp(
    source: &str,
    _stage_path: &Path,
    _event_tx: &Sender<UpdateEvent>,
) -> Result<(), UpdaterError> {
    Err(UpdaterError::UnsupportedManifestSource(source.to_owned()))
}

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
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    const TEST_MINISIGN_PUBLIC_KEY: &str = "untrusted comment: minisign public key E7620F1842B4E81F\n\
         RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    const TEST_MINISIGN_SIGNATURE: &str = concat!(
        "untrusted comment: signature from minisign secret key\n",
        "RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/",
        "z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n",
        "trusted comment: timestamp:1556193335\tfile:test\n",
        "y/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg=="
    );
    const TEST_MINISIGN_SIGNATURE_BASE64: &str = concat!(
        "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUlVRZjZMUkNHQTlp",
        "NTU5cjNnN1YxcU55SkRBcEdpcDhNZnFjYWRJZ1Q5Q3VoVjNFTWhIb04xbUdUa1VpZEYvejdTcmxRZ1hkeThvZmpi",
        "N2JOSkp5bERPb2NyQ284S0x6WndvPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNTU2MTkzMzM1CWZpbGU6",
        "dGVzdAp5L3JVdzJ5OC9oT1VZalpVNzFlSHAvV28xS1o0MGZHeTJWSkVEbDM0WE1KTStUWDQ4U3MvMTd1M0l2SWZi",
        "VlIxRmtaWlNOQ2lzUWJ1UVkrYkh3aEVCZz09"
    );
    const TEST_ARTIFACT_SHA256: &str =
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    fn updater_with_test_minisign_key(
        event_tx: crossbeam_channel::Sender<UpdateEvent>,
        manifest_path: &Path,
    ) -> Updater {
        Updater::with_manifest_source_and_minisign_key(
            event_tx,
            Some(SmolStr::new(manifest_path.to_string_lossy())),
            SmolStr::new_static(TEST_MINISIGN_PUBLIC_KEY),
        )
    }

    #[test]
    fn decode_tauri_minisign_signature_accepts_raw_and_base64() {
        assert_eq!(
            decode_tauri_minisign_signature(TEST_MINISIGN_SIGNATURE).expect("raw signature"),
            TEST_MINISIGN_SIGNATURE
        );
        assert_eq!(
            decode_tauri_minisign_signature(TEST_MINISIGN_SIGNATURE_BASE64)
                .expect("base64 signature"),
            TEST_MINISIGN_SIGNATURE
        );
    }

    #[test]
    fn decode_tauri_minisign_signature_rejects_invalid_payloads() {
        let bad_base64 = decode_tauri_minisign_signature("not a tauri updater signature!");
        assert!(matches!(
            bad_base64,
            Err(UpdaterError::VerificationFailed(message))
                if message.contains("base64 decode failed")
        ));

        let non_minisign = decode_tauri_minisign_signature("dGVzdA==");
        assert!(matches!(
            non_minisign,
            Err(UpdaterError::VerificationFailed(message))
                if message.contains("not a minisign signature")
        ));
    }

    #[test]
    fn check_returns_none_without_manifest_source() {
        let (tx, _rx) = unbounded::<UpdateEvent>();
        let updater = Updater::with_manifest_source(tx, None);
        assert!(updater.check().expect("check").is_none());
    }

    #[test]
    fn check_reads_local_manifest_and_returns_available_update() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-manifest-{}.json",
            std::process::id()
        ));
        std::fs::write(
            &manifest_path,
            r#"{"version":"9.9.9","date":"2026-05-11","body":"Test release"}"#,
        )
        .expect("write manifest");
        let (tx, _rx) = unbounded::<UpdateEvent>();
        let updater =
            Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));

        let info = updater.check().expect("check").expect("available");
        assert_eq!(info.version.as_str(), "9.9.9");
        assert_eq!(info.current_version.as_str(), env!("CARGO_PKG_VERSION"));
        assert_eq!(info.date.as_deref(), Some("2026-05-11"));
        assert_eq!(info.body.as_deref(), Some("Test release"));
        let _ = std::fs::remove_file(&manifest_path);
    }

    #[test]
    fn check_honours_skipped_manifest_version() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-skipped-{}.json",
            std::process::id()
        ));
        std::fs::write(&manifest_path, r#"{"version":"9.9.8"}"#).expect("write manifest");
        let (tx, _rx) = unbounded::<UpdateEvent>();
        let updater =
            Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
        updater.skip_version(SmolStr::new_static("9.9.8"));

        assert!(updater.check().expect("check").is_none());
        let _ = std::fs::remove_file(&manifest_path);
    }

    #[test]
    fn check_tauri_style_manifest_maps_notes_and_pub_date() {
        let info = parse_update_manifest(
            r#"{"version":"9.9.7","pub_date":"2026-05-11T00:00:00Z","notes":"Release notes","url":"file://C:/tmp/BentoDeskSetup.exe","sha256":"204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df","signature":"sig"}"#,
            SmolStr::new_static("0.0.1"),
        )
        .expect("manifest");

        assert_eq!(info.version.as_str(), "9.9.7");
        assert_eq!(info.current_version.as_str(), "0.0.1");
        assert_eq!(info.date.as_deref(), Some("2026-05-11T00:00:00Z"));
        assert_eq!(info.body.as_deref(), Some("Release notes"));
        assert_eq!(
            info.artifact_url.as_deref(),
            Some("file://C:/tmp/BentoDeskSetup.exe")
        );
        assert_eq!(
            info.artifact_sha256.as_deref(),
            Some("204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df")
        );
        assert_eq!(info.signature.as_deref(), Some("sig"));
    }

    #[test]
    fn check_tauri_v2_platform_manifest_selects_windows_artifact() {
        let info = parse_update_manifest(
            r#"{
                "version":"9.9.7",
                "pub_date":"2026-05-11T00:00:00Z",
                "notes":"Release notes",
                "url":"file://C:/tmp/FallbackSetup.exe",
                "sha256":"0000000000000000000000000000000000000000000000000000000000000000",
                "platforms":{
                    "darwin-aarch64":{
                        "url":"https://example.invalid/BentoDesk.dmg",
                        "signature":"mac-sig"
                    },
                    "windows-x86_64":{
                        "url":"file://C:/tmp/BentoDeskSetup.exe",
                        "signature":"win-sig",
                        "sha256":"204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df"
                    }
                }
            }"#,
            SmolStr::new_static("0.0.1"),
        )
        .expect("platform manifest");

        assert_eq!(
            info.artifact_url.as_deref(),
            Some("file://C:/tmp/BentoDeskSetup.exe")
        );
        assert_eq!(
            info.artifact_sha256.as_deref(),
            Some("204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df")
        );
        assert_eq!(info.signature.as_deref(), Some("win-sig"));
    }

    #[test]
    fn version_compare_handles_semver_and_equal_versions() {
        assert!(version_is_newer("1.2.4", "1.2.3"));
        assert!(version_is_newer("v2.0.0", "1.9.9"));
        assert!(!version_is_newer("1.2.3", "1.2.3"));
        assert!(!version_is_newer("1.2.2", "1.2.3"));
    }

    #[test]
    fn download_copies_local_artifact_and_emits_progress_ready() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-download-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-artifact-{}.bin",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"installer-bytes").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{"version":"9.9.6","artifact_url":"{artifact_source}","artifact_sha256":"204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df"}}"#
            ),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater =
            Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
        assert!(updater.check().expect("check").is_some());

        updater.download().expect("download");
        let staged = updater.staged_artifact().expect("staged artifact");
        assert_eq!(
            std::fs::read(&staged).expect("read staged"),
            b"installer-bytes"
        );

        let mut saw_progress = false;
        let mut saw_ready = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                UpdateEvent::Progress { progress } => {
                    saw_progress = true;
                    assert_eq!(progress.total_bytes, Some(15));
                    assert_eq!(progress.chunk_len, 15);
                }
                UpdateEvent::Ready { info } => {
                    saw_ready = true;
                    assert_eq!(info.version.as_str(), "9.9.6");
                }
                other => panic!("unexpected updater event {other:?}"),
            }
        }
        assert!(saw_progress);
        assert!(saw_ready);
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
        let _ = std::fs::remove_file(&staged);
    }

    #[test]
    fn download_copies_tauri_v2_platform_artifact_and_emits_progress_ready() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-platform-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-platform-artifact-{}.bin",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"installer-bytes").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{
                    "version":"9.9.55",
                    "platforms":{{
                        "windows-x86_64":{{
                            "url":"{artifact_source}",
                            "sha256":"204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df"
                        }}
                    }}
                }}"#
            ),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater =
            Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
        assert!(updater.check().expect("check").is_some());

        updater.download().expect("download");
        let staged = updater.staged_artifact().expect("staged artifact");
        assert_eq!(
            std::fs::read(&staged).expect("read staged"),
            b"installer-bytes"
        );

        let mut saw_progress = false;
        let mut saw_ready = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                UpdateEvent::Progress { progress } => {
                    saw_progress = true;
                    assert_eq!(progress.total_bytes, Some(15));
                    assert_eq!(progress.chunk_len, 15);
                }
                UpdateEvent::Ready { info } => {
                    saw_ready = true;
                    assert_eq!(info.version.as_str(), "9.9.55");
                    assert_eq!(info.signature.as_deref(), None);
                }
                other => panic!("unexpected updater event {other:?}"),
            }
        }
        assert!(saw_progress);
        assert!(saw_ready);
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
        let _ = std::fs::remove_file(&staged);
    }

    #[cfg(windows)]
    #[test]
    fn download_accepts_valid_minisign_signature_with_sha256() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-signed-sha-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-signed-sha-artifact-{}.bin",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"test").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{"version":"9.9.4","artifact_url":"{artifact_source}","sha256":"{TEST_ARTIFACT_SHA256}","signature":{}}}"#,
                serde_json::to_string(TEST_MINISIGN_SIGNATURE).expect("signature json")
            ),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater = updater_with_test_minisign_key(tx, &manifest_path);
        assert!(updater.check().expect("check").is_some());

        updater.download().expect("download");
        let staged = updater.staged_artifact().expect("staged artifact");
        assert_eq!(std::fs::read(&staged).expect("read staged"), b"test");
        assert!(rx.try_iter().any(
            |event| matches!(event, UpdateEvent::Ready { info } if info.version.as_str() == "9.9.4")
        ));
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
        let _ = std::fs::remove_file(&staged);
    }

    #[cfg(windows)]
    #[test]
    fn download_accepts_valid_minisign_signature_only_manifest() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-signature-only-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-signature-only-artifact-{}.bin",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"test").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{"version":"9.9.3","artifact_url":"{artifact_source}","signature":{}}}"#,
                serde_json::to_string(TEST_MINISIGN_SIGNATURE).expect("signature json")
            ),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater = updater_with_test_minisign_key(tx, &manifest_path);
        assert!(updater.check().expect("check").is_some());

        updater.download().expect("download");
        let staged = updater.staged_artifact().expect("staged artifact");
        assert_eq!(std::fs::read(&staged).expect("read staged"), b"test");
        assert!(rx.try_iter().any(
            |event| matches!(event, UpdateEvent::Ready { info } if info.version.as_str() == "9.9.3")
        ));
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
        let _ = std::fs::remove_file(&staged);
    }

    #[cfg(windows)]
    #[test]
    fn download_accepts_tauri_base64_minisign_signature() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-base64-signature-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-base64-signature-artifact-{}.bin",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"test").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{"version":"9.9.32","artifact_url":"{artifact_source}","signature":{}}}"#,
                serde_json::to_string(TEST_MINISIGN_SIGNATURE_BASE64).expect("signature json")
            ),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater = updater_with_test_minisign_key(tx, &manifest_path);
        assert!(updater.check().expect("check").is_some());

        updater.download().expect("download");
        let staged = updater.staged_artifact().expect("staged artifact");
        assert_eq!(std::fs::read(&staged).expect("read staged"), b"test");
        assert!(rx.try_iter().any(
            |event| matches!(event, UpdateEvent::Ready { info } if info.version.as_str() == "9.9.32")
        ));
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
        let _ = std::fs::remove_file(&staged);
    }

    #[cfg(windows)]
    #[test]
    fn download_verifies_tauri_v2_platform_minisign_signature() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-platform-signature-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-platform-signature-artifact-{}.bin",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"test").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{
                    "version":"9.9.56",
                    "platforms":{{
                        "windows-x86_64":{{
                            "url":"{artifact_source}",
                            "signature":{}
                        }}
                    }}
                }}"#,
                serde_json::to_string(TEST_MINISIGN_SIGNATURE).expect("signature json")
            ),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater = updater_with_test_minisign_key(tx, &manifest_path);
        assert!(updater.check().expect("check").is_some());

        updater.download().expect("download");
        let staged = updater.staged_artifact().expect("staged artifact");
        assert_eq!(std::fs::read(&staged).expect("read staged"), b"test");
        assert!(rx.try_iter().any(
            |event| matches!(event, UpdateEvent::Ready { info } if info.version.as_str() == "9.9.56")
        ));
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
        let _ = std::fs::remove_file(&staged);
    }

    #[cfg(windows)]
    #[test]
    fn download_verifies_tauri_v2_platform_base64_minisign_signature() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-platform-base64-signature-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-platform-base64-signature-artifact-{}.bin",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"test").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{
                    "version":"9.9.57",
                    "platforms":{{
                        "windows-x86_64":{{
                            "url":"{artifact_source}",
                            "signature":{}
                        }}
                    }}
                }}"#,
                serde_json::to_string(TEST_MINISIGN_SIGNATURE_BASE64).expect("signature json")
            ),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater = updater_with_test_minisign_key(tx, &manifest_path);
        assert!(updater.check().expect("check").is_some());

        updater.download().expect("download");
        let staged = updater.staged_artifact().expect("staged artifact");
        assert_eq!(std::fs::read(&staged).expect("read staged"), b"test");
        assert!(rx.try_iter().any(
            |event| matches!(event, UpdateEvent::Ready { info } if info.version.as_str() == "9.9.57")
        ));
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
        let _ = std::fs::remove_file(&staged);
    }

    #[cfg(windows)]
    #[test]
    fn download_deletes_stage_and_emits_error_when_minisign_mismatches() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-signature-mismatch-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-signature-mismatch-artifact-{}.bin",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"Test").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{"version":"9.9.31","artifact_url":"{artifact_source}","signature":{}}}"#,
                serde_json::to_string(TEST_MINISIGN_SIGNATURE).expect("signature json")
            ),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater = updater_with_test_minisign_key(tx, &manifest_path);
        assert!(updater.check().expect("check").is_some());

        let error = updater
            .download()
            .expect_err("artifact signed for different bytes must fail");
        assert!(matches!(error, UpdaterError::VerificationFailed(_)));
        assert!(updater.staged_artifact().is_none());
        let staged = staged_artifact_path("9.9.31", artifact_source.as_str()).expect("stage path");
        assert!(!staged.exists());
        let event = rx
            .try_iter()
            .find(|event| matches!(event, UpdateEvent::Error { .. }))
            .expect("verify error event");
        assert!(matches!(
            event,
            UpdateEvent::Error { kind, message }
                if kind.as_str() == "verify" && message.contains("minisign signature mismatch")
        ));
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
    }

    #[cfg(windows)]
    #[test]
    fn download_deletes_stage_and_emits_error_when_base64_signature_is_invalid() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-invalid-base64-signature-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-invalid-base64-signature-artifact-{}.bin",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"test").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{"version":"9.9.33","artifact_url":"{artifact_source}","signature":"not a tauri updater signature!"}}"#
            ),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater = updater_with_test_minisign_key(tx, &manifest_path);
        assert!(updater.check().expect("check").is_some());

        let error = updater
            .download()
            .expect_err("invalid base64 signature must fail");
        assert!(matches!(error, UpdaterError::VerificationFailed(_)));
        assert!(updater.staged_artifact().is_none());
        let staged = staged_artifact_path("9.9.33", artifact_source.as_str()).expect("stage path");
        assert!(!staged.exists());
        let event = rx
            .try_iter()
            .find(|event| matches!(event, UpdateEvent::Error { .. }))
            .expect("verify error event");
        assert!(matches!(
            event,
            UpdateEvent::Error { kind, message }
                if kind.as_str() == "verify" && message.contains("base64 decode failed")
        ));
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
    }

    #[cfg(windows)]
    #[test]
    fn download_deletes_stage_and_emits_error_when_signed_sha256_mismatches() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-sha-mismatch-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-sha-mismatch-artifact-{}.bin",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"test").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{"version":"9.9.2","artifact_url":"{artifact_source}","sha256":"0000000000000000000000000000000000000000000000000000000000000000","signature":{}}}"#,
                serde_json::to_string(TEST_MINISIGN_SIGNATURE).expect("signature json")
            ),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater = updater_with_test_minisign_key(tx, &manifest_path);
        assert!(updater.check().expect("check").is_some());

        let error = updater.download().expect_err("sha mismatch must fail");
        assert!(matches!(error, UpdaterError::VerificationFailed(_)));
        assert!(updater.staged_artifact().is_none());
        let staged = staged_artifact_path("9.9.2", artifact_source.as_str()).expect("stage path");
        assert!(!staged.exists());
        let event = rx
            .try_iter()
            .find(|event| matches!(event, UpdateEvent::Error { .. }))
            .expect("verify error event");
        assert!(matches!(
            event,
            UpdateEvent::Error { kind, message }
                if kind.as_str() == "verify" && message.contains("sha256 mismatch")
        ));
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
    }

    #[cfg(windows)]
    #[test]
    fn background_check_emits_available_and_preserves_pending_download() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-background-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-background-artifact-{}.bin",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"installer-bytes").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{"version":"9.9.1","artifact_url":"{artifact_source}","sha256":"204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df"}}"#
            ),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater =
            Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));

        updater.spawn_background_check();
        let event = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("background updater event");
        assert!(matches!(
            event,
            UpdateEvent::Available { info } if info.version.as_str() == "9.9.1"
        ));

        updater.download().expect("download after background check");
        let staged = updater.staged_artifact().expect("staged artifact");
        assert_eq!(
            std::fs::read(&staged).expect("read staged"),
            b"installer-bytes"
        );
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
        let _ = std::fs::remove_file(&staged);
    }

    #[test]
    fn recurring_background_check_repeats_until_test_run_limit() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-recurring-manifest-{}.json",
            std::process::id()
        ));
        std::fs::write(&manifest_path, r#"{"version":"9.9.2"}"#).expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater =
            Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));

        updater.spawn_recurring_background_check_for_test(Duration::from_millis(10), 2);
        for _ in 0..2 {
            let event = rx
                .recv_timeout(Duration::from_secs(2))
                .expect("recurring updater event");
            assert!(matches!(
                event,
                UpdateEvent::Available { info } if info.version.as_str() == "9.9.2"
            ));
        }
        let _ = std::fs::remove_file(&manifest_path);
    }

    #[cfg(windows)]
    #[test]
    fn download_streams_http_artifact_and_emits_progress_ready() {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;
        use std::thread;

        let artifact_bytes = b"remote-installer-bytes".to_vec();
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local http");
        let addr = listener.local_addr().expect("local addr");
        let served_bytes = artifact_bytes.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request).expect("read request");
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                served_bytes.len()
            );
            stream.write_all(header.as_bytes()).expect("write header");
            stream.write_all(&served_bytes).expect("write body");
        });

        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-http-download-manifest-{}.json",
            std::process::id()
        ));
        let artifact_url = format!("http://{addr}/BentoDeskSetup.exe");
        std::fs::write(
            &manifest_path,
            format!(r#"{{"version":"9.9.4","artifact_url":"{artifact_url}"}}"#),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater =
            Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
        assert!(updater.check().expect("check").is_some());

        updater.download().expect("http download");
        server.join().expect("server join");
        let staged = updater.staged_artifact().expect("staged artifact");
        assert_eq!(std::fs::read(&staged).expect("read staged"), artifact_bytes);

        let mut saw_progress = false;
        let mut saw_ready = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                UpdateEvent::Progress { progress } => {
                    saw_progress = true;
                    assert_eq!(progress.total_bytes, Some(22));
                    assert_eq!(progress.chunk_len, 22);
                }
                UpdateEvent::Ready { info } => {
                    saw_ready = true;
                    assert_eq!(info.version.as_str(), "9.9.4");
                }
                other => panic!("unexpected updater event {other:?}"),
            }
        }
        assert!(saw_progress);
        assert!(saw_ready);
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&staged);
    }

    #[test]
    fn install_requires_a_staged_artifact() {
        let (tx, _rx) = unbounded::<UpdateEvent>();
        let updater = Updater::with_manifest_source(tx, None);
        assert!(matches!(
            updater.install(),
            Err(UpdaterError::InvalidManifest(message)) if message.contains("no pending update")
        ));
    }

    #[test]
    fn install_launches_staged_nsis_artifact_and_emits_installing() {
        let manifest_path = std::env::temp_dir().join(format!(
            "bento-nano-update-install-manifest-{}.json",
            std::process::id()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "bento-nano-update-artifact-{}.exe",
            std::process::id()
        ));
        std::fs::write(&artifact_path, b"fake-nsis").expect("write artifact");
        let artifact_source = artifact_path.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &manifest_path,
            format!(r#"{{"version":"9.9.5","artifact_url":"{artifact_source}"}}"#),
        )
        .expect("write manifest");
        let (tx, rx) = unbounded::<UpdateEvent>();
        let updater =
            Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));
        assert!(updater.check().expect("check").is_some());
        updater.download().expect("download");
        let staged = updater.staged_artifact().expect("staged artifact");

        let launched = std::cell::RefCell::new(None::<PathBuf>);
        updater
            .install_with_launcher(|path| {
                launched.borrow_mut().replace(path.to_path_buf());
                Ok(())
            })
            .expect("install");
        assert_eq!(launched.borrow().as_ref(), Some(&staged));

        let mut saw_installing = false;
        while let Ok(event) = rx.try_recv() {
            if let UpdateEvent::Installing { info } = event {
                saw_installing = true;
                assert_eq!(info.version.as_str(), "9.9.5");
            }
        }
        assert!(saw_installing);
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&artifact_path);
        let _ = std::fs::remove_file(&staged);
    }

    #[test]
    fn skip_version_round_trips() {
        let (tx, _rx) = unbounded::<UpdateEvent>();
        let updater = Updater::with_manifest_source(tx, None);
        assert!(updater.current_skipped().is_none());
        updater.skip_version(SmolStr::new_static("2.1.0"));
        assert_eq!(updater.current_skipped().as_deref(), Some("2.1.0"));
    }

    #[test]
    fn check_interval_matches_frequency() {
        assert_eq!(check_interval_hours(UpdateCheckFrequency::Daily), Some(24));
        assert_eq!(
            check_interval_hours(UpdateCheckFrequency::Weekly),
            Some(24 * 7)
        );
        assert_eq!(check_interval_hours(UpdateCheckFrequency::Manual), None);
    }

    #[test]
    fn pkg_version_matches_cargo_constant() {
        let v = pkg_version();
        assert_eq!(v.as_str(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn update_info_serde_round_trip() {
        let info = UpdateInfo {
            version: SmolStr::new_static("2.1.0"),
            current_version: SmolStr::new_static("2.0.0"),
            date: Some(SmolStr::new_static("2026-05-03T00:00:00Z")),
            body: Some("Initial v2.1 release".to_string()),
            artifact_url: Some("file://C:/tmp/BentoDeskSetup.exe".to_string()),
            artifact_sha256: Some(
                "204676736cea68d6411da9d3aa3fab0a5e70b023ba30cd560cfa9c8e7250f4df".to_string(),
            ),
            signature: Some("sig".to_string()),
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let parsed: UpdateInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, info);
    }

    #[test]
    fn check_frequency_serde_round_trip() {
        let json = serde_json::to_string(&UpdateCheckFrequency::Daily).expect("ser");
        let parsed: UpdateCheckFrequency = serde_json::from_str(&json).expect("de");
        assert_eq!(parsed, UpdateCheckFrequency::Daily);
    }
}
