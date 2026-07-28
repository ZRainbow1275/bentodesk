//! Native shell owner: `hotkeys_updates`.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MinibarPinStateChange {
    Inserted,
    Refreshed,
}
pub(super) const KEYBINDING_PREFIX: &str = "keybinding.";
pub(super) const GLOBAL_HOTKEY_BASE_ID: i32 = 0x4240;

pub(super) fn default_hotkey_bindings() -> smallvec::SmallVec<[hotkey::HotkeyBinding; 20]> {
    smallvec::SmallVec::from_slice(hotkey::DEFAULT_BINDINGS)
}

pub(super) fn global_hotkey_id(command: hotkey::HotkeyCommand) -> Option<i32> {
    let offset = match command {
        hotkey::HotkeyCommand::Escape => return None,
        hotkey::HotkeyCommand::ToggleMain => 1,
        hotkey::HotkeyCommand::CreateZone => 2,
        hotkey::HotkeyCommand::DuplicateZone => 3,
        hotkey::HotkeyCommand::ToggleZoneLock => 4,
        hotkey::HotkeyCommand::ToggleAllZones => 5,
        hotkey::HotkeyCommand::AutoOrganize => 6,
        hotkey::HotkeyCommand::ReflowLayout => 7,
        hotkey::HotkeyCommand::OpenBulkManager => 8,
        hotkey::HotkeyCommand::FocusNextZone => 9,
        hotkey::HotkeyCommand::FocusPreviousZone => 10,
        hotkey::HotkeyCommand::ToggleSettings => 11,
        hotkey::HotkeyCommand::OpenSearch => 12,
        hotkey::HotkeyCommand::QuitApp => 13,
        hotkey::HotkeyCommand::OpenTimeline => 14,
        hotkey::HotkeyCommand::OpenSnapshotPicker => 15,
        hotkey::HotkeyCommand::UndoCheckpoint => 16,
        hotkey::HotkeyCommand::RedoCheckpoint => 17,
    };
    Some(GLOBAL_HOTKEY_BASE_ID + offset)
}

pub(super) fn global_hotkey_modifiers(mods: hotkey::ModFlags) -> u32 {
    let mut flags = MOD_NOREPEAT;
    if mods.ctrl {
        flags |= MOD_CONTROL;
    }
    if mods.shift {
        flags |= MOD_SHIFT;
    }
    if mods.alt {
        flags |= MOD_ALT;
    }
    flags
}

pub(super) fn global_hotkey_command(root: &AppRoot, id: i32) -> Option<hotkey::HotkeyCommand> {
    root.global_hotkeys
        .borrow()
        .iter()
        .find(|registration| registration.id == id)
        .map(|registration| registration.command)
}

pub(super) unsafe fn unregister_global_hotkeys(root: &AppRoot, hwnd: HWND) {
    let mut registrations = root.global_hotkeys.borrow_mut();
    for registration in registrations.drain(..) {
        // SAFETY: `registration.id` was previously registered against this
        // Main HWND by `register_global_hotkeys`.
        let ok = unsafe { UnregisterHotKey(hwnd, registration.id) };
        if ok == 0 {
            tracing::warn!(
                target: "bentodesk::hotkey",
                id = registration.id,
                command = ?registration.command,
                error = unsafe { GetLastError() },
                "global hotkey unregister failed"
            );
        }
    }
}

pub(super) unsafe fn register_global_hotkeys(root: &AppRoot, hwnd: HWND) -> usize {
    // SAFETY: Re-registration is Main-HWND scoped and starts by unregistering
    // every id known to have succeeded previously.
    unsafe { unregister_global_hotkeys(root, hwnd) };
    let bindings = root.hotkey_bindings.borrow();
    let mut registrations = root.global_hotkeys.borrow_mut();
    for binding in bindings.iter() {
        let Some(id) = global_hotkey_id(binding.command) else {
            continue;
        };
        let modifiers = global_hotkey_modifiers(binding.mods);
        // SAFETY: RegisterHotKey is called with the live Main HWND, an app
        // range id, Win32 modifier flags, and a virtual-key code from the
        // validated runtime table.
        let ok = unsafe { RegisterHotKey(hwnd, id, modifiers, binding.vk) };
        if ok == 0 {
            tracing::warn!(
                target: "bentodesk::hotkey",
                id,
                vk = binding.vk,
                modifiers,
                command = ?binding.command,
                error = unsafe { GetLastError() },
                "global hotkey registration failed; focused-HWND route remains available"
            );
            continue;
        }
        registrations.push(GlobalHotkeyRegistration {
            id,
            command: binding.command,
        });
    }
    let registered = registrations.len();
    log_static(format!("hotkey: registered_global count={registered}\n").as_str());
    registered
}

pub(super) fn refresh_global_hotkeys(root: &AppRoot) -> bool {
    let Some(hwnd) = find_main_hwnd(root) else {
        return false;
    };
    // SAFETY: `hwnd` is the registered Main HWND owned by this process.
    unsafe { register_global_hotkeys(root, hwnd) > 0 }
}

pub(super) fn apply_hotkey_binding(root: &AppRoot, action: &str, chord: &str) -> bool {
    let mut bindings = root.hotkey_bindings.borrow_mut();
    let binding = match hotkey::validate_binding(&bindings, action, chord) {
        Ok(binding) => binding,
        Err(hotkey::BindingValidationError::UnsupportedActionOrChord) => {
            tracing::warn!(
                target: "bentodesk::hotkey",
                %action,
                %chord,
                "keybinding rejected: unsupported action or chord"
            );
            return false;
        }
        Err(hotkey::BindingValidationError::ChordAlreadyAssigned) => {
            tracing::warn!(
                target: "bentodesk::hotkey",
                %action,
                %chord,
                "keybinding rejected: chord already assigned"
            );
            return false;
        }
    };
    if let Some(existing) = bindings
        .iter_mut()
        .find(|existing| existing.command == binding.command)
    {
        if existing.vk == binding.vk && existing.mods == binding.mods {
            return false;
        }
        *existing = binding;
        drop(bindings);
        let _ = refresh_global_hotkeys(root);
        return true;
    }
    bindings.push(binding);
    drop(bindings);
    let _ = refresh_global_hotkeys(root);
    true
}

pub(super) fn apply_hotkey_setting_to_runtime(
    root: &AppRoot,
    key: &str,
    value: &bento_nano_app::SettingValue,
) -> bool {
    let Some(action) = key.strip_prefix(KEYBINDING_PREFIX) else {
        return false;
    };
    match value {
        bento_nano_app::SettingValue::Str(chord) => apply_hotkey_binding(root, action, chord),
        _ => {
            tracing::warn!(
                target: "bentodesk::hotkey",
                %key,
                "keybinding rejected: value must be a string chord"
            );
            false
        }
    }
}

pub(super) fn keybinding_action_at(row_index: usize) -> Option<&'static str> {
    keybindings_section::keybinding_rows()
        .get(row_index)
        .map(|row| row.action)
}

pub(super) fn keybinding_setting_key(action: &str) -> Option<SmolStr> {
    hotkey::command_for_action(action)?;
    Some(SmolStr::new(format!("{KEYBINDING_PREFIX}{action}")))
}

pub(super) fn set_keybinding_feedback(
    app: &AppState,
    action: &str,
    message: SmolStr,
    is_error: bool,
) {
    let action = SmolStr::new(action);
    let feedback = if is_error {
        SettingsKeybindingFeedback::Error { action, message }
    } else {
        SettingsKeybindingFeedback::Success { action, message }
    };
    app.settings_keybinding_feedback
        .borrow_mut()
        .replace(feedback);
}

pub(super) fn validate_keybinding_candidate(
    root: &AppRoot,
    action: &str,
    chord: &str,
) -> Result<(), hotkey::BindingValidationError> {
    let bindings = root.hotkey_bindings.borrow();
    hotkey::validate_binding(&bindings, action, chord).map(|_| ())
}

pub(super) fn persist_keybinding_reset_to_vault(
    vault: &mut bento_nano_backend::config_vault::Vault,
    action: &str,
) -> Result<bool, bento_nano_backend::config_vault::VaultError> {
    if vault.is_locked_passphrase() {
        return Err(bento_nano_backend::config_vault::VaultError::NoPassphraseSet);
    }
    let Some(key) = keybinding_setting_key(action) else {
        return Ok(false);
    };
    let removed = vault.remove_setting(key.as_str());
    if removed {
        vault.flush()?;
    }
    Ok(true)
}

pub(super) fn queue_locale_setting_toggle(root: &AppRoot) {
    let next_locale = if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
        "en-US"
    } else {
        "zh-CN"
    };
    root.dispatcher
        .push(locale_setting_command_for(next_locale));
    root.dispatcher.push(Command::ToggleLocale);
}

pub(super) fn locale_setting_command_for(locale: &'static str) -> Command {
    Command::SetSetting {
        key: SmolStr::new_static(SETTING_DISPLAY_LOCALE),
        value: bento_nano_app::SettingValue::Str(SmolStr::new_static(locale)),
    }
}

pub(super) fn queue_update_frequency_cycle(root: &AppRoot) {
    let next_frequency = {
        let app = root.app.borrow();
        next_update_frequency(app.update_check_frequency.get())
    };
    root.dispatcher
        .push(update_frequency_setting_command_for(next_frequency));
}

pub(super) fn queue_update_auto_download_toggle(root: &AppRoot) {
    let next_value = {
        let app = root.app.borrow();
        !app.update_auto_download.get()
    };
    root.dispatcher.push(bool_setting_command_for(
        SETTING_UPDATES_AUTO_DOWNLOAD,
        next_value,
    ));
}

pub(super) fn queue_update_action(root: &AppRoot) {
    let command = {
        let app = root.app.borrow();
        match &*app.settings_updater_status.borrow() {
            SettingsUpdaterStatus::Available { .. } => Some(Command::DownloadUpdate),
            SettingsUpdaterStatus::Ready { .. } => Some(Command::InstallUpdateAndRestart),
            _ => None,
        }
    };
    if let Some(command) = command {
        root.dispatcher.push(command);
    } else {
        let app = root.app.borrow();
        *app.settings_updater_status.borrow_mut() = SettingsUpdaterStatus::Error(
            localized_current("当前没有可执行的更新操作", "No update action is available"),
        );
    }
}

pub(super) fn queue_update_skip(root: &AppRoot, hwnd: HWND) {
    let version = {
        let app = root.app.borrow();
        app.settings_updater_status.borrow().version_for_skip()
    };
    match version {
        Some(version) => {
            root.dispatcher.push(Command::SkipUpdateVersion(version));
        }
        None => {
            let app = root.app.borrow();
            *app.settings_updater_status.borrow_mut() =
                SettingsUpdaterStatus::Error(localized_current(
                    "当前没有可跳过的更新版本",
                    "No update version is available to skip",
                ));
            request_redraw(hwnd);
        }
    }
}

pub(super) fn queue_stealth_enabled_toggle(root: &AppRoot) {
    let next_value = {
        let app = root.app.borrow();
        !app.stealth_enabled.get()
    };
    root.dispatcher.push(bool_setting_command_for(
        SETTING_STEALTH_ENABLED,
        next_value,
    ));
}

// M7 (2026-06-01) — `queue_encryption_mode_cycle` removed. The orphan
// `CycleEncryptionMode` 2-cycle (None↔Dpapi, Passphrase→None) was replaced
// wholesale by the §10 3-button mode grid; its passphrase-capture-activation
// snippet moved into the `SelectEncryptionModePassphrase` / `FocusPassphraseField`
// dispatch arms, and the direct None/Dpapi sets now route through the existing
// `encryption_mode_setting_command_for` helper from the mode-button arms.

// M6-UI (2026-05-29) — the Wave J1b popup picker→backend mapping table
// (`PICKER_TO_BACKEND`) and its three lookup helpers
// (`backend_theme_id_for_picker_index` / `picker_index_for_backend_id` /
// `picker_preset_display_name`) were removed: §3 Appearance is now the inline
// grid. Each ThemeCard carries its own stable `theme_id`
// (`theme_picker::BUILTIN_THEMES[i].theme_id`) which routes straight through
// M6a's `apply_active_theme_by_id` (all 17 builtins, no partial-preview
// fallback), so no index→id translation table is needed.

pub(super) fn queue_zone_display_mode_cycle(root: &AppRoot) {
    let app = root.app.borrow();
    let next_mode = app.zone_display_mode.get().next();
    app.set_zone_display_mode(next_mode);
    app.settings_dirty.set(true);
    app.settings_save_error.borrow_mut().take();
}

/// α4 (Wave I-α, 2026-05-25) — dispatch `Command::SetSetting` with an
/// explicit zone-display mode chosen from the 3-radio picker (instead of
/// cycling). Mirrors `queue_zone_display_mode_cycle` byte-for-byte except
/// for the source of `next_mode`.
pub(super) fn queue_zone_display_mode_set(root: &AppRoot, mode: bento_nano_app::ZoneDisplayMode) {
    let app = root.app.borrow();
    app.set_zone_display_mode(mode);
    app.settings_dirty.set(true);
    app.settings_save_error.borrow_mut().take();
}

pub(super) fn update_frequency_setting_command_for(frequency: UpdateCheckFrequency) -> Command {
    Command::SetSetting {
        key: SmolStr::new_static(SETTING_UPDATES_CHECK_FREQUENCY),
        value: bento_nano_app::SettingValue::Str(SmolStr::new_static(update_frequency_wire(
            frequency,
        ))),
    }
}

pub(super) fn encryption_mode_setting_command_for(mode: SettingsEncryptionMode) -> Command {
    Command::SetSetting {
        key: SmolStr::new_static(SETTING_ENCRYPTION_MODE),
        value: bento_nano_app::SettingValue::Str(SmolStr::new_static(mode.as_wire())),
    }
}

/// P2 (#7 fix wave 2026-06-01) — the user-visible mode label that MATCHES the
/// mode-button TITLES (Tauri uses one `modeLabel()` for the current-mode value,
/// the buttons, AND the applied banner). Passphrase maps to the FULL token
/// (`ENCRYPTION_MODE_PASSPHRASE_FULL`, id 236 = 自定义口令, NOT the short
/// `ENCRYPTION_MODE_PASSPHRASE` (id 86 = 瀵嗙爜) the renderer's
/// `localized_encryption_mode` used. None/DPAPI reuse the shared button ids.
pub(super) fn localized_encryption_mode_button_label(mode: SettingsEncryptionMode) -> &'static str {
    use bento_nano_style::i18n_zh_cn::ids;
    match mode {
        SettingsEncryptionMode::None => bento_nano_style::t(ids::ENCRYPTION_MODE_NONE),
        SettingsEncryptionMode::Dpapi => bento_nano_style::t(ids::ENCRYPTION_MODE_DPAPI),
        SettingsEncryptionMode::Passphrase => {
            bento_nano_style::t(ids::ENCRYPTION_MODE_PASSPHRASE_FULL)
        }
    }
}

/// P15 (#7 fix wave 2026-06-01) — PURE focus seam for clicking the passphrase
/// INPUT (`FocusPassphraseField`). Sets the focused field + the char-capture
/// flag so typing is captured, but DOES NOT switch the encryption mode/purpose
/// to an apply and DOES NOT clear the draft (so a click-to-refocus mid-edit
/// keeps what was typed). The purpose Cell tracks Set vs Unlock so the
/// subsequent BUTTON apply routes correctly, but selecting the input alone
/// never applies. Matches Tauri, where the input's focus is inert and only the
/// Passphrase button calls `applyMode`.
pub(super) fn focus_passphrase_field(app: &AppState) {
    let purpose = if app.passphrase_unlock_required.get() {
        PassphraseEntryPurpose::Unlock
    } else {
        PassphraseEntryPurpose::Set
    };
    app.passphrase_entry_purpose.set(purpose);
    app.passphrase_entry_active.set(true);
    app.settings_focused_field
        .set(bento_nano_app::SettingsTextField::Passphrase);
}

/// P15 (#7 fix wave 2026-06-01) — PURE apply seam for the Passphrase BUTTON
/// (`SelectEncryptionModePassphrase`), mirroring Tauri `applyMode("Passphrase")`.
/// Reads the already-typed draft:
///   — empty → sets the localized `ENCRYPTION_REQUIRED` error banner + returns
///     `None` (no command — the apply is refused, exactly like Tauri's early
///     `setError(encryptionPassphraseRequired)` return);
///   — non-empty → clears the in-flight capture (the button commits the typed
///     draft directly) + returns the verify-probe→apply `Command`
///     (`SetEncryptionPassphrase` on Set, `UnlockEncryptionPassphrase` on
///     Unlock). The command's vault reopen IS the probe — a bad passphrase
///     fails the reopen and the command handler surfaces the error banner.
pub(super) fn passphrase_button_command(app: &AppState) -> Option<Command> {
    let draft = app.passphrase_draft.borrow().trim().to_owned();
    if draft.is_empty() {
        app.settings_encryption_status
            .borrow_mut()
            .replace(SettingsBackupStatus::Error(SmolStr::new(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_REQUIRED),
            )));
        return None;
    }
    let purpose = if app.passphrase_unlock_required.get() {
        PassphraseEntryPurpose::Unlock
    } else {
        PassphraseEntryPurpose::Set
    };
    // The button commits the typed draft directly — the in-flight capture is no
    // longer needed, so deactivate + clear it (and blur the field).
    app.passphrase_entry_active.set(false);
    app.passphrase_draft.borrow_mut().clear();
    app.settings_focused_field
        .set(bento_nano_app::SettingsTextField::None);
    Some(match purpose {
        PassphraseEntryPurpose::Set => Command::SetEncryptionPassphrase(SmolStr::new(draft)),
        PassphraseEntryPurpose::Unlock => Command::UnlockEncryptionPassphrase(SmolStr::new(draft)),
    })
}

/// P9 (#7 fix wave 2026-06-01) — set the §10 Encryption success banner to
/// `"{ENCRYPTION_MODE_APPLIED} {mode label}"` (green) after a None/DPAPI mode
/// change, mirroring Tauri `applyMode`'s `setInfo`. The label uses the
/// button-title source so the banner, the current-mode value, and the active
/// button title all read identically.
pub(super) fn set_encryption_mode_applied_banner(app: &AppState) {
    let mode = app.encryption_mode.get();
    let prefix = bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_APPLIED);
    let label = localized_encryption_mode_button_label(mode);
    app.settings_encryption_status
        .borrow_mut()
        .replace(SettingsBackupStatus::Success(SmolStr::new(format!(
            "{prefix} {label}"
        ))));
}

pub(super) fn bool_setting_command_for(key: &'static str, value: bool) -> Command {
    Command::SetSetting {
        key: SmolStr::new_static(key),
        value: bento_nano_app::SettingValue::Bool(value),
    }
}

// M7 (2026-06-01) — `next_encryption_mode` removed. It existed only to drive
// the orphan `CycleEncryptionMode` 2-cycle (now replaced by the §10 3-button
// mode grid, which sets each mode explicitly rather than cycling). The
// `encryption_mode_setting_command_for` helper below stays — it's now called
// from the `SelectEncryptionModeNone` / `SelectEncryptionModeDpapi` arms.

pub(super) fn next_update_frequency(current: UpdateCheckFrequency) -> UpdateCheckFrequency {
    match current {
        UpdateCheckFrequency::Daily => UpdateCheckFrequency::Weekly,
        UpdateCheckFrequency::Weekly => UpdateCheckFrequency::Manual,
        UpdateCheckFrequency::Manual => UpdateCheckFrequency::Daily,
    }
}

pub(super) fn should_start_background_update_check(frequency: UpdateCheckFrequency) -> bool {
    update_check_interval(frequency).is_some()
}

pub(super) fn update_check_interval(
    frequency: UpdateCheckFrequency,
) -> Option<std::time::Duration> {
    bento_nano_backend::updater::check_interval_hours(frequency)
        .map(|hours| std::time::Duration::from_secs(hours.saturating_mul(60 * 60)))
}

pub(super) fn maybe_start_background_update_check(root: &AppRoot) {
    let frequency = root.app.borrow().update_check_frequency.get();
    if !should_start_background_update_check(frequency) {
        return;
    }
    if let Some(interval) = update_check_interval(frequency) {
        root.updater.spawn_recurring_background_check(interval);
    }
}

pub(super) fn encryption_mode_from_wire(value: &str) -> Option<SettingsEncryptionMode> {
    match value {
        "None" => Some(SettingsEncryptionMode::None),
        "Dpapi" => Some(SettingsEncryptionMode::Dpapi),
        "Passphrase" => Some(SettingsEncryptionMode::Passphrase),
        _ => None,
    }
}

pub(super) fn backend_encryption_mode_from_app(
    mode: SettingsEncryptionMode,
) -> bento_nano_backend::config_vault::EncryptionMode {
    match mode {
        SettingsEncryptionMode::None => bento_nano_backend::config_vault::EncryptionMode::None,
        SettingsEncryptionMode::Dpapi => bento_nano_backend::config_vault::EncryptionMode::Dpapi,
        SettingsEncryptionMode::Passphrase => {
            bento_nano_backend::config_vault::EncryptionMode::None
        }
    }
}

pub(super) fn update_frequency_wire(frequency: UpdateCheckFrequency) -> &'static str {
    match frequency {
        UpdateCheckFrequency::Daily => "Daily",
        UpdateCheckFrequency::Weekly => "Weekly",
        UpdateCheckFrequency::Manual => "Manual",
    }
}

pub(super) fn update_frequency_from_wire(value: &str) -> Option<UpdateCheckFrequency> {
    match value {
        "Daily" => Some(UpdateCheckFrequency::Daily),
        "Weekly" => Some(UpdateCheckFrequency::Weekly),
        "Manual" => Some(UpdateCheckFrequency::Manual),
        _ => None,
    }
}

pub(super) fn theme_base_accent_from_wire(value: &str) -> Option<SmolStr> {
    palette_picker::swatch_table()
        .iter()
        .find(|swatch| swatch.hex.as_str() == value)
        .map(|swatch| swatch.hex.clone())
}

pub(super) fn apply_theme_base_accent_to_app(app: &AppState, accent: Option<SmolStr>) -> bool {
    let mut current = app.theme_base_accent.borrow_mut();
    if *current == accent {
        return false;
    }
    *current = accent;
    true
}

pub(super) fn persist_theme_base_accent_to_vault(
    vault: &mut bento_nano_backend::config_vault::Vault,
    accent: Option<&SmolStr>,
) -> Result<bool, bento_nano_backend::config_vault::VaultError> {
    if vault.is_locked_passphrase() {
        return Err(bento_nano_backend::config_vault::VaultError::NoPassphraseSet);
    }
    match accent {
        Some(hex) => {
            if theme_base_accent_from_wire(hex.as_str()).is_none() {
                tracing::warn!(
                    target: "bentodesk::vault",
                    %hex,
                    "theme.base_accent rejected: unsupported palette swatch"
                );
                return Ok(false);
            }
            vault.set_setting(
                SETTING_THEME_BASE_ACCENT,
                bento_nano_backend::config_vault::SettingValue::Str(hex.clone()),
            );
        }
        None => {
            vault.remove_setting(SETTING_THEME_BASE_ACCENT);
        }
    }
    vault.flush()?;
    Ok(true)
}

pub(super) fn themes_dir_for_root(root: &AppRoot) -> PathBuf {
    themes::themes_dir(&state_dir_for_root(root))
}

pub(super) fn state_dir_for_root(root: &AppRoot) -> PathBuf {
    let zones_path = root.app.borrow().zones_path.clone();
    if let Some(parent) = zones_path
        .parent()
        .filter(|_| !zones_path.as_os_str().is_empty())
    {
        return parent.to_path_buf();
    }
    match storage::appdata_path() {
        Ok(path) => path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        Err(_error) => PathBuf::from("."),
    }
}
