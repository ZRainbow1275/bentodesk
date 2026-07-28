//! Native shell owner: `themes_plugins`.

use super::*;

pub(super) fn migrate_legacy_tauri_settings_to_vault(state_dir: &Path) {
    let accepted_accents = palette_picker::swatch_table()
        .iter()
        .map(|swatch| swatch.hex.as_str())
        .collect::<smallvec::SmallVec<[&str; 16]>>();
    let Some(mtx) = bento_nano_backend::config_vault::Vault::global() else {
        return;
    };
    let Ok(mut vault) = mtx.lock() else {
        tracing::warn!(
            target: "bentodesk::vault",
            "legacy Tauri settings import skipped: vault mutex poisoned"
        );
        return;
    };
    match bento_nano_backend::config_vault::tauri_settings::migrate_first_existing_tauri_settings(
        state_dir,
        &mut vault,
        &accepted_accents,
    ) {
        Ok(report) if report.vault_locked => {
            tracing::warn!(
                target: "bentodesk::vault",
                source = ?report.source_path,
                "legacy Tauri settings import skipped: passphrase vault locked"
            );
        }
        Ok(report) if report.imported_any() => {
            tracing::info!(
                target: "bentodesk::vault",
                source = ?report.source_path,
                imported = ?report.imported_keys,
                skipped_existing = ?report.skipped_existing_keys,
                skipped_invalid = ?report.skipped_invalid_fields,
                skipped_unsupported = ?report.skipped_unsupported_fields,
                "legacy Tauri settings imported into selected-stack vault"
            );
        }
        Ok(report) if report.only_skipped_existing() => {
            tracing::debug!(
                target: "bentodesk::vault",
                source = ?report.source_path,
                skipped_existing = ?report.skipped_existing_keys,
                "legacy Tauri settings import found no missing vault keys"
            );
        }
        Ok(report) => {
            if report.source_path.is_some()
                && (!report.skipped_invalid_fields.is_empty()
                    || !report.skipped_unsupported_fields.is_empty())
            {
                tracing::warn!(
                    target: "bentodesk::vault",
                    source = ?report.source_path,
                    skipped_invalid = ?report.skipped_invalid_fields,
                    skipped_unsupported = ?report.skipped_unsupported_fields,
                    "legacy Tauri settings import completed with skipped fields"
                );
            }
        }
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::vault",
                error = %error,
                "legacy Tauri settings import failed; selected-stack vault remains authoritative"
            );
        }
    }
}

pub(super) fn theme_options_from_themes(
    themes: &[bento_nano_backend::themes::Theme],
) -> Vec<ThemeOption> {
    themes
        .iter()
        .map(|theme| ThemeOption {
            id: theme.id.clone(),
            name: theme.name.clone(),
            is_builtin: theme.is_builtin,
        })
        .collect()
}

pub(super) fn load_themes_for_root(
    root: &AppRoot,
) -> Result<Vec<bento_nano_backend::themes::Theme>, ThemeError> {
    themes::load_all_themes(&themes_dir_for_root(root))
}

pub(super) fn import_theme_for_root(
    root: &AppRoot,
    source_path: &Path,
) -> Result<bento_nano_backend::themes::Theme, ThemeError> {
    themes::import_theme_file(source_path, &themes_dir_for_root(root))
}

pub(super) fn load_available_theme_options(root: &AppRoot) -> Result<Vec<ThemeOption>, ThemeError> {
    load_themes_for_root(root).map(|loaded| theme_options_from_themes(&loaded))
}

pub(super) fn apply_available_themes_to_app(root: &AppRoot) -> Result<bool, ThemeError> {
    let options = load_available_theme_options(root)?;
    let app = root.app.borrow();
    Ok(app.set_available_themes(options))
}

pub(super) fn plugin_type_label(plugin_type: &PluginType) -> SmolStr {
    use bento_nano_style::i18n_zh_cn::ids;

    match plugin_type {
        PluginType::Theme => SmolStr::new(bento_nano_style::t(ids::PLUGIN_TYPE_THEME)),
        PluginType::Widget => SmolStr::new(bento_nano_style::t(ids::PLUGIN_TYPE_WIDGET)),
        PluginType::Organizer => SmolStr::new(bento_nano_style::t(ids::PLUGIN_TYPE_ORGANIZER)),
    }
}

pub(super) fn plugin_entries_from_registry(registry: PluginRegistry) -> Vec<SettingsPluginEntry> {
    registry
        .plugins
        .into_iter()
        .map(|plugin| SettingsPluginEntry {
            id: SmolStr::new(plugin.id),
            name: SmolStr::new(plugin.name),
            version: SmolStr::new(plugin.version),
            plugin_type: plugin_type_label(&plugin.plugin_type),
            // M1h — thread the manifest author/description (already loaded by
            // the registry) through so the inline §11 plugin card shows the
            // author + description lines (Tauri `plugin-card__author/__desc`).
            author: SmolStr::new(plugin.author),
            description: SmolStr::new(plugin.description),
            enabled: plugin.enabled,
        })
        .collect()
}

pub(super) fn list_plugins_for_root(root: &AppRoot) -> Result<Vec<SettingsPluginEntry>, String> {
    PluginRegistry::load(&state_dir_for_root(root))
        .map(plugin_entries_from_registry)
        .map_err(|error| error.to_string())
}

pub(super) fn refresh_settings_plugins_for_root(root: &AppRoot) -> Result<bool, String> {
    let entries = list_plugins_for_root(root)?;
    let app = root.app.borrow();
    app.settings_plugin_uninstall_confirm.set(None);
    Ok(app.set_settings_plugins(entries))
}

pub(super) fn apply_theme_after_plugin_mutation(root: &AppRoot) -> Result<bool, String> {
    let current_id = {
        let app = root.app.borrow();
        app.active_theme_id.borrow().clone()
    };
    let loaded = load_themes_for_root(root).map_err(|error| error.to_string())?;
    let options = theme_options_from_themes(&loaded);
    if active_theme_id_is_builtin(current_id.as_str()) {
        let app = root.app.borrow();
        let mut changed = app.set_available_themes(options);
        drop(app);
        changed |=
            apply_active_theme_to_app(root, current_id).map_err(|error| error.to_string())?;
        return Ok(changed);
    }
    let theme = loaded
        .iter()
        .find(|theme| theme.id == current_id)
        .or_else(|| loaded.iter().find(|theme| theme.is_builtin))
        .cloned()
        .ok_or_else(|| no_themes_available_message().to_owned())?;
    if theme.id != current_id {
        if let Some(mtx) = bento_nano_backend::config_vault::Vault::global() {
            if let Ok(mut vault) = mtx.lock() {
                let _ = persist_active_theme_to_vault(&mut vault, &theme.id);
            }
        }
    }
    apply_active_theme_selection_to_app(root, options, theme).map_err(|error| error.to_string())
}

pub(super) fn refresh_plugin_dependent_state(root: &AppRoot) -> Result<bool, String> {
    let mut changed = refresh_settings_plugins_for_root(root)?;
    changed |= apply_theme_after_plugin_mutation(root)?;
    Ok(changed)
}

pub(super) fn set_plugin_setting_error(root: &AppRoot, message: SmolStr) {
    let app = root.app.borrow();
    app.settings_plugin_status
        .borrow_mut()
        .replace(SettingsBackupStatus::Error(message));
}

pub(super) fn set_plugin_setting_success(root: &AppRoot, message: SmolStr) {
    let app = root.app.borrow();
    app.settings_plugin_status
        .borrow_mut()
        .replace(SettingsBackupStatus::Success(message));
}

pub(super) fn localized_plugin_message(
    prefix_id: bento_nano_style::StringId,
    detail: impl core::fmt::Display,
) -> SmolStr {
    SmolStr::new(format!("{}{detail}", bento_nano_style::t(prefix_id)))
}

pub(super) fn load_theme_selection_for_root(
    root: &AppRoot,
    id: &str,
) -> Result<(Vec<ThemeOption>, bento_nano_backend::themes::Theme), ThemeError> {
    let loaded = load_themes_for_root(root)?;
    let options = theme_options_from_themes(&loaded);
    let Some(theme) = loaded.into_iter().find(|theme| theme.id.as_str() == id) else {
        return Err(ThemeError::NotFound { id: id.to_owned() });
    };
    Ok((options, theme))
}

pub(super) fn apply_active_theme_selection_to_app(
    root: &AppRoot,
    options: Vec<ThemeOption>,
    theme: bento_nano_backend::themes::Theme,
) -> Result<bool, ThemeError> {
    let tokens = themes::to_theme_tokens(&theme)?;
    let app = root.app.borrow();
    let mut changed = app.set_available_themes(options);
    changed |= app.apply_active_theme(theme.id.clone(), theme.name.clone(), tokens);
    app.settings_theme_status
        .borrow_mut()
        .replace(SettingsBackupStatus::Success(SmolStr::new(format!(
            "{} {}",
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_THEME_HEADING),
            theme.name
        ))));
    Ok(changed)
}

pub(super) fn active_theme_id_is_builtin(id: &str) -> bool {
    bento_nano_style::tokens::palette_tauri_for_theme(id).is_some()
}

pub(super) fn apply_builtin_active_theme_to_app(root: &AppRoot, id: &str) -> Option<bool> {
    let app = root.app.borrow();
    let changed = app.apply_active_theme_by_id(id)?;
    let name = app.active_theme_name.borrow().clone();
    app.settings_theme_status
        .borrow_mut()
        .replace(SettingsBackupStatus::Success(SmolStr::new(format!(
            "{} {name}",
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_THEME_HEADING)
        ))));
    Some(changed)
}

pub(super) fn apply_active_theme_to_app(root: &AppRoot, id: SmolStr) -> Result<bool, ThemeError> {
    if let Some(changed) = apply_builtin_active_theme_to_app(root, id.as_str()) {
        return Ok(changed);
    }
    let (options, theme) = load_theme_selection_for_root(root, id.as_str())?;
    apply_active_theme_selection_to_app(root, options, theme)
}

pub(super) fn persist_active_theme_to_vault(
    vault: &mut bento_nano_backend::config_vault::Vault,
    id: &SmolStr,
) -> Result<bool, bento_nano_backend::config_vault::VaultError> {
    if vault.is_locked_passphrase() {
        return Err(bento_nano_backend::config_vault::VaultError::NoPassphraseSet);
    }
    vault.set_setting(
        SETTING_ACTIVE_THEME,
        bento_nano_backend::config_vault::SettingValue::Str(id.clone()),
    );
    vault.flush()?;
    Ok(true)
}

pub(super) fn zone_display_mode_from_wire(value: &str) -> Option<ZoneDisplayMode> {
    ZoneDisplayMode::parse(value)
}

pub(super) fn persist_zone_display_mode_to_vault(
    vault: &mut bento_nano_backend::config_vault::Vault,
    mode: ZoneDisplayMode,
) -> Result<bool, bento_nano_backend::config_vault::VaultError> {
    if vault.is_locked_passphrase() {
        return Err(bento_nano_backend::config_vault::VaultError::NoPassphraseSet);
    }
    vault.set_setting(
        SETTING_ZONE_DISPLAY_MODE,
        bento_nano_backend::config_vault::SettingValue::Str(SmolStr::new_static(mode.as_wire())),
    );
    vault.flush()?;
    Ok(true)
}

pub(super) fn set_theme_setting_error(root: &AppRoot, message: SmolStr) {
    let app = root.app.borrow();
    app.settings_theme_status
        .borrow_mut()
        .replace(SettingsBackupStatus::Error(message));
}

pub(super) fn no_themes_available_message() -> &'static str {
    if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
        "没有可用主题"
    } else {
        "No themes available"
    }
}

pub(super) fn set_theme_setting_success(root: &AppRoot, message: SmolStr) {
    let app = root.app.borrow();
    app.settings_theme_status
        .borrow_mut()
        .replace(SettingsBackupStatus::Success(message));
}

pub(super) fn persist_skipped_update_to_vault(version: &SmolStr) {
    let Some(mtx) = bento_nano_backend::config_vault::Vault::global() else {
        tracing::warn!(
            target: "bentodesk::updater",
            version = %version,
            "SkipUpdateVersion: vault not initialised; skip remains in memory only"
        );
        return;
    };
    match mtx.lock() {
        Ok(mut vault) => {
            if vault.is_locked_passphrase() {
                tracing::warn!(
                    target: "bentodesk::updater",
                    version = %version,
                    "SkipUpdateVersion: vault locked; skip remains in memory only"
                );
                return;
            }
            vault.set_setting(
                SETTING_UPDATES_SKIPPED_VERSION,
                bento_nano_backend::config_vault::SettingValue::Str(version.clone()),
            );
            if let Err(error) = vault.flush() {
                tracing::warn!(
                    target: "bentodesk::updater",
                    version = %version,
                    error = %error,
                    "SkipUpdateVersion: flush failed; skip remains in memory only"
                );
            }
        }
        Err(_poisoned) => {
            tracing::warn!(
                target: "bentodesk::updater",
                version = %version,
                "SkipUpdateVersion: vault mutex poisoned; skip remains in memory only"
            );
        }
    }
}
