//! Native shell owner: `startup_backup_types`.

use super::*;

#[derive(Debug)]
pub(super) enum SettingsBackupError {
    MissingVaultParent,
    NoBackups,
    BackupNotFound(SmolStr),
    Vault(bento_nano_backend::config_vault::VaultError),
    Io {
        op: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

impl core::fmt::Display for SettingsBackupError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingVaultParent => f.write_str("vault path has no parent directory"),
            Self::NoBackups => f.write_str("no settings backups found"),
            Self::BackupNotFound(backup_id) => {
                write!(f, "settings backup not found: {backup_id}")
            }
            Self::Vault(e) => write!(f, "vault backup failed: {e}"),
            Self::Io { op, path, source } => {
                write!(f, "{op} failed at {}: {source}", path.display())
            }
        }
    }
}

impl core::error::Error for SettingsBackupError {}

impl From<bento_nano_backend::config_vault::VaultError> for SettingsBackupError {
    fn from(value: bento_nano_backend::config_vault::VaultError) -> Self {
        Self::Vault(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupLayoutLoadSource {
    SelectedZonesBin,
    LegacyLayoutJson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StartupLayoutLoadOutcome {
    pub(super) source: StartupLayoutLoadSource,
    pub(super) zone_count: usize,
    pub(super) persisted: bool,
}

#[derive(Debug)]
pub(super) enum StartupLayoutLoadError {
    MissingStateDir,
    Layout(LayoutError),
    Platform(PlatformError),
}

impl core::fmt::Display for StartupLayoutLoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingStateDir => f.write_str("zones.bin path has no parent directory"),
            Self::Layout(error) => write!(f, "legacy layout migration failed: {error}"),
            Self::Platform(error) => write!(f, "{error}"),
        }
    }
}

impl core::error::Error for StartupLayoutLoadError {}

impl From<LayoutError> for StartupLayoutLoadError {
    fn from(value: LayoutError) -> Self {
        Self::Layout(value)
    }
}

impl From<PlatformError> for StartupLayoutLoadError {
    fn from(value: PlatformError) -> Self {
        Self::Platform(value)
    }
}

pub(super) fn legacy_layout_path_for_zones_path(
    zones_path: &Path,
) -> Result<PathBuf, StartupLayoutLoadError> {
    zones_path
        .parent()
        .map(|dir| dir.join("layout.json"))
        .ok_or(StartupLayoutLoadError::MissingStateDir)
}

pub(super) fn legacy_layout_backup_path(layout_path: &Path) -> PathBuf {
    let file_name = layout_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("layout.json"));
    layout_path.with_file_name(format!("{file_name}.bak"))
}

pub(super) fn load_legacy_layout_with_backup(
    layout_path: &Path,
) -> Result<LayoutData, LayoutError> {
    let backup_path = legacy_layout_backup_path(layout_path);
    if !layout_path.exists() && backup_path.exists() {
        return LayoutData::load(&backup_path);
    }
    match LayoutData::load(layout_path) {
        Ok(layout) => Ok(layout),
        Err(primary_error) => {
            if backup_path.exists()
                && let Ok(layout) = LayoutData::load(&backup_path)
            {
                return Ok(layout);
            }
            Err(primary_error)
        }
    }
}

pub(super) fn install_startup_zones(root: &AppRoot, zones_path: &Path, zones: ZoneList) -> usize {
    let count = zones.len();
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.to_path_buf();
        app.zones = zones;
        bump_next_zone_id_from_zones(&app);
        app.dirty.set(false);
    }
    count
}

pub(super) fn startup_layout_viewport(root: &AppRoot) -> bento_nano_style::Size {
    let viewport = root.app.borrow().viewport;
    if viewport.width >= 1.0 && viewport.height >= 1.0 {
        return viewport;
    }
    let (width, height) = bento_nano_platform::default_size(WindowKind::Main);
    bento_nano_style::Size {
        width: width as f32,
        height: height as f32,
    }
}

pub(super) fn load_startup_zones_or_migrate_legacy(
    root: &AppRoot,
    zones_path: &Path,
) -> Result<Option<StartupLayoutLoadOutcome>, StartupLayoutLoadError> {
    {
        let mut app = root.app.borrow_mut();
        app.zones_path = zones_path.to_path_buf();
    }
    let legacy_layout_path = legacy_layout_path_for_zones_path(zones_path)?;
    let legacy_backup_path = legacy_layout_backup_path(&legacy_layout_path);
    let legacy_layout_available = legacy_layout_path.exists() || legacy_backup_path.exists();
    match storage::read_zones(zones_path) {
        Ok(zones) if !zones.is_empty() => {
            let zone_count = install_startup_zones(root, zones_path, zones);
            return Ok(Some(StartupLayoutLoadOutcome {
                source: StartupLayoutLoadSource::SelectedZonesBin,
                zone_count,
                persisted: false,
            }));
        }
        Ok(zones) if !legacy_layout_available => {
            install_startup_zones(root, zones_path, zones);
            return Ok(None);
        }
        Ok(_) => {}
        Err(PlatformError::Storage(_)) => {
            storage::quarantine_corrupt(zones_path)?;
        }
        Err(error) => return Err(error.into()),
    }

    if !legacy_layout_available {
        return Ok(None);
    }
    let legacy_layout = load_legacy_layout_with_backup(&legacy_layout_path)?;
    if legacy_layout.zones.is_empty() {
        return Ok(None);
    }
    let viewport = startup_layout_viewport(root);
    let migrated_zones = zone_list_from_bento_zones(&legacy_layout.zones, viewport);
    let zone_count = install_startup_zones(root, zones_path, migrated_zones);
    {
        let app = root.app.borrow();
        storage::write_zones_atomic(&app.zones_path, &app.zones)?;
    }
    Ok(Some(StartupLayoutLoadOutcome {
        source: StartupLayoutLoadSource::LegacyLayoutJson,
        zone_count,
        persisted: true,
    }))
}

pub(super) fn run_startup_layout_load_or_migrate(root: &AppRoot, zones_path: &Path) {
    match load_startup_zones_or_migrate_legacy(root, zones_path) {
        Ok(Some(outcome)) => match outcome.source {
            StartupLayoutLoadSource::SelectedZonesBin => {
                tracing::info!(
                    target: "bentodesk::layout",
                    path = %zones_path.display(),
                    zones = outcome.zone_count,
                    "loaded selected-stack zones.bin at startup"
                );
                log_static(
                    format!(
                        "layout: startup load source=SelectedZonesBin zones={} persisted={}\n",
                        outcome.zone_count, outcome.persisted
                    )
                    .as_str(),
                );
            }
            StartupLayoutLoadSource::LegacyLayoutJson => {
                tracing::info!(
                    target: "bentodesk::layout",
                    path = %legacy_layout_path_for_zones_path(zones_path)
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|_| String::from("layout.json")),
                    zones = outcome.zone_count,
                    persisted = outcome.persisted,
                    "migrated legacy Tauri layout.json into selected-stack zones.bin"
                );
                log_static(
                    format!(
                        "layout: migrated legacy Tauri layout.json zones={} persisted={}\n",
                        outcome.zone_count, outcome.persisted
                    )
                    .as_str(),
                );
            }
        },
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::layout",
                path = %zones_path.display(),
                error = %error,
                "startup layout load/migration failed; shell will continue with recoverable empty state"
            );
            log_static(
                format!(
                    "layout: startup load/migration failed path={} error={}\n",
                    zones_path.display(),
                    error
                )
                .as_str(),
            );
        }
    }
}

#[derive(Debug)]
pub(super) enum RecoveryBundleShellError {
    MissingZonesPath,
    MissingVault,
    MissingBundle,
    ZoneCountOverflow(usize),
    Backend(bento_nano_backend::recovery_bundle::RecoveryBundleError),
    Vault(bento_nano_backend::config_vault::VaultError),
    Stealth(bento_nano_backend::stealth::StealthError),
    IconPosition(bento_nano_backend::icon_positions::IconPositionError),
    Platform(PlatformError),
    Io {
        op: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

impl core::fmt::Display for RecoveryBundleShellError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingZonesPath => f.write_str("zones.bin path is not initialised"),
            Self::MissingVault => f.write_str("config vault is not initialised"),
            Self::MissingBundle => f.write_str("no recovery bundle found"),
            Self::ZoneCountOverflow(count) => {
                write!(
                    f,
                    "zone count exceeds recovery bundle metadata limit: {count}"
                )
            }
            Self::Backend(error) => write!(f, "{error}"),
            Self::Vault(error) => write!(f, "recovery bundle vault restore failed: {error}"),
            Self::Stealth(error) => write!(f, "recovery bundle safety manifest failed: {error}"),
            Self::IconPosition(error) => {
                write!(f, "recovery bundle icon backup failed: {error}")
            }
            Self::Platform(error) => write!(f, "{error}"),
            Self::Io { op, path, source } => {
                write!(f, "{op} failed at {}: {source}", path.display())
            }
        }
    }
}

impl core::error::Error for RecoveryBundleShellError {}

impl From<bento_nano_backend::recovery_bundle::RecoveryBundleError> for RecoveryBundleShellError {
    fn from(value: bento_nano_backend::recovery_bundle::RecoveryBundleError) -> Self {
        Self::Backend(value)
    }
}

impl From<bento_nano_backend::config_vault::VaultError> for RecoveryBundleShellError {
    fn from(value: bento_nano_backend::config_vault::VaultError) -> Self {
        Self::Vault(value)
    }
}

impl From<bento_nano_backend::stealth::StealthError> for RecoveryBundleShellError {
    fn from(value: bento_nano_backend::stealth::StealthError) -> Self {
        Self::Stealth(value)
    }
}

impl From<bento_nano_backend::icon_positions::IconPositionError> for RecoveryBundleShellError {
    fn from(value: bento_nano_backend::icon_positions::IconPositionError) -> Self {
        Self::IconPosition(value)
    }
}

impl From<PlatformError> for RecoveryBundleShellError {
    fn from(value: PlatformError) -> Self {
        Self::Platform(value)
    }
}

#[derive(Debug, Clone)]
pub(super) enum RecoveryIconRestoreOutcome {
    NotIncluded,
    Restored(bento_nano_backend::icon_positions::RestoreResult),
    Failed(SmolStr),
}

impl RecoveryIconRestoreOutcome {
    pub(super) fn status_suffix(&self) -> SmolStr {
        match self {
            Self::NotIncluded => SmolStr::new_static(""),
            Self::Restored(result) => SmolStr::new(format!(
                " + icons live {}/{}/{}",
                result.restored, result.skipped, result.failed
            )),
            Self::Failed(error) => SmolStr::new(format!(" + icon restore failed: {error}")),
        }
    }

    pub(super) fn log_fields(&self) -> (bool, u32, u32, u32, bool, SmolStr) {
        match self {
            Self::NotIncluded => (false, 0, 0, 0, false, SmolStr::new_static("")),
            Self::Restored(result) => (
                true,
                result.restored,
                result.skipped,
                result.failed,
                result.auto_arrange_toggled,
                SmolStr::new_static(""),
            ),
            Self::Failed(error) => (true, 0, 0, 0, false, error.clone()),
        }
    }

    pub(super) fn localized_status_suffix(&self) -> SmolStr {
        match self {
            Self::NotIncluded => SmolStr::new_static(""),
            Self::Restored(result) => localized_current(
                format!(
                    " + 图标 {}/{}/{}",
                    result.restored, result.skipped, result.failed
                ),
                format!(
                    " + icons live {}/{}/{}",
                    result.restored, result.skipped, result.failed
                ),
            ),
            Self::Failed(error) => localized_current(
                format!(" + 图标恢复失败：{error}"),
                format!(" + icon restore failed: {error}"),
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct RecoveryBundleRestoreOutcome {
    pub(super) summary: bento_nano_backend::recovery_bundle::RecoveryBundleSummary,
    pub(super) icon_restore: RecoveryIconRestoreOutcome,
    pub(super) user_data_restore:
        bento_nano_backend::recovery_bundle::RecoveryUserDataRestoreReport,
}

pub(super) fn recovery_user_data_status_suffix(
    report: bento_nano_backend::recovery_bundle::RecoveryUserDataRestoreReport,
) -> SmolStr {
    if report.restored_files == 0 {
        SmolStr::new_static("")
    } else {
        SmolStr::new(format!(
            " + user data {}/{} B",
            report.restored_files, report.restored_bytes
        ))
    }
}

pub(super) fn localized_recovery_user_data_status_suffix(
    report: bento_nano_backend::recovery_bundle::RecoveryUserDataRestoreReport,
) -> SmolStr {
    if report.restored_files == 0 {
        SmolStr::new_static("")
    } else {
        localized_current(
            format!(
                " + 用户数据 {}/{} B",
                report.restored_files, report.restored_bytes
            ),
            format!(
                " + user data {}/{} B",
                report.restored_files, report.restored_bytes
            ),
        )
    }
}

pub(super) const SETTING_BACKUP_LAST_CREATED: &str = "backup.last_created";
pub(super) const SETTING_BACKUP_LAST_RESTORED: &str = "backup.last_restored";
pub(super) const DEFAULT_BACKUP_RETAINED: usize = 10;
pub(super) const MAX_BACKUP_RETAINED: usize = 100;
