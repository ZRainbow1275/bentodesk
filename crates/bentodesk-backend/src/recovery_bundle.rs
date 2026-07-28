//! Selected-stack synchronized recovery bundle.
//!
//! The Tauri baseline keeps a coherent `recovery_bundle.json` beside the
//! normal per-file backups. Native cannot reuse Tauri `AppHandle`,
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
pub const RECOVERY_BUNDLE_ZONES_CODEC: &str = "bentodesk-platform::storage/zones.bin:v5";
/// Codec tag for the bundled selected-stack config vault payload.
pub const RECOVERY_BUNDLE_VAULT_CODEC: &str = "bentodesk-backend::config_vault/vault.bin:v1";
/// Codec tag for synchronized selected-stack user-data sidecars.
pub const RECOVERY_BUNDLE_USER_DATA_CODEC: &str = "bentodesk-user-data/raw:v1";

const RECOVERY_BUNDLE_DIR: &str = "recovery";
const RECOVERY_BUNDLE_FILENAME: &str = "recovery_bundle.json";
const RECOVERY_DIAGNOSTICS_FILENAME: &str = "recovery_diagnostics.json";
const FNV64_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV64_PRIME: u64 = 0x0000_0100_0000_01b3;
const USER_DATA_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const USER_DATA_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const USER_DATA_ROOT_FILES: &[&str] = &["rules.json", "rules.json.bak"];
const USER_DATA_DIRS: &[&str] = &["timeline", "snapshots", "backups", "themes", "plugins"];

mod diagnostics;
mod user_data;

pub use diagnostics::{
    RecoveryDiagnosticsBlob, RecoveryDiagnosticsIconBackup, RecoveryDiagnosticsOptionalBlob,
    RecoveryDiagnosticsReport, RecoveryDiagnosticsSafetyManifest, RecoveryDiagnosticsUserDataFile,
};

pub use user_data::{
    RecoveredUserDataFile, RecoveryUserDataFile, RecoveryUserDataRestoreReport,
    collect_user_data_files, restore_user_data_files,
};
use user_data::{validate_and_decode_user_data_files, validate_user_data_metadata};

/// JSON persisted recovery bundle for the selected stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverySafetyManifest {
    /// Desktop path that owns the `.bentodesk/manifest.json` snapshot.
    pub desktop_path: String,
    /// Full selected-stack safety manifest snapshot.
    pub manifest: SafetyManifest,
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
mod tests;
