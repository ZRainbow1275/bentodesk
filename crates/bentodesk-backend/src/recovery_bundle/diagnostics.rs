//! Serializable recovery diagnostics model.

use super::*;

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
