//! Native shell owner: `settings_keys_restore`.

use super::*;

pub(super) const SETTING_DISPLAY_LOCALE: &str = "display.locale";
pub(super) const SETTING_UPDATES_CHECK_FREQUENCY: &str = "updates.check_frequency";
pub(super) const SETTING_UPDATES_AUTO_DOWNLOAD: &str = "updates.auto_download";
pub(super) const SETTING_UPDATES_SKIPPED_VERSION: &str = "updates.skipped_version";
pub(super) const SETTING_STEALTH_ENABLED: &str = "stealth.enabled";
pub(super) const SETTING_ENCRYPTION_MODE: &str = "encryption.mode";
pub(super) const SETTING_APPEARANCE_ACCENT_COLOR: &str = "accent_color";
pub(super) const SETTING_THEME_BASE_ACCENT: &str = "theme.base_accent";
pub(super) const SETTING_ACTIVE_THEME: &str = "active_theme";
pub(super) const SETTING_ZONE_DISPLAY_MODE: &str = "zone_display_mode";
pub(super) const SETTING_DEBUG_OVERLAY: &str = "debug_overlay";
pub(super) const SETTING_MINIBAR_PINNED_ZONES: &str = "minibar.pinned_zones";
// M1a 2026-05-29 — General-section persistence keys. Names mirror Tauri's
// `AppSettings` field names in dotted form (see `bentodesk/src/types/settings.ts`
// and `updateSettingsStore` at `SettingsPanel.tsx:216-227`) so a future
// vault file ported between the two builds reads back identically.
pub(super) const SETTING_GENERAL_GHOST_LAYER_ENABLED: &str = "general.ghost_layer_enabled";
pub(super) const SETTING_GENERAL_LAUNCH_AT_STARTUP: &str = "general.launch_at_startup";
pub(super) const SETTING_GENERAL_SHOW_IN_TASKBAR: &str = "general.show_in_taskbar";
pub(super) const SETTING_GENERAL_AUTO_GROUP_ENABLED: &str = "general.auto_group_enabled";
pub(super) const SETTING_GENERAL_PORTABLE_MODE: &str = "general.portable_mode";
// M1d 2026-05-29 — Performance §5 + Startup management §6 persistence keys.
// Dotted names mirror Tauri's `AppSettings` (`performance.*` / `startup.*`)
// so a vault file stays portable between the two builds.
pub(super) const SETTING_PERF_EXPAND_DELAY_MS: &str = "performance.expand_delay_ms";
pub(super) const SETTING_PERF_COLLAPSE_DELAY_MS: &str = "performance.collapse_delay_ms";
pub(super) const SETTING_PERF_ICON_CACHE_SIZE: &str = "performance.icon_cache_size";
pub(super) const SETTING_STARTUP_HIGH_PRIORITY: &str = "startup.high_priority";
pub(super) const SETTING_STARTUP_CRASH_RESTART_ENABLED: &str = "startup.crash_restart_enabled";
pub(super) const SETTING_STARTUP_CRASH_MAX_RETRIES: &str = "startup.crash_max_retries";
pub(super) const SETTING_STARTUP_CRASH_WINDOW_SECS: &str = "startup.crash_window_secs";
pub(super) const SETTING_STARTUP_SAFE_AFTER_HIBERNATION: &str =
    "startup.safe_start_after_hibernation";
pub(super) const SETTING_STARTUP_HIBERNATE_RESUME_DELAY_MS: &str =
    "startup.hibernate_resume_delay_ms";
// W1 (#7 fix wave 2026-06-01) — §2 Paths persistence keys. Tauri keys these as
// `desktop_path` (string) + `watch_paths` (string[]). The nano vault stores
// scalars, so the watch list persists as a single newline-joined `Str` (one
// path per line) — exactly the in-memory `watch_paths_draft` shape. The dotted
// `paths.*` names follow the existing `general.*` / `performance.*` convention.
pub(super) const SETTING_PATHS_DESKTOP_PATH: &str = "paths.desktop_path";
pub(super) const SETTING_PATHS_WATCH_PATHS: &str = "paths.watch_paths";

/// M1a 2026-05-29 — restore each General-section AppState Cell from the
/// persisted vault values written by `save_settings_general`. Absent keys
/// keep the AppState defaults so a fresh installation reads as the
/// designed-default toggle layout (matches Tauri's `defaultAppSettings`
/// fall-through in `stores/settings.ts`).
///
/// Called once from `apply_persisted_settings_from_vault` after the locale /
/// updater frequency restores so the General section is in its persisted
/// state before the panel can render. Returns silently when the vault
/// global is not yet installed (early startup before `init_global` runs);
/// callers must tolerate that branch without retrying.
pub(super) fn apply_general_settings_from_vault(app: &AppState) {
    let Some(mtx) = bento_nano_backend::config_vault::Vault::global() else {
        return;
    };
    let Ok(vault) = mtx.lock() else {
        tracing::warn!(
            target: "bentodesk::vault",
            "general settings restore skipped: vault mutex poisoned"
        );
        return;
    };
    let ghost = vault.get_setting(SETTING_GENERAL_GHOST_LAYER_ENABLED);
    let startup = vault.get_setting(SETTING_GENERAL_LAUNCH_AT_STARTUP);
    let taskbar = vault.get_setting(SETTING_GENERAL_SHOW_IN_TASKBAR);
    let auto_group = vault.get_setting(SETTING_GENERAL_AUTO_GROUP_ENABLED);
    let portable = vault.get_setting(SETTING_GENERAL_PORTABLE_MODE);
    // M1d — Performance §5 + Startup management §6 reads.
    let expand_delay = vault.get_setting(SETTING_PERF_EXPAND_DELAY_MS);
    let collapse_delay = vault.get_setting(SETTING_PERF_COLLAPSE_DELAY_MS);
    let icon_cache = vault.get_setting(SETTING_PERF_ICON_CACHE_SIZE);
    let high_priority = vault.get_setting(SETTING_STARTUP_HIGH_PRIORITY);
    let crash_restart = vault.get_setting(SETTING_STARTUP_CRASH_RESTART_ENABLED);
    let crash_retries = vault.get_setting(SETTING_STARTUP_CRASH_MAX_RETRIES);
    let crash_window = vault.get_setting(SETTING_STARTUP_CRASH_WINDOW_SECS);
    let safe_start = vault.get_setting(SETTING_STARTUP_SAFE_AFTER_HIBERNATION);
    let hibernate_delay = vault.get_setting(SETTING_STARTUP_HIBERNATE_RESUME_DELAY_MS);
    // W1 (#7 fix wave) — rehydrate the two §2 Paths drafts so path/watch edits
    // survive a relaunch (they were dropped on flush before this fix).
    let desktop_path = vault.get_setting(SETTING_PATHS_DESKTOP_PATH);
    let watch_paths = vault.get_setting(SETTING_PATHS_WATCH_PATHS);
    drop(vault);

    restore_general_bool_cell(&app.setting_desktop_embed, ghost, "ghost_layer_enabled");
    restore_general_bool_cell(&app.setting_autostart, startup, "launch_at_startup");
    // Mc-3 #12 — HKCU\Run is the real source of truth; the toggle previously
    // never wrote it. Override the persisted mirror with the actual registry
    // state so the UI is honest. Display-only (no boot re-assert → respects
    // Task Manager disable).
    app.setting_autostart
        .set(bento_nano_backend::autostart::is_enabled());
    restore_general_bool_cell(&app.setting_show_in_taskbar, taskbar, "show_in_taskbar");
    restore_general_bool_cell(&app.setting_smart_layout, auto_group, "auto_group_enabled");
    restore_general_bool_cell(&app.setting_portable_mode, portable, "portable_mode");
    // M1d — clamp persisted ints back to their Tauri bounds on restore so a
    // hand-edited / ported vault can never push a slider out of range.
    use bento_nano_app::state::{
        COLLAPSE_DELAY_MAX_MS, COLLAPSE_DELAY_MIN_MS, CRASH_MAX_RETRIES_MAX, CRASH_MAX_RETRIES_MIN,
        CRASH_WINDOW_SECS_MAX, CRASH_WINDOW_SECS_MIN, EXPAND_DELAY_MAX_MS, EXPAND_DELAY_MIN_MS,
        HIBERNATE_DELAY_MAX_MS, HIBERNATE_DELAY_MIN_MS, ICON_CACHE_MAX, ICON_CACHE_MIN,
    };
    restore_general_int_cell(
        &app.expand_delay_ms,
        expand_delay,
        EXPAND_DELAY_MIN_MS,
        EXPAND_DELAY_MAX_MS,
        "expand_delay_ms",
    );
    restore_general_int_cell(
        &app.collapse_delay_ms,
        collapse_delay,
        COLLAPSE_DELAY_MIN_MS,
        COLLAPSE_DELAY_MAX_MS,
        "collapse_delay_ms",
    );
    restore_general_int_cell(
        &app.icon_cache_size,
        icon_cache,
        ICON_CACHE_MIN,
        ICON_CACHE_MAX,
        "icon_cache_size",
    );
    restore_general_bool_cell(
        &app.startup_high_priority,
        high_priority,
        "startup_high_priority",
    );
    restore_general_bool_cell(
        &app.crash_restart_enabled,
        crash_restart,
        "crash_restart_enabled",
    );
    restore_general_int_cell(
        &app.crash_max_retries,
        crash_retries,
        CRASH_MAX_RETRIES_MIN,
        CRASH_MAX_RETRIES_MAX,
        "crash_max_retries",
    );
    restore_general_int_cell(
        &app.crash_window_secs,
        crash_window,
        CRASH_WINDOW_SECS_MIN,
        CRASH_WINDOW_SECS_MAX,
        "crash_window_secs",
    );
    restore_general_bool_cell(
        &app.safe_start_after_hibernation,
        safe_start,
        "safe_start_after_hibernation",
    );
    restore_general_int_cell(
        &app.hibernate_resume_delay_ms,
        hibernate_delay,
        HIBERNATE_DELAY_MIN_MS,
        HIBERNATE_DELAY_MAX_MS,
        "hibernate_resume_delay_ms",
    );
    // W1 (#7 fix wave) — apply the two §2 Paths drafts. Absent keys keep the
    // AppState defaults (`D:\\Desktop` / empty) so a fresh install reads as
    // designed; a persisted (possibly empty) string overrides them.
    restore_general_str_cell(&app.desktop_path_draft, desktop_path, "desktop_path");
    restore_general_str_cell(&app.watch_paths_draft, watch_paths, "watch_paths");
}

/// W1 2026-06-01 — apply a `SettingValue::Str` to one §2 Paths draft
/// `RefCell<SmolStr>`, logging if the persisted value has the wrong tag. `None`
/// keeps the AppState default (silent — first-launch path).
pub(super) fn restore_general_str_cell(
    cell: &std::cell::RefCell<SmolStr>,
    value: Option<bento_nano_backend::config_vault::SettingValue>,
    label: &'static str,
) {
    match value {
        Some(bento_nano_backend::config_vault::SettingValue::Str(s)) => {
            *cell.borrow_mut() = s;
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                key = %label,
                "general settings restore skipped: non-str value"
            );
        }
        None => {}
    }
}

/// M1d 2026-05-29 — apply a `SettingValue::Int` to one `Cell<i32>`, clamped to
/// `[min, max]`, logging if the persisted value has the wrong tag. `None`
/// keeps the AppState default (silent — first-launch path). The clamp guards
/// a hand-edited / ported vault from pushing a slider out of its Tauri range.
pub(super) fn restore_general_int_cell(
    cell: &std::cell::Cell<i32>,
    value: Option<bento_nano_backend::config_vault::SettingValue>,
    min: i32,
    max: i32,
    label: &'static str,
) {
    match value {
        Some(bento_nano_backend::config_vault::SettingValue::Int(i)) => {
            let clamped = (i as i32).clamp(min, max);
            cell.set(clamped);
        }
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                key = %label,
                "general settings restore skipped: non-int value"
            );
        }
        None => {}
    }
}

/// M1a 2026-05-29 — apply a `SettingValue::Bool` to one General-section
/// `Cell<bool>`, logging if the persisted value has the wrong tag. `None`
/// keeps the AppState default (silent — first-launch path).
pub(super) fn restore_general_bool_cell(
    cell: &std::cell::Cell<bool>,
    value: Option<bento_nano_backend::config_vault::SettingValue>,
    label: &'static str,
) {
    match value {
        Some(bento_nano_backend::config_vault::SettingValue::Bool(b)) => cell.set(b),
        Some(_) => {
            tracing::warn!(
                target: "bentodesk::vault",
                key = %label,
                "general settings restore skipped: non-bool value"
            );
        }
        None => {}
    }
}

/// M1a 2026-05-29 — persist the 5 General toggle Cells to the config vault
/// in one batched write + flush. Wired to the footer Save click (and only
/// runs when `settings_dirty` is true; clean Save short-circuits). After
/// the write completes the dirty flag clears, leaving the panel in a
/// "just-saved" state until the next toggle.
pub(super) fn persist_settings_accent_to_vault(
    vault: &mut bento_nano_backend::config_vault::Vault,
    accent_draft: Option<&SmolStr>,
    clear_requested: bool,
) {
    if clear_requested {
        vault.remove_setting(SETTING_APPEARANCE_ACCENT_COLOR);
        vault.remove_setting(SETTING_THEME_BASE_ACCENT);
        return;
    }
    let Some(accent) = accent_draft else {
        return;
    };
    vault.set_setting(
        SETTING_APPEARANCE_ACCENT_COLOR,
        bento_nano_backend::config_vault::SettingValue::Str(accent.clone()),
    );
    vault.set_setting(
        SETTING_THEME_BASE_ACCENT,
        bento_nano_backend::config_vault::SettingValue::Str(accent.clone()),
    );
}

pub(super) const SETTINGS_PROTECTED_PREFIXES: &[&str] = &[
    r"c:\windows",
    r"c:\program files",
    r"c:\program files (x86)",
    r"c:\programdata",
    r"c:\$recycle.bin",
    r"c:\system volume information",
];

pub(super) const SETTINGS_TRANSACTION_KEYS: &[&str] = &[
    SETTING_GENERAL_GHOST_LAYER_ENABLED,
    SETTING_GENERAL_LAUNCH_AT_STARTUP,
    SETTING_GENERAL_SHOW_IN_TASKBAR,
    SETTING_GENERAL_AUTO_GROUP_ENABLED,
    SETTING_GENERAL_PORTABLE_MODE,
    SETTING_PERF_EXPAND_DELAY_MS,
    SETTING_PERF_COLLAPSE_DELAY_MS,
    SETTING_PERF_ICON_CACHE_SIZE,
    SETTING_STARTUP_HIGH_PRIORITY,
    SETTING_STARTUP_CRASH_RESTART_ENABLED,
    SETTING_STARTUP_CRASH_MAX_RETRIES,
    SETTING_STARTUP_CRASH_WINDOW_SECS,
    SETTING_STARTUP_SAFE_AFTER_HIBERNATION,
    SETTING_STARTUP_HIBERNATE_RESUME_DELAY_MS,
    SETTING_PATHS_DESKTOP_PATH,
    SETTING_PATHS_WATCH_PATHS,
    SETTING_APPEARANCE_ACCENT_COLOR,
    SETTING_THEME_BASE_ACCENT,
    SETTING_ACTIVE_THEME,
    SETTING_ZONE_DISPLAY_MODE,
];
