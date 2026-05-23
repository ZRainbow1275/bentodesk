//! Selected-stack synchronized recovery bundle.
//!
//! The Tauri baseline keeps a coherent `recovery_bundle.json` beside the
//! normal per-file backups. Nano cannot reuse Tauri `AppHandle`,
//! `LayoutData`, or Solid settings types, so this module stores the selected
//! stack's authoritative `zones.bin` payload plus optional config-vault bytes
//! in an atomic JSON bundle. Shell code is responsible for converting live
//! `ZoneList` values to and from the binary codec and for reopening the
//! restored vault; this backend module owns the durable bundle file, checksum
//! validation, and JSON backup recovery.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::{
    config_vault::wire::{Base64Error, base64_decode, base64_encode},
    icon_positions::SavedIconLayout,
    stealth::SafetyManifest,
    storage::{self, StorageError},
};

/// Current JSON schema version for selected-stack recovery bundles.
pub const RECOVERY_BUNDLE_SCHEMA_VERSION: u32 = 2;
/// Oldest schema version this binary can still restore.
pub const RECOVERY_BUNDLE_MIN_SCHEMA_VERSION: u32 = 1;
/// Codec tag for the bundled `zones.bin` payload.
pub const RECOVERY_BUNDLE_ZONES_CODEC: &str = "bento-nano-platform::storage/zones.bin:v5";
/// Codec tag for the bundled selected-stack config vault payload.
pub const RECOVERY_BUNDLE_VAULT_CODEC: &str = "bento-nano-backend::config_vault/vault.bin:v1";
/// Codec tag for synchronized selected-stack user-data sidecars.
pub const RECOVERY_BUNDLE_USER_DATA_CODEC: &str = "bento-nano-user-data/raw:v1";

const RECOVERY_BUNDLE_DIR: &str = "recovery";
const RECOVERY_BUNDLE_FILENAME: &str = "recovery_bundle.json";
const RECOVERY_DIAGNOSTICS_FILENAME: &str = "recovery_diagnostics.json";
const FNV64_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV64_PRIME: u64 = 0x0000_0100_0000_01b3;
const USER_DATA_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const USER_DATA_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const USER_DATA_ROOT_FILES: &[&str] = &["rules.json", "rules.json.bak"];
const USER_DATA_DIRS: &[&str] = &["timeline", "snapshots", "backups", "themes", "plugins"];

/// JSON persisted recovery bundle for the selected stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverySafetyManifest {
    /// Desktop path that owns the `.bentodesk/manifest.json` snapshot.
    pub desktop_path: String,
    /// Full selected-stack safety manifest snapshot.
    pub manifest: SafetyManifest,
}

/// Binary user-data sidecar bundled alongside the authoritative `zones.bin`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryUserDataFile {
    /// Path relative to the selected-stack data root.
    pub relative_path: SmolStr,
    /// Explicit codec tag for the raw payload.
    pub codec: SmolStr,
    /// Byte length before base64 encoding.
    pub len_bytes: u64,
    /// FNV-1a checksum of the raw bytes, hex-encoded.
    pub checksum: SmolStr,
    /// Base64 encoded raw bytes.
    pub payload_b64: String,
}

/// JSON persisted recovery bundle for the selected stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryBundle {
    /// Schema version for forward-compatible decoding.
    pub schema_version: u32,
    /// UTC RFC3339 timestamp generated when the bundle was captured.
    pub captured_at: SmolStr,
    /// Display form of the data root that owns this bundle.
    pub data_root: String,
    /// Display form of the `zones.bin` path whose bytes were captured.
    pub zones_path: String,
    /// Explicit codec tag for the base64 payload.
    pub zones_codec: SmolStr,
    /// Number of zones in the live layout at capture time.
    pub zone_count: u32,
    /// Byte length before base64 encoding.
    pub zones_len_bytes: u64,
    /// FNV-1a checksum of the raw `zones.bin` bytes, hex-encoded.
    pub zones_checksum: SmolStr,
    /// Base64 encoded `zones.bin` bytes.
    pub zones_bin_b64: String,
    /// Display form of the captured config-vault path.
    #[serde(default)]
    pub vault_path: Option<String>,
    /// Explicit codec tag for the base64 config-vault payload.
    #[serde(default)]
    pub vault_codec: Option<SmolStr>,
    /// Config-vault byte length before base64 encoding.
    #[serde(default)]
    pub vault_len_bytes: Option<u64>,
    /// FNV-1a checksum of the raw config-vault bytes, hex-encoded.
    #[serde(default)]
    pub vault_checksum: Option<SmolStr>,
    /// Base64 encoded config-vault bytes.
    #[serde(default)]
    pub vault_bin_b64: Option<String>,
    /// Safety manifest snapshots captured from real Desktop `.bentodesk`
    /// directories referenced by live zone items.
    #[serde(default)]
    pub safety_manifests: Vec<RecoverySafetyManifest>,
    /// Optional saved desktop icon layout sidecar.
    #[serde(default)]
    pub icon_backup: Option<SavedIconLayout>,
    /// Selected-stack user-data sidecars, such as rules, timeline,
    /// snapshots, settings backups, imported themes, and plugin registry.
    #[serde(default)]
    pub user_data_files: Vec<RecoveryUserDataFile>,
}

/// Lightweight status returned to the shell after capture or restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryBundleSummary {
    /// UTC RFC3339 timestamp generated when the bundle was captured.
    pub captured_at: SmolStr,
    /// Number of zones captured in the bundle.
    pub zone_count: u32,
    /// Byte length of the bundled `zones.bin` payload.
    pub zones_len_bytes: u64,
    /// Stable checksum shown in logs/tests.
    pub zones_checksum: SmolStr,
    /// Whether the bundle contains a synchronized config-vault payload.
    pub vault_included: bool,
    /// Byte length of the bundled config-vault payload.
    pub vault_len_bytes: Option<u64>,
    /// Stable checksum for the bundled config-vault payload.
    pub vault_checksum: Option<SmolStr>,
    /// Number of safety manifests included in the bundle.
    pub safety_manifest_count: usize,
    /// Whether the bundle contains a saved icon-layout sidecar.
    pub icon_backup_included: bool,
    /// Number of synchronized user-data sidecar files included.
    pub user_data_file_count: usize,
    /// Total byte length of synchronized user-data sidecar payloads.
    pub user_data_len_bytes: u64,
    /// Filesystem path of the bundle JSON.
    pub path: PathBuf,
}

/// Decoded config-vault payload ready for shell-side restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredVaultPayload {
    /// Raw config-vault bytes validated against the bundle checksum.
    pub vault_bin: Vec<u8>,
    /// Display form of the vault path captured at bundle time.
    pub vault_path: Option<String>,
}

/// Decoded user-data sidecar ready for shell-side restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredUserDataFile {
    /// Safe relative path under the selected-stack data root.
    pub relative_path: SmolStr,
    /// Raw bytes validated against the bundle checksum.
    pub bytes: Vec<u8>,
    /// Stable checksum copied from the bundle metadata.
    pub checksum: SmolStr,
}

/// Decoded recovery payload ready for shell-side `zones.bin` decoding.
#[derive(Debug, Clone)]
pub struct RecoveredZonesPayload {
    /// Raw `zones.bin` bytes validated against the bundle checksum.
    pub zones_bin: Vec<u8>,
    /// Optional raw config-vault bytes validated against the bundle checksum.
    pub vault: Option<RecoveredVaultPayload>,
    /// Safety manifest snapshots to restore as sidecars.
    pub safety_manifests: Vec<RecoverySafetyManifest>,
    /// Optional saved icon-layout sidecar to restore under app data.
    pub icon_backup: Option<SavedIconLayout>,
    /// Validated selected-stack user-data sidecars.
    pub user_data_files: Vec<RecoveredUserDataFile>,
    /// Bundle metadata for visible status reporting.
    pub summary: RecoveryBundleSummary,
}

/// JSON diagnostics export for the currently captured recovery bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveryDiagnosticsReport {
    /// Report schema version. Kept separate from the bundle schema.
    pub schema_version: u32,
    /// UTC RFC3339 timestamp generated when this diagnostics report was exported.
    pub generated_at: SmolStr,
    /// Display form of the selected-stack data root.
    pub data_root: String,
    /// Filesystem path of the diagnostics JSON itself.
    pub diagnostics_path: String,
    /// Filesystem path of the source recovery bundle JSON.
    pub bundle_path: String,
    /// Bundle capture timestamp copied from the source bundle.
    pub captured_at: SmolStr,
    /// Validated `zones.bin` payload diagnostics.
    pub zones: RecoveryDiagnosticsBlob,
    /// Validated optional config-vault payload diagnostics.
    pub vault: RecoveryDiagnosticsOptionalBlob,
    /// Safety manifest sidecar diagnostics.
    pub safety_manifests: Vec<RecoveryDiagnosticsSafetyManifest>,
    /// Desktop icon-layout sidecar diagnostics.
    pub icon_backup: RecoveryDiagnosticsIconBackup,
    /// Synchronized selected-stack user-data sidecar diagnostics.
    #[serde(default)]
    pub user_data_files: Vec<RecoveryDiagnosticsUserDataFile>,
}

/// Binary blob diagnostics copied from validated bundle metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryDiagnosticsBlob {
    /// Explicit codec tag for this payload.
    pub codec: SmolStr,
    /// Zone count reported by the captured bundle.
    pub zone_count: u32,
    /// Byte length stored in bundle metadata.
    pub len_bytes: u64,
    /// Byte length measured after base64 decode.
    pub decoded_len_bytes: u64,
    /// Stable FNV-1a checksum copied from bundle metadata after validation.
    pub checksum: SmolStr,
    /// Display form of the original payload path.
    pub path: String,
}

/// Optional config-vault payload diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryDiagnosticsOptionalBlob {
    /// Whether this optional payload is present in the source bundle.
    pub included: bool,
    /// Explicit codec tag for this payload.
    pub codec: Option<SmolStr>,
    /// Byte length stored in bundle metadata.
    pub len_bytes: Option<u64>,
    /// Byte length measured after base64 decode.
    pub decoded_len_bytes: Option<u64>,
    /// Stable FNV-1a checksum copied from bundle metadata after validation.
    pub checksum: Option<SmolStr>,
    /// Display form of the original payload path.
    pub path: Option<String>,
}

/// Safety manifest sidecar summary for diagnostics export.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryDiagnosticsSafetyManifest {
    /// Desktop path that owns the `.bentodesk/manifest.json` snapshot.
    pub desktop_path: String,
    /// Number of hidden-item manifest entries.
    pub entry_count: usize,
    /// Number of zone summaries stored in the manifest.
    pub zone_count: usize,
    /// Captured manifest screen width.
    pub screen_width: u32,
    /// Captured manifest screen height.
    pub screen_height: u32,
    /// Manifest update timestamp.
    pub last_updated: String,
}

/// Icon-layout sidecar summary for diagnostics export.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveryDiagnosticsIconBackup {
    /// Whether an icon-layout backup is present in the source bundle.
    pub included: bool,
    /// Number of desktop icons captured in the sidecar.
    pub icon_count: usize,
    /// Sidecar capture timestamp.
    pub saved_at: Option<String>,
    /// Captured resolution width.
    pub resolution_width: Option<u32>,
    /// Captured resolution height.
    pub resolution_height: Option<u32>,
    /// Captured DPI scale factor.
    pub dpi: Option<f64>,
}

/// User-data sidecar diagnostics copied from validated bundle metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryDiagnosticsUserDataFile {
    /// Path relative to the selected-stack data root.
    pub relative_path: SmolStr,
    /// Explicit codec tag for this payload.
    pub codec: SmolStr,
    /// Byte length stored in bundle metadata.
    pub len_bytes: u64,
    /// Byte length measured after base64 decode.
    pub decoded_len_bytes: u64,
    /// Stable FNV-1a checksum copied from bundle metadata after validation.
    pub checksum: SmolStr,
}

/// Result of restoring selected-stack user-data sidecars.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecoveryUserDataRestoreReport {
    /// Number of files written under the selected-stack data root.
    pub restored_files: usize,
    /// Total payload bytes restored.
    pub restored_bytes: u64,
}

/// Errors surfaced by selected-stack recovery bundle operations.
#[derive(Debug)]
pub enum RecoveryBundleError {
    /// Data root could not be derived from a state file path.
    MissingDataRoot { path: PathBuf },
    /// Atomic JSON persistence failed.
    Storage(StorageError),
    /// Base64 payload failed to decode.
    Base64(Base64Error),
    /// The persisted schema is not supported by this binary.
    UnsupportedSchema { found: u32 },
    /// The persisted zones codec is not supported by this binary.
    UnsupportedCodec { found: SmolStr },
    /// The persisted config-vault codec is not supported by this binary.
    UnsupportedVaultCodec { found: SmolStr },
    /// Bundle payload length metadata does not match decoded bytes.
    LengthMismatch { expected: u64, actual: u64 },
    /// Bundle checksum does not match decoded bytes.
    ChecksumMismatch { expected: SmolStr, actual: SmolStr },
    /// Config-vault payload metadata is only partially present.
    IncompleteVaultPayload,
    /// A safety manifest snapshot has no target desktop path.
    InvalidSafetyManifestDesktop { index: usize },
    /// Bundle config-vault payload length metadata does not match decoded bytes.
    VaultLengthMismatch { expected: u64, actual: u64 },
    /// Bundle config-vault checksum does not match decoded bytes.
    VaultChecksumMismatch { expected: SmolStr, actual: SmolStr },
    /// A user-data sidecar path is not a safe relative path under data root.
    InvalidUserDataPath { path: SmolStr },
    /// A user-data sidecar codec is not supported by this binary.
    UnsupportedUserDataCodec { found: SmolStr },
    /// A user-data sidecar length metadata does not match decoded bytes.
    UserDataLengthMismatch {
        path: SmolStr,
        expected: u64,
        actual: u64,
    },
    /// A user-data sidecar checksum does not match decoded bytes.
    UserDataChecksumMismatch {
        path: SmolStr,
        expected: SmolStr,
        actual: SmolStr,
    },
    /// A user-data sidecar exceeds the migration safety limit.
    UserDataFileTooLarge {
        path: PathBuf,
        bytes: u64,
        max_bytes: u64,
    },
    /// User-data sidecars exceed the total migration safety limit.
    UserDataTotalTooLarge { bytes: u64, max_bytes: u64 },
    /// Filesystem I/O failed while collecting or restoring user-data files.
    UserDataIo {
        op: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

impl core::fmt::Display for RecoveryBundleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingDataRoot { path } => {
                write!(
                    f,
                    "recovery bundle data root unavailable for {}",
                    path.display()
                )
            }
            Self::Storage(err) => write!(f, "{err}"),
            Self::Base64(err) => write!(f, "recovery bundle base64 decode failed: {err}"),
            Self::UnsupportedSchema { found } => {
                write!(f, "unsupported recovery bundle schema version: {found}")
            }
            Self::UnsupportedCodec { found } => {
                write!(f, "unsupported recovery bundle zones codec: {found}")
            }
            Self::UnsupportedVaultCodec { found } => {
                write!(f, "unsupported recovery bundle vault codec: {found}")
            }
            Self::LengthMismatch { expected, actual } => write!(
                f,
                "recovery bundle zones length mismatch: expected {expected}, got {actual}"
            ),
            Self::ChecksumMismatch { expected, actual } => write!(
                f,
                "recovery bundle checksum mismatch: expected {expected}, got {actual}"
            ),
            Self::IncompleteVaultPayload => {
                f.write_str("recovery bundle vault payload metadata is incomplete")
            }
            Self::InvalidSafetyManifestDesktop { index } => {
                write!(
                    f,
                    "recovery bundle safety manifest #{index} has an empty desktop path"
                )
            }
            Self::VaultLengthMismatch { expected, actual } => write!(
                f,
                "recovery bundle vault length mismatch: expected {expected}, got {actual}"
            ),
            Self::VaultChecksumMismatch { expected, actual } => write!(
                f,
                "recovery bundle vault checksum mismatch: expected {expected}, got {actual}"
            ),
            Self::InvalidUserDataPath { path } => {
                write!(f, "recovery bundle user-data path is unsafe: {path}")
            }
            Self::UnsupportedUserDataCodec { found } => {
                write!(f, "unsupported recovery bundle user-data codec: {found}")
            }
            Self::UserDataLengthMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "recovery bundle user-data length mismatch at {path}: expected {expected}, got {actual}"
            ),
            Self::UserDataChecksumMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "recovery bundle user-data checksum mismatch at {path}: expected {expected}, got {actual}"
            ),
            Self::UserDataFileTooLarge {
                path,
                bytes,
                max_bytes,
            } => write!(
                f,
                "recovery bundle user-data file too large at {}: {} bytes > {} bytes",
                path.display(),
                bytes,
                max_bytes
            ),
            Self::UserDataTotalTooLarge { bytes, max_bytes } => write!(
                f,
                "recovery bundle user-data total too large: {bytes} bytes > {max_bytes} bytes"
            ),
            Self::UserDataIo { op, path, source } => {
                write!(
                    f,
                    "recovery bundle user-data {op} failed at {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl core::error::Error for RecoveryBundleError {}

impl From<StorageError> for RecoveryBundleError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<Base64Error> for RecoveryBundleError {
    fn from(value: Base64Error) -> Self {
        Self::Base64(value)
    }
}

/// Resolve the selected-stack recovery bundle path under `data_root`.
pub fn bundle_path(data_root: &Path) -> PathBuf {
    data_root
        .join(RECOVERY_BUNDLE_DIR)
        .join(RECOVERY_BUNDLE_FILENAME)
}

/// Resolve the selected-stack recovery diagnostics report path.
pub fn diagnostics_path(data_root: &Path) -> PathBuf {
    data_root
        .join(RECOVERY_BUNDLE_DIR)
        .join(RECOVERY_DIAGNOSTICS_FILENAME)
}

/// Resolve a data root from a concrete selected-stack state file path.
pub fn data_root_for_state_file(path: &Path) -> Result<PathBuf, RecoveryBundleError> {
    path.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| RecoveryBundleError::MissingDataRoot {
            path: path.to_path_buf(),
        })
}

/// Load a recovery bundle if one exists.
pub fn load_bundle(data_root: &Path) -> Result<Option<RecoveryBundle>, RecoveryBundleError> {
    storage::read_json_with_recovery(&bundle_path(data_root), "Recovery bundle").map_err(Into::into)
}

/// Write a fully-formed recovery bundle atomically.
pub fn write_bundle(data_root: &Path, bundle: &RecoveryBundle) -> Result<(), RecoveryBundleError> {
    storage::write_json_atomic(&bundle_path(data_root), bundle).map_err(Into::into)
}

fn user_data_relative_wire_path(path: &Path) -> Result<SmolStr, RecoveryBundleError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let Some(text) = part.to_str() else {
                    return Err(RecoveryBundleError::InvalidUserDataPath {
                        path: SmolStr::new(path.display().to_string()),
                    });
                };
                parts.push(text.to_string());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => {
                return Err(RecoveryBundleError::InvalidUserDataPath {
                    path: SmolStr::new(path.display().to_string()),
                });
            }
        }
    }
    let joined = parts.join("/");
    if !safe_user_data_relative_path(&joined) {
        return Err(RecoveryBundleError::InvalidUserDataPath {
            path: SmolStr::new(joined),
        });
    }
    Ok(SmolStr::new(joined))
}

fn push_user_data_file(
    data_root: &Path,
    relative_path: PathBuf,
    files: &mut Vec<RecoveryUserDataFile>,
    total_bytes: &mut u64,
) -> Result<(), RecoveryBundleError> {
    let absolute = data_root.join(&relative_path);
    let metadata = match std::fs::metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(RecoveryBundleError::UserDataIo {
                op: "metadata",
                path: absolute,
                source,
            });
        }
    };
    if !metadata.is_file() {
        return Ok(());
    }
    let len = metadata.len();
    if len > USER_DATA_MAX_FILE_BYTES {
        return Err(RecoveryBundleError::UserDataFileTooLarge {
            path: absolute,
            bytes: len,
            max_bytes: USER_DATA_MAX_FILE_BYTES,
        });
    }
    *total_bytes = total_bytes.saturating_add(len);
    if *total_bytes > USER_DATA_MAX_TOTAL_BYTES {
        return Err(RecoveryBundleError::UserDataTotalTooLarge {
            bytes: *total_bytes,
            max_bytes: USER_DATA_MAX_TOTAL_BYTES,
        });
    }
    let bytes = std::fs::read(&absolute).map_err(|source| RecoveryBundleError::UserDataIo {
        op: "read",
        path: absolute.clone(),
        source,
    })?;
    files.push(RecoveryUserDataFile {
        relative_path: user_data_relative_wire_path(&relative_path)?,
        codec: SmolStr::new_static(RECOVERY_BUNDLE_USER_DATA_CODEC),
        len_bytes: bytes.len() as u64,
        checksum: checksum_hex(&bytes),
        payload_b64: base64_encode(&bytes),
    });
    Ok(())
}

fn collect_user_data_dir_files(
    data_root: &Path,
    relative_dir: PathBuf,
    out: &mut Vec<PathBuf>,
) -> Result<(), RecoveryBundleError> {
    let absolute = data_root.join(&relative_dir);
    let read_dir = match std::fs::read_dir(&absolute) {
        Ok(read_dir) => read_dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(RecoveryBundleError::UserDataIo {
                op: "read_dir",
                path: absolute,
                source,
            });
        }
    };
    for entry in read_dir {
        let entry = entry.map_err(|source| RecoveryBundleError::UserDataIo {
            op: "read_dir_entry",
            path: absolute.clone(),
            source,
        })?;
        let path = entry.path();
        let file_name = entry.file_name();
        let child_relative = relative_dir.join(file_name);
        let metadata = entry
            .metadata()
            .map_err(|source| RecoveryBundleError::UserDataIo {
                op: "metadata",
                path,
                source,
            })?;
        if metadata.is_dir() {
            collect_user_data_dir_files(data_root, child_relative, out)?;
        } else if metadata.is_file() {
            out.push(child_relative);
        }
    }
    Ok(())
}

/// Collect selected-stack user-data sidecars that are not covered by the
/// primary `zones.bin`, config-vault payload, safety manifest, or icon backup.
pub fn collect_user_data_files(
    data_root: &Path,
) -> Result<Vec<RecoveryUserDataFile>, RecoveryBundleError> {
    let mut relative_paths = Vec::new();
    for file_name in USER_DATA_ROOT_FILES {
        relative_paths.push(PathBuf::from(file_name));
    }
    for dir_name in USER_DATA_DIRS {
        collect_user_data_dir_files(data_root, PathBuf::from(dir_name), &mut relative_paths)?;
    }
    relative_paths.sort();
    relative_paths.dedup();

    let mut files = Vec::new();
    let mut total_bytes = 0u64;
    for relative_path in relative_paths {
        push_user_data_file(data_root, relative_path, &mut files, &mut total_bytes)?;
    }
    validate_user_data_metadata(&files)?;
    Ok(files)
}

fn write_user_data_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), RecoveryBundleError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| RecoveryBundleError::UserDataIo {
            op: "create_dir_all",
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let temp_path = path.with_extension("recovery.tmp");
    std::fs::write(&temp_path, bytes).map_err(|source| RecoveryBundleError::UserDataIo {
        op: "write_temp",
        path: temp_path.clone(),
        source,
    })?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|source| RecoveryBundleError::UserDataIo {
            op: "remove_existing",
            path: path.to_path_buf(),
            source,
        })?;
    }
    std::fs::rename(&temp_path, path).map_err(|source| RecoveryBundleError::UserDataIo {
        op: "rename_temp",
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Restore validated selected-stack user-data sidecars under `data_root`.
pub fn restore_user_data_files(
    data_root: &Path,
    files: &[RecoveredUserDataFile],
) -> Result<RecoveryUserDataRestoreReport, RecoveryBundleError> {
    let mut report = RecoveryUserDataRestoreReport::default();
    for file in files {
        if !safe_user_data_relative_path(file.relative_path.as_str()) {
            return Err(RecoveryBundleError::InvalidUserDataPath {
                path: file.relative_path.clone(),
            });
        }
        let path = data_root.join(file.relative_path.as_str());
        write_user_data_file_atomic(&path, &file.bytes)?;
        report.restored_files = report.restored_files.saturating_add(1);
        report.restored_bytes = report
            .restored_bytes
            .saturating_add(file.bytes.len() as u64);
    }
    Ok(report)
}

/// Capture raw `zones.bin` bytes into an atomic recovery bundle.
pub fn refresh_zones_bundle(
    data_root: &Path,
    zones_path: &Path,
    zones_bin: &[u8],
    zone_count: u32,
) -> Result<RecoveryBundleSummary, RecoveryBundleError> {
    refresh_bundle(
        data_root,
        zones_path,
        zones_bin,
        zone_count,
        None,
        &[],
        None,
    )
}

/// Capture raw `zones.bin` bytes plus optional config-vault bytes into an
/// atomic recovery bundle.
pub fn refresh_bundle(
    data_root: &Path,
    zones_path: &Path,
    zones_bin: &[u8],
    zone_count: u32,
    vault: Option<(&Path, &[u8])>,
    safety_manifests: &[RecoverySafetyManifest],
    icon_backup: Option<SavedIconLayout>,
) -> Result<RecoveryBundleSummary, RecoveryBundleError> {
    refresh_bundle_with_user_data(
        data_root,
        zones_path,
        zones_bin,
        zone_count,
        RecoveryBundleSidecars {
            vault,
            safety_manifests,
            icon_backup,
            user_data_files: &[],
        },
    )
}

#[derive(Default)]
pub struct RecoveryBundleSidecars<'a> {
    pub vault: Option<(&'a Path, &'a [u8])>,
    pub safety_manifests: &'a [RecoverySafetyManifest],
    pub icon_backup: Option<SavedIconLayout>,
    pub user_data_files: &'a [RecoveryUserDataFile],
}

/// Capture raw `zones.bin`, optional config-vault bytes, and synchronized
/// selected-stack user-data sidecars into an atomic recovery bundle.
pub fn refresh_bundle_with_user_data(
    data_root: &Path,
    zones_path: &Path,
    zones_bin: &[u8],
    zone_count: u32,
    sidecars: RecoveryBundleSidecars<'_>,
) -> Result<RecoveryBundleSummary, RecoveryBundleError> {
    let RecoveryBundleSidecars {
        vault,
        safety_manifests,
        icon_backup,
        user_data_files,
    } = sidecars;
    validate_user_data_metadata(user_data_files)?;
    let (vault_path, vault_codec, vault_len_bytes, vault_checksum, vault_bin_b64) = match vault {
        Some((path, bytes)) => (
            Some(path.display().to_string()),
            Some(SmolStr::new_static(RECOVERY_BUNDLE_VAULT_CODEC)),
            Some(bytes.len() as u64),
            Some(checksum_hex(bytes)),
            Some(base64_encode(bytes)),
        ),
        None => (None, None, None, None, None),
    };
    let bundle = RecoveryBundle {
        schema_version: RECOVERY_BUNDLE_SCHEMA_VERSION,
        captured_at: SmolStr::new(crate::time::now_rfc3339()),
        data_root: data_root.display().to_string(),
        zones_path: zones_path.display().to_string(),
        zones_codec: SmolStr::new_static(RECOVERY_BUNDLE_ZONES_CODEC),
        zone_count,
        zones_len_bytes: zones_bin.len() as u64,
        zones_checksum: checksum_hex(zones_bin),
        zones_bin_b64: base64_encode(zones_bin),
        vault_path,
        vault_codec,
        vault_len_bytes,
        vault_checksum,
        vault_bin_b64,
        safety_manifests: safety_manifests.to_vec(),
        icon_backup,
        user_data_files: user_data_files.to_vec(),
    };
    write_bundle(data_root, &bundle)?;
    Ok(summary_for_bundle(data_root, &bundle))
}

/// Load and validate the bundled `zones.bin` bytes.
pub fn recover_zones_payload(
    data_root: &Path,
) -> Result<Option<RecoveredZonesPayload>, RecoveryBundleError> {
    let Some(bundle) = load_bundle(data_root)? else {
        return Ok(None);
    };
    let zones_bin = validate_and_decode_zones(&bundle)?;
    let vault = validate_and_decode_vault(&bundle)?;
    validate_safety_manifests(&bundle.safety_manifests)?;
    let user_data_files = validate_and_decode_user_data_files(&bundle.user_data_files)?;
    let summary = summary_for_bundle(data_root, &bundle);
    Ok(Some(RecoveredZonesPayload {
        zones_bin,
        vault,
        safety_manifests: bundle.safety_manifests,
        icon_backup: bundle.icon_backup,
        user_data_files,
        summary,
    }))
}

/// Build a validated diagnostics report for the latest recovery bundle.
pub fn diagnostics_report(
    data_root: &Path,
) -> Result<Option<RecoveryDiagnosticsReport>, RecoveryBundleError> {
    let Some(bundle) = load_bundle(data_root)? else {
        return Ok(None);
    };
    let zones_bin = validate_and_decode_zones(&bundle)?;
    let vault_payload = validate_and_decode_vault(&bundle)?;
    validate_safety_manifests(&bundle.safety_manifests)?;
    let user_data_payloads = validate_and_decode_user_data_files(&bundle.user_data_files)?;
    Ok(Some(report_for_bundle(
        data_root,
        &bundle,
        &zones_bin,
        &vault_payload,
        &user_data_payloads,
    )))
}

/// Export a validated recovery diagnostics report beside the bundle JSON.
pub fn export_diagnostics_report(
    data_root: &Path,
) -> Result<Option<RecoveryDiagnosticsReport>, RecoveryBundleError> {
    let Some(report) = diagnostics_report(data_root)? else {
        return Ok(None);
    };
    storage::write_json_atomic(&diagnostics_path(data_root), &report)?;
    Ok(Some(report))
}

fn validate_and_decode_zones(bundle: &RecoveryBundle) -> Result<Vec<u8>, RecoveryBundleError> {
    if !(RECOVERY_BUNDLE_MIN_SCHEMA_VERSION..=RECOVERY_BUNDLE_SCHEMA_VERSION)
        .contains(&bundle.schema_version)
    {
        return Err(RecoveryBundleError::UnsupportedSchema {
            found: bundle.schema_version,
        });
    }
    if bundle.zones_codec.as_str() != RECOVERY_BUNDLE_ZONES_CODEC {
        return Err(RecoveryBundleError::UnsupportedCodec {
            found: bundle.zones_codec.clone(),
        });
    }
    let decoded = base64_decode(&bundle.zones_bin_b64)?;
    let actual_len = decoded.len() as u64;
    if actual_len != bundle.zones_len_bytes {
        return Err(RecoveryBundleError::LengthMismatch {
            expected: bundle.zones_len_bytes,
            actual: actual_len,
        });
    }
    let actual_checksum = checksum_hex(&decoded);
    if actual_checksum != bundle.zones_checksum {
        return Err(RecoveryBundleError::ChecksumMismatch {
            expected: bundle.zones_checksum.clone(),
            actual: actual_checksum,
        });
    }
    Ok(decoded)
}

fn validate_and_decode_vault(
    bundle: &RecoveryBundle,
) -> Result<Option<RecoveredVaultPayload>, RecoveryBundleError> {
    match (
        &bundle.vault_path,
        &bundle.vault_codec,
        bundle.vault_len_bytes,
        &bundle.vault_checksum,
        &bundle.vault_bin_b64,
    ) {
        (None, None, None, None, None) => Ok(None),
        (Some(path), Some(codec), Some(expected_len), Some(expected_checksum), Some(encoded)) => {
            if codec.as_str() != RECOVERY_BUNDLE_VAULT_CODEC {
                return Err(RecoveryBundleError::UnsupportedVaultCodec {
                    found: codec.clone(),
                });
            }
            let decoded = base64_decode(encoded)?;
            let actual_len = decoded.len() as u64;
            if actual_len != expected_len {
                return Err(RecoveryBundleError::VaultLengthMismatch {
                    expected: expected_len,
                    actual: actual_len,
                });
            }
            let actual_checksum = checksum_hex(&decoded);
            if actual_checksum.as_str() != expected_checksum.as_str() {
                return Err(RecoveryBundleError::VaultChecksumMismatch {
                    expected: expected_checksum.clone(),
                    actual: actual_checksum,
                });
            }
            Ok(Some(RecoveredVaultPayload {
                vault_bin: decoded,
                vault_path: Some(path.clone()),
            }))
        }
        _ => Err(RecoveryBundleError::IncompleteVaultPayload),
    }
}

fn safe_user_data_relative_path(relative_path: &str) -> bool {
    let path = Path::new(relative_path);
    if path.is_absolute() {
        return false;
    }
    let mut components = 0usize;
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part_text = part.to_string_lossy();
                if part_text.is_empty() {
                    return false;
                }
                if components == 0 && part_text.eq_ignore_ascii_case(RECOVERY_BUNDLE_DIR) {
                    return false;
                }
                components += 1;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => return false,
        }
    }
    components > 0
}

fn validate_user_data_metadata(files: &[RecoveryUserDataFile]) -> Result<(), RecoveryBundleError> {
    let mut total = 0u64;
    for file in files {
        if !safe_user_data_relative_path(file.relative_path.as_str()) {
            return Err(RecoveryBundleError::InvalidUserDataPath {
                path: file.relative_path.clone(),
            });
        }
        if file.codec.as_str() != RECOVERY_BUNDLE_USER_DATA_CODEC {
            return Err(RecoveryBundleError::UnsupportedUserDataCodec {
                found: file.codec.clone(),
            });
        }
        if file.len_bytes > USER_DATA_MAX_FILE_BYTES {
            return Err(RecoveryBundleError::UserDataFileTooLarge {
                path: PathBuf::from(file.relative_path.as_str()),
                bytes: file.len_bytes,
                max_bytes: USER_DATA_MAX_FILE_BYTES,
            });
        }
        total = total.saturating_add(file.len_bytes);
        if total > USER_DATA_MAX_TOTAL_BYTES {
            return Err(RecoveryBundleError::UserDataTotalTooLarge {
                bytes: total,
                max_bytes: USER_DATA_MAX_TOTAL_BYTES,
            });
        }
    }
    Ok(())
}

fn validate_and_decode_user_data_file(
    file: &RecoveryUserDataFile,
) -> Result<RecoveredUserDataFile, RecoveryBundleError> {
    validate_user_data_metadata(core::slice::from_ref(file))?;
    let decoded = base64_decode(&file.payload_b64)?;
    let actual_len = decoded.len() as u64;
    if actual_len != file.len_bytes {
        return Err(RecoveryBundleError::UserDataLengthMismatch {
            path: file.relative_path.clone(),
            expected: file.len_bytes,
            actual: actual_len,
        });
    }
    let actual_checksum = checksum_hex(&decoded);
    if actual_checksum != file.checksum {
        return Err(RecoveryBundleError::UserDataChecksumMismatch {
            path: file.relative_path.clone(),
            expected: file.checksum.clone(),
            actual: actual_checksum,
        });
    }
    Ok(RecoveredUserDataFile {
        relative_path: file.relative_path.clone(),
        bytes: decoded,
        checksum: file.checksum.clone(),
    })
}

fn validate_and_decode_user_data_files(
    files: &[RecoveryUserDataFile],
) -> Result<Vec<RecoveredUserDataFile>, RecoveryBundleError> {
    validate_user_data_metadata(files)?;
    files
        .iter()
        .map(validate_and_decode_user_data_file)
        .collect()
}

fn validate_safety_manifests(
    manifests: &[RecoverySafetyManifest],
) -> Result<(), RecoveryBundleError> {
    for (index, snapshot) in manifests.iter().enumerate() {
        if snapshot.desktop_path.trim().is_empty() {
            return Err(RecoveryBundleError::InvalidSafetyManifestDesktop { index });
        }
    }
    Ok(())
}

fn summary_for_bundle(data_root: &Path, bundle: &RecoveryBundle) -> RecoveryBundleSummary {
    let user_data_len_bytes = bundle
        .user_data_files
        .iter()
        .fold(0u64, |total, file| total.saturating_add(file.len_bytes));
    RecoveryBundleSummary {
        captured_at: bundle.captured_at.clone(),
        zone_count: bundle.zone_count,
        zones_len_bytes: bundle.zones_len_bytes,
        zones_checksum: bundle.zones_checksum.clone(),
        vault_included: bundle.vault_bin_b64.is_some(),
        vault_len_bytes: bundle.vault_len_bytes,
        vault_checksum: bundle.vault_checksum.clone(),
        safety_manifest_count: bundle.safety_manifests.len(),
        icon_backup_included: bundle.icon_backup.is_some(),
        user_data_file_count: bundle.user_data_files.len(),
        user_data_len_bytes,
        path: bundle_path(data_root),
    }
}

fn report_for_bundle(
    data_root: &Path,
    bundle: &RecoveryBundle,
    zones_bin: &[u8],
    vault_payload: &Option<RecoveredVaultPayload>,
    user_data_payloads: &[RecoveredUserDataFile],
) -> RecoveryDiagnosticsReport {
    let report_path = diagnostics_path(data_root);
    let safety_manifests = bundle
        .safety_manifests
        .iter()
        .map(|snapshot| RecoveryDiagnosticsSafetyManifest {
            desktop_path: snapshot.desktop_path.clone(),
            entry_count: snapshot.manifest.entries.len(),
            zone_count: snapshot.manifest.zones.len(),
            screen_width: snapshot.manifest.screen_width,
            screen_height: snapshot.manifest.screen_height,
            last_updated: snapshot.manifest.last_updated.clone(),
        })
        .collect();
    RecoveryDiagnosticsReport {
        schema_version: 1,
        generated_at: SmolStr::new(crate::time::now_rfc3339()),
        data_root: data_root.display().to_string(),
        diagnostics_path: report_path.display().to_string(),
        bundle_path: bundle_path(data_root).display().to_string(),
        captured_at: bundle.captured_at.clone(),
        zones: RecoveryDiagnosticsBlob {
            codec: bundle.zones_codec.clone(),
            zone_count: bundle.zone_count,
            len_bytes: bundle.zones_len_bytes,
            decoded_len_bytes: zones_bin.len() as u64,
            checksum: bundle.zones_checksum.clone(),
            path: bundle.zones_path.clone(),
        },
        vault: RecoveryDiagnosticsOptionalBlob {
            included: vault_payload.is_some(),
            codec: bundle.vault_codec.clone(),
            len_bytes: bundle.vault_len_bytes,
            decoded_len_bytes: vault_payload
                .as_ref()
                .map(|payload| payload.vault_bin.len() as u64),
            checksum: bundle.vault_checksum.clone(),
            path: bundle.vault_path.clone(),
        },
        safety_manifests,
        icon_backup: diagnostics_icon_backup(&bundle.icon_backup),
        user_data_files: bundle
            .user_data_files
            .iter()
            .zip(user_data_payloads.iter())
            .map(|(file, payload)| RecoveryDiagnosticsUserDataFile {
                relative_path: file.relative_path.clone(),
                codec: file.codec.clone(),
                len_bytes: file.len_bytes,
                decoded_len_bytes: payload.bytes.len() as u64,
                checksum: payload.checksum.clone(),
            })
            .collect(),
    }
}

fn diagnostics_icon_backup(icon_backup: &Option<SavedIconLayout>) -> RecoveryDiagnosticsIconBackup {
    match icon_backup {
        Some(layout) => RecoveryDiagnosticsIconBackup {
            included: true,
            icon_count: layout.icons.len(),
            saved_at: Some(layout.saved_at.clone()),
            resolution_width: Some(layout.resolution.width),
            resolution_height: Some(layout.resolution.height),
            dpi: Some(layout.dpi),
        },
        None => RecoveryDiagnosticsIconBackup {
            included: false,
            icon_count: 0,
            saved_at: None,
            resolution_width: None,
            resolution_height: None,
            dpi: None,
        },
    }
}

fn checksum_hex(bytes: &[u8]) -> SmolStr {
    let mut hash = FNV64_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV64_PRIME);
    }
    SmolStr::new(format!("{hash:016x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "bento-nano-recovery-bundle-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn missing_bundle_returns_none() {
        let dir = scratch_dir("missing");
        let loaded = load_bundle(&dir).expect("load missing bundle");
        assert!(loaded.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bundle_roundtrip_preserves_validated_zones_payload() {
        let dir = scratch_dir("roundtrip");
        let zones_path = dir.join("zones.bin");
        let zones_bin = b"BNTZ-test-payload";

        let summary =
            refresh_zones_bundle(&dir, &zones_path, zones_bin, 3).expect("capture bundle");
        assert_eq!(summary.zone_count, 3);
        assert_eq!(summary.zones_len_bytes, zones_bin.len() as u64);
        assert!(summary.path.exists(), "bundle json must exist");

        let recovered = recover_zones_payload(&dir)
            .expect("recover bundle")
            .expect("bundle present");
        assert_eq!(recovered.zones_bin, zones_bin);
        assert!(recovered.vault.is_none());
        assert_eq!(recovered.summary.zone_count, 3);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bundle_roundtrip_preserves_validated_zones_and_vault_payload() {
        let dir = scratch_dir("roundtrip-vault");
        let zones_path = dir.join("zones.bin");
        let vault_path = dir.join("vault.bin");
        let zones_bin = b"BNTZ-test-payload";
        let vault_bin = br#"{"version":1,"mode":"None","payload":"settings"}"#;

        let summary = refresh_bundle(
            &dir,
            &zones_path,
            zones_bin,
            2,
            Some((&vault_path, vault_bin)),
            &[],
            None,
        )
        .expect("capture bundle");
        assert_eq!(summary.zone_count, 2);
        assert!(summary.vault_included);
        assert_eq!(summary.vault_len_bytes, Some(vault_bin.len() as u64));

        let recovered = recover_zones_payload(&dir)
            .expect("recover bundle")
            .expect("bundle present");
        assert_eq!(recovered.zones_bin, zones_bin);
        let recovered_vault = recovered.vault.expect("vault payload");
        assert_eq!(recovered_vault.vault_bin, vault_bin);
        let expected_vault_path = vault_path.display().to_string();
        assert_eq!(
            recovered_vault.vault_path.as_deref(),
            Some(expected_vault_path.as_str())
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bundle_roundtrip_preserves_selected_stack_user_data_sidecars() {
        let dir = scratch_dir("roundtrip-user-data");
        let zones_path = dir.join("zones.bin");
        let timeline_dir = dir.join("timeline");
        let snapshot_dir = dir.join("snapshots");
        std::fs::create_dir_all(&timeline_dir).expect("timeline dir");
        std::fs::create_dir_all(&snapshot_dir).expect("snapshot dir");
        std::fs::write(dir.join("rules.json"), br#"[{"id":"rule-1"}]"#).expect("rules");
        std::fs::write(timeline_dir.join("checkpoint-1.json"), br#"{"id":"cp-1"}"#)
            .expect("timeline");
        std::fs::write(snapshot_dir.join("snap-1.json"), br#"{"id":"snap-1"}"#).expect("snapshot");

        let user_data_files = collect_user_data_files(&dir).expect("collect user data");
        assert_eq!(user_data_files.len(), 3);

        let summary = refresh_bundle_with_user_data(
            &dir,
            &zones_path,
            b"zones",
            1,
            RecoveryBundleSidecars {
                user_data_files: &user_data_files,
                ..RecoveryBundleSidecars::default()
            },
        )
        .expect("capture bundle with user data");
        assert_eq!(summary.user_data_file_count, 3);
        assert!(summary.user_data_len_bytes > 0);

        std::fs::remove_file(dir.join("rules.json")).expect("remove rules");
        std::fs::remove_file(timeline_dir.join("checkpoint-1.json")).expect("remove timeline");
        std::fs::remove_file(snapshot_dir.join("snap-1.json")).expect("remove snapshot");

        let payload = recover_zones_payload(&dir)
            .expect("recover bundle")
            .expect("bundle present");
        assert_eq!(payload.user_data_files.len(), 3);
        let report =
            restore_user_data_files(&dir, &payload.user_data_files).expect("restore user data");
        assert_eq!(report.restored_files, 3);
        assert_eq!(
            std::fs::read(dir.join("rules.json")).expect("restored rules"),
            br#"[{"id":"rule-1"}]"#
        );
        assert_eq!(
            std::fs::read(timeline_dir.join("checkpoint-1.json")).expect("restored timeline"),
            br#"{"id":"cp-1"}"#
        );
        assert_eq!(
            std::fs::read(snapshot_dir.join("snap-1.json")).expect("restored snapshot"),
            br#"{"id":"snap-1"}"#
        );

        let diagnostics = diagnostics_report(&dir)
            .expect("diagnostics")
            .expect("report present");
        assert_eq!(diagnostics.user_data_files.len(), 3);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn user_data_restore_rejects_path_traversal() {
        let dir = scratch_dir("user-data-traversal");
        let payload = RecoveredUserDataFile {
            relative_path: SmolStr::new_static("../escape.json"),
            bytes: b"bad".to_vec(),
            checksum: checksum_hex(b"bad"),
        };
        let err = restore_user_data_files(&dir, &[payload]).expect_err("unsafe path rejected");
        assert!(matches!(
            err,
            RecoveryBundleError::InvalidUserDataPath { .. }
        ));
        assert!(!dir.join("..").join("escape.json").exists());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn checksum_mismatch_rejects_payload_before_shell_restore() {
        let dir = scratch_dir("tamper");
        let zones_path = dir.join("zones.bin");
        refresh_zones_bundle(&dir, &zones_path, b"good-payload", 1).expect("capture bundle");

        let mut bundle = load_bundle(&dir)
            .expect("load bundle")
            .expect("bundle exists");
        bundle.zones_bin_b64 = base64_encode(b"tampered-payload");
        write_bundle(&dir, &bundle).expect("write tampered bundle");

        let err = recover_zones_payload(&dir).expect_err("tamper must fail");
        assert!(
            matches!(err, RecoveryBundleError::LengthMismatch { .. })
                || matches!(err, RecoveryBundleError::ChecksumMismatch { .. })
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn incomplete_vault_payload_rejects_before_shell_restore() {
        let dir = scratch_dir("incomplete-vault");
        let zones_path = dir.join("zones.bin");
        refresh_zones_bundle(&dir, &zones_path, b"good-payload", 1).expect("capture bundle");

        let mut bundle = load_bundle(&dir)
            .expect("load bundle")
            .expect("bundle exists");
        bundle.vault_bin_b64 = Some(base64_encode(b"vault"));
        write_bundle(&dir, &bundle).expect("write incomplete bundle");

        let err = recover_zones_payload(&dir).expect_err("incomplete vault must fail");
        assert!(matches!(err, RecoveryBundleError::IncompleteVaultPayload));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bundle_roundtrip_preserves_safety_manifest_snapshots() {
        let dir = scratch_dir("manifest");
        let zones_path = dir.join("zones.bin");
        let desktop_path = dir.join("Desktop");
        let manifest = SafetyManifest {
            schema_version: crate::stealth::MANIFEST_SCHEMA_VERSION.to_string(),
            entries: vec![crate::stealth::ManifestEntry {
                original_path: desktop_path.join("doc.txt").display().to_string(),
                hidden_path: desktop_path
                    .join(".bentodesk")
                    .join("1")
                    .join("doc.txt")
                    .display()
                    .to_string(),
                zone_id: "1".to_string(),
                file_size_bytes: 42,
                hidden_at: "2026-05-08T00:00:00Z".to_string(),
                display_name: "doc.txt".to_string(),
                icon_x: Some(10),
                icon_y: Some(20),
                file_type: "File".to_string(),
            }],
            zones: Vec::new(),
            screen_width: 1920,
            screen_height: 1080,
            last_updated: "2026-05-08T00:00:00Z".to_string(),
        };
        let snapshot = RecoverySafetyManifest {
            desktop_path: desktop_path.display().to_string(),
            manifest,
        };

        let summary = refresh_bundle(&dir, &zones_path, b"zones", 1, None, &[snapshot], None)
            .expect("capture manifest bundle");
        assert_eq!(summary.safety_manifest_count, 1);

        let recovered = recover_zones_payload(&dir)
            .expect("recover bundle")
            .expect("bundle present");
        assert_eq!(recovered.safety_manifests.len(), 1);
        assert_eq!(
            recovered.safety_manifests[0].manifest.entries[0].display_name,
            "doc.txt"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bundle_roundtrip_preserves_icon_backup_sidecar() {
        let dir = scratch_dir("icon-backup");
        let zones_path = dir.join("zones.bin");
        let icon_backup = SavedIconLayout {
            icons: vec![crate::icon_positions::IconPosition {
                name: "doc.txt".to_string(),
                x: 10,
                y: 20,
            }],
            saved_at: "2026-05-08T00:00:00Z".to_string(),
            resolution: crate::icon_positions::Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
        };

        let summary = refresh_bundle(&dir, &zones_path, b"zones", 1, None, &[], Some(icon_backup))
            .expect("capture icon backup bundle");
        assert!(summary.icon_backup_included);

        let recovered = recover_zones_payload(&dir)
            .expect("recover bundle")
            .expect("bundle present");
        let recovered_icon_backup = recovered.icon_backup.expect("icon backup");
        assert_eq!(recovered_icon_backup.icons.len(), 1);
        assert_eq!(recovered_icon_backup.icons[0].name, "doc.txt");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn diagnostics_export_preserves_validated_bundle_metadata() {
        let dir = scratch_dir("diagnostics");
        let zones_path = dir.join("zones.bin");
        let vault_path = dir.join("vault.bin");
        let desktop_path = dir.join("Desktop");
        let zones_bin = b"BNTZ-diagnostics-payload";
        let vault_bin = br#"{"version":1,"mode":"None","payload":"settings"}"#;
        let manifest = SafetyManifest {
            schema_version: crate::stealth::MANIFEST_SCHEMA_VERSION.to_string(),
            entries: vec![crate::stealth::ManifestEntry {
                original_path: desktop_path.join("doc.txt").display().to_string(),
                hidden_path: desktop_path
                    .join(".bentodesk")
                    .join("1")
                    .join("doc.txt")
                    .display()
                    .to_string(),
                zone_id: "1".to_string(),
                file_size_bytes: 42,
                hidden_at: "2026-05-08T00:00:00Z".to_string(),
                display_name: "doc.txt".to_string(),
                icon_x: Some(10),
                icon_y: Some(20),
                file_type: "File".to_string(),
            }],
            zones: Vec::new(),
            screen_width: 1920,
            screen_height: 1080,
            last_updated: "2026-05-08T00:00:00Z".to_string(),
        };
        let snapshot = RecoverySafetyManifest {
            desktop_path: desktop_path.display().to_string(),
            manifest,
        };
        let icon_backup = SavedIconLayout {
            icons: vec![crate::icon_positions::IconPosition {
                name: "doc.txt".to_string(),
                x: 10,
                y: 20,
            }],
            saved_at: "2026-05-08T00:00:00Z".to_string(),
            resolution: crate::icon_positions::Resolution {
                width: 1920,
                height: 1080,
            },
            dpi: 1.0,
        };

        refresh_bundle(
            &dir,
            &zones_path,
            zones_bin,
            4,
            Some((&vault_path, vault_bin)),
            &[snapshot],
            Some(icon_backup),
        )
        .expect("capture diagnostics bundle");

        let report = export_diagnostics_report(&dir)
            .expect("export diagnostics")
            .expect("report exists");
        assert_eq!(report.schema_version, 1);
        assert_eq!(report.zones.zone_count, 4);
        assert_eq!(report.zones.len_bytes, zones_bin.len() as u64);
        assert_eq!(report.zones.decoded_len_bytes, zones_bin.len() as u64);
        assert!(report.vault.included);
        assert_eq!(report.vault.decoded_len_bytes, Some(vault_bin.len() as u64));
        assert_eq!(report.safety_manifests.len(), 1);
        assert_eq!(report.safety_manifests[0].entry_count, 1);
        assert!(report.icon_backup.included);
        assert_eq!(report.icon_backup.icon_count, 1);
        assert!(
            diagnostics_path(&dir).exists(),
            "diagnostics json must exist"
        );

        let raw = std::fs::read(diagnostics_path(&dir)).expect("read diagnostics");
        let persisted: RecoveryDiagnosticsReport =
            serde_json::from_slice(&raw).expect("parse diagnostics");
        assert_eq!(persisted.zones.checksum, report.zones.checksum);
        assert_eq!(
            persisted.bundle_path,
            bundle_path(&dir).display().to_string()
        );

        let _ = std::fs::remove_dir_all(dir);
    }
}
