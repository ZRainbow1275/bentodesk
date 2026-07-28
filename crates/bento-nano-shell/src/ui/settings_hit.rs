use super::*;

// -----------------------------------------------------------------------------
// Phase 2.1 Ruling C — settings panel hit-tester.
//
// Geometry constants live in `bento_nano_app::settings_panel` so the renderer
// and this hit-tester share a single source of truth. We re-export the rect
// helpers here for callers that already pull `ui::*`.
// -----------------------------------------------------------------------------

// Round-2 M1 — only the modal helpers + the J1b active-theme chip rect stay
// in the active import set. The Wave K1 row helpers are orphan-alive in
// `bento-nano-app::settings_panel` but no longer referenced from this hit
// tester; their `pub use` re-exports were dropped here to keep the symbol
// surface trim.
// M1h (2026-05-29) — the plugins modal-rect helpers (`settings_plugin_row_rect`
// / `_toggle_rect` / `_uninstall_rect`, `settings_plugins_close_rect` /
// `_install_rect` / `_modal_rect` / `_refresh_rect`) were dropped from this
// re-export when the Plugins surface moved inline; the inline §11 hit-test uses
// the fully-qualified `bento_nano_app::settings_panel::settings_plugin_*` paths
// (same convention as the Backup §9 hits).
pub use bento_nano_app::settings_panel::{
    SETTINGS_BACKUP_ENTRY_VISIBLE_MAX, SETTINGS_CLOSE_BTN_H, SETTINGS_CLOSE_BTN_W,
    SETTINGS_PANEL_HEIGHT, SETTINGS_PANEL_PADDING, SETTINGS_PANEL_WIDTH, SETTINGS_SWITCH_BTN_H,
    SETTINGS_SWITCH_BTN_W, settings_keybinding_record_rect, settings_keybinding_reset_rect,
    settings_keybindings_close_rect, settings_keybindings_modal_rect,
};

/// What part of the settings overlay was clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHit {
    /// Locale-switch button row.
    SwitchLocale,
    /// Open the keybindings recorder/reset modal.
    OpenKeybindings,
    /// Close the keybindings recorder/reset modal.
    CloseKeybindings,
    // M1h (2026-05-29) — `OpenPlugins` / `ClosePlugins` / `RefreshPlugins` were
    // removed: the Plugins surface is inline (no modal to open/close) and Tauri
    // has no Refresh affordance in the §11 section (the list refreshes on
    // Settings open). Install / Toggle / Uninstall below stay.
    /// Select a `.bdplugin`/zip archive for install through the selected-stack
    /// safe archive extraction and plugin registry path.
    InstallPlugin,
    /// Toggle the visible plugin row at this row index.
    TogglePlugin(usize),
    /// Uninstall the visible plugin row at this row index.
    UninstallPlugin(usize),
    /// Confirm the destructive uninstall action for the visible plugin row.
    ConfirmUninstallPlugin(usize),
    /// Cancel the currently pending plugin uninstall confirmation.
    CancelUninstallPlugin,
    /// Start recording the visible keybinding row at this row index.
    RecordKeybinding(usize),
    /// Reset the visible keybinding row at this row index.
    ResetKeybinding(usize),
    /// Updater cadence cycle button row.
    CycleUpdateFrequency,
    /// Updater lifecycle check-now button.
    CheckForUpdates,
    /// Updater auto-download toggle row.
    ToggleUpdateAutoDownload,
    /// Updater download/install stateful action button.
    RunUpdateAction,
    /// Updater skip-current-version button.
    SkipCurrentUpdate,
    /// Stealth storage master switch row.
    ToggleStealthEnabled,
    /// Theme base accent picker row.
    OpenThemeBasePalette,
    /// Native JSON theme import row.
    ImportTheme,
    // M6-UI (2026-05-29) — `CycleActiveTheme` (the Row-5 chip that toggled the
    // now-deleted swatch popup) was removed. Theme selection is the inline §3
    // Appearance grid: `SelectTheme(id)` below re-skins live.
    /// M6-UI — §3 Appearance grid: a ThemeCard click. `id` is the preset id
    /// (`0..PRESET_COUNT`, == `theme_picker::BUILTIN_THEMES` index). The
    /// dispatch arm resolves `preset.theme_id` → `apply_active_theme_by_id`
    /// (live re-skin) + sets `settings_dirty`.
    SelectTheme(u8),
    /// M6-UI — §3 Appearance accent row (Control B): an accent swatch click.
    /// `index` is the VIBRANT strip index (`0..ACCENT_SWATCH_COUNT`). The
    /// dispatch arm writes `settings_draft_accent_color` + sets `settings_dirty`.
    SelectAccent(u8),
    /// V21-N15 — §3 Appearance inline hex accent editor (`#rrggbb`).
    EditAccentColor,
    /// V21-N16 — §3 Appearance native Windows colour picker launcher.
    OpenAccentColorPicker,
    /// V21-N16 - §3 Appearance inline accent reset button.
    ClearAccentColor,
    /// Process default zone display-mode cycle button.
    CycleZoneDisplayMode,
    /// α4 (Wave I-α, 2026-05-25) — pick a specific zone-display mode
    /// (Hover / Always / Click) from the 3-radio picker that replaces the
    /// orphan cycle button. Dispatches `Command::SetSetting` like the
    /// cycle button used to, but with the explicit chosen mode wire string
    /// instead of `zone_display_mode.next()`.
    SetZoneDisplayMode(bento_nano_app::ZoneDisplayMode),
    /// Create a config-vault backup now.
    CreateSettingsBackup,
    /// List real config-vault backup files.
    ListSettingsBackups,
    /// Restore the newest real config-vault backup.
    RestoreLatestSettingsBackup,
    /// Restore the visible backup entry at this newest-first list index.
    RestoreSettingsBackup(usize),
    /// Capture a synchronized recovery bundle for the current layout.
    CreateRecoveryBundle,
    /// Export a validated recovery diagnostics report.
    ExportRecoveryDiagnostics,
    /// Restore the current layout from the latest recovery bundle.
    RestoreRecoveryBundle,
    /// Bottom close button.
    Close,
    /// Anywhere inside the panel chrome but not on a button — eat the click.
    Body,
    /// Outside the panel rect — dismiss the panel (Ruling C: click-outside).
    Outside,
    // M6-UI (2026-05-29) — the Wave J1b swatch-popup hits
    // (`PickerThumbnail` / `PickerAccent` / `PickerReset` / `PickerSave` /
    // `ClosePicker`) AND the orphan `CycleActiveTheme` chip (whose sole purpose
    // was toggling that popup) were removed: §3 Appearance is now an inline
    // grid. Card / accent-swatch clicks land on `SelectTheme` / `SelectAccent`
    // below.
    // ------------------------------------------------------------------
    // Round-2 M1 — dark Settings shell hits. New variants only; the K1
    // variants above stay orphan-alive (Ruling B) so the existing dispatch
    // arms in `bento-nano-shell::main` continue to link.
    // ------------------------------------------------------------------
    /// Top-section row 0: 桌面嵌入设 (desktop embed).
    ToggleDesktopEmbed,
    /// Top-section row 1: 开机启动 (run at startup).
    ToggleAutostart,
    /// Top-section row 2: 显示在任务栏 (show in taskbar).
    ToggleShowInTaskbar,
    /// Top-section row 3: 智能自动布局 (smart auto-layout).
    ToggleSmartLayout,
    /// Top-section row 4: 便携模式 (portable mode — restart required).
    /// M1a 2026-05-29: renamed from `ToggleSpeedMode` to reach Tauri 1:1
    /// parity with `SettingsPanel.tsx:294` (bound field `portable_mode`).
    TogglePortableMode,
    /// Open the locale chooser (currently flips locale; M5 promotes this to
    /// a popup once the dropdown menu lands). Distinct from `SwitchLocale`
    /// only by intent — they dispatch identically in M1.
    OpenLocaleMenu,
    /// Sticky-footer Cancel button — discards in-memory changes and closes.
    CancelSettings,
    /// Sticky-footer Save button — closes (real persistence wires in a
    /// later wave).
    SaveSettings,
    /// Wheel/keyboard scroll delta against the body (positive = scroll down
    /// in DIPs). The wheel handler dispatches this so the wheel-routing path
    /// stays single-purpose; mouse hit-test never emits it directly.
    ScrollBodyDelta(i32),
    /// M1i 2026-05-29 — 桌面源 §2 refresh (`↻`) button: re-run
    /// `desktop_sources::all_desktop_dirs` and repopulate the cached
    /// `AppState::desktop_sources` read-only list, then redraw. Replaces the
    /// two per-card cosmetic enable toggles (`ToggleSourcePrimary` /
    /// `ToggleSourcePublic`), which were removed as a deliberate Tauri-parity
    /// change — the Tauri `desktop-source-card` has no toggle, only a 已监视
    /// badge.
    RefreshDesktopSources,
    /// Round-2 M2 / M7 — 桌面路径 input click → focus it for live keyboard
    /// editing (sets `settings_focused_field = DesktopPath`).
    EditDesktopPath,
    /// Round-2 M2 / M7 — 监控值 textarea click → focus it for live keyboard
    /// editing (sets `settings_focused_field = WatchValues`).
    EditWatchValues,

    // ------------------------------------------------------------------
    // M7 2026-06-01 — Encryption §10 card (`EncryptionCard.tsx`). The
    // 3-button mode grid + passphrase-field focus replace the orphan
    // `CycleEncryptionMode` 2-cycle (removed with its dispatch arm +
    // `queue_encryption_mode_cycle` / `next_encryption_mode`).
    // ------------------------------------------------------------------
    /// M7 — §10 mode grid: select the "None" mode button. Dispatches
    /// `SetSetting{ "encryption.mode", "None" }` direct.
    SelectEncryptionModeNone,
    /// M7 — §10 mode grid: select the "DPAPI" mode button. Dispatches
    /// `SetSetting{ "encryption.mode", "Dpapi" }` direct.
    SelectEncryptionModeDpapi,
    /// M7 — §10 mode grid: select the "Passphrase" mode button. Activates
    /// passphrase capture (sets `passphrase_entry_active` + purpose = Set +
    /// `settings_focused_field = Passphrase`); the actual
    /// `SetEncryptionPassphrase` fires on Enter after the user types.
    SelectEncryptionModePassphrase,
    /// M7 — §10 passphrase input click → focus it for live keyboard editing
    /// (activate passphrase capture, purpose = Set, same as the Passphrase
    /// mode button).
    FocusPassphraseField,

    // ------------------------------------------------------------------
    // M1d 2026-05-29 — Performance §5 + Startup management §6. These
    // replace the deleted bespoke 高级 / 未来集成验证 hits. Every variant
    // mutates real AppState, is Save-gated, and is reverted by Cancel.
    // ------------------------------------------------------------------
    /// M1d — Performance slider drag. `index` selects the slider
    /// (0=expand_delay, 1=collapse_delay, 2=icon_cache) and the quantized
    /// client `x` lets the dispatcher map track-x→stepped value. Quantizing
    /// to i32 keeps `SettingsHit` `Eq` derivable.
    DragPerformanceSlider { index: u8, x_q: i32 },
    /// M1d — Startup §6 toggle: 高优先级启动 (`startup_high_priority`).
    ToggleStartupHighPriority,
    /// M1d — Startup §6 toggle: 崩溃自动重启 (`crash_restart_enabled`).
    /// Gates the two crash steppers below.
    ToggleCrashRestart,
    /// M1d — Startup §6 stepper `+`: 最大重试次数 (`crash_max_retries`).
    IncCrashMaxRetries,
    /// M1d — Startup §6 stepper `−`: 最大重试次数 (`crash_max_retries`).
    DecCrashMaxRetries,
    /// M1d — Startup §6 stepper `+`: 崩溃窗口（秒）(`crash_window_secs`).
    IncCrashWindowSecs,
    /// M1d — Startup §6 stepper `−`: 崩溃窗口（秒）(`crash_window_secs`).
    DecCrashWindowSecs,
    /// M1d — Startup §6 toggle: 休眠安全恢复
    /// (`safe_start_after_hibernation`). Gates the hibernate slider below.
    ToggleSafeStartHibernation,
    /// M1d — Startup §6 hibernate slider drag (恢复延迟 ms). Carries the
    /// quantized client `x` for the dispatcher's track-x→value map.
    DragHibernateDelay(i32),

    // ------------------------------------------------------------------
    // M1e 2026-05-29 — Stealth §7 card (`StealthModeCard.tsx`). Both
    // variants dispatch to a REAL `bento_nano_backend::stealth` call (no
    // no-op arms): Refresh re-reads `stealth::status()`; Reapply builds a
    // `StealthConfig` + calls `reapply_hidden_on_startup`.
    // ------------------------------------------------------------------
    /// M1e — Stealth §7 Refresh button: re-read `stealth::status()` into the
    /// cached `app.stealth_status` snapshot and redraw.
    RefreshStealth,
    /// M1e — Stealth §7 Reapply button (重新应用): build the live
    /// `StealthConfig` and call `stealth::reapply_hidden_on_startup`, then
    /// refresh the cached status.
    ReapplyStealth,
}

/// Resolve a click point against the settings overlay layout.
///
/// Round-2 M1 — the panel is now the dark shell; only the top 5 toggle rows
/// + language row + sticky footer + close × hit-test. Wave K1 row helpers
///   stay live as orphans so the existing match arms in `main.rs` still link
///   (Ruling B), but no rect emits them in M1.
pub fn settings_hit(app: &AppState, x: f32, y: f32) -> SettingsHit {
    let vp = app.viewport;
    if app.settings_keybindings_open.get() {
        let modal = settings_keybindings_modal_rect(vp);
        if x >= modal.x && x < modal.right() && y >= modal.y && y < modal.bottom() {
            let close = settings_keybindings_close_rect(vp);
            if x >= close.x && x < close.right() && y >= close.y && y < close.bottom() {
                return SettingsHit::CloseKeybindings;
            }
            for row_index in
                0..bento_nano_app::business::settings::keybindings_section::keybinding_rows().len()
            {
                let record = settings_keybinding_record_rect(vp, row_index);
                if x >= record.x && x < record.right() && y >= record.y && y < record.bottom() {
                    return SettingsHit::RecordKeybinding(row_index);
                }
                let reset = settings_keybinding_reset_rect(vp, row_index);
                if x >= reset.x && x < reset.right() && y >= reset.y && y < reset.bottom() {
                    return SettingsHit::ResetKeybinding(row_index);
                }
            }
        }
        return SettingsHit::Body;
    }
    // M1h (2026-05-29) — the plugins MODAL hit block was removed: the Plugins
    // surface is now an always-inline §11 section hit-tested at the body level
    // (Install / per-card Toggle / per-card Uninstall) near the end of this
    // function, after the Backup §9 hits. `settings_plugins_open` +
    // `OpenPlugins` / `ClosePlugins` / `RefreshPlugins` were deleted.
    // M6-UI (2026-05-29) — the Wave J1b swatch-popup hit block was removed:
    // §3 Appearance is now an always-inline grid hit-tested at the body level
    // (cards → `SelectTheme`, accent swatches → `SelectAccent`) near the end of
    // this function, after the Plugins §11 hits. `theme_picker_open` +
    // `theme_picker_popup_origin` / `theme_picker_layout` / `hit_test` were
    // deleted.
    // Round-2 M1 — dark shell hit routing.
    let panel = bento_nano_app::settings_panel::settings_panel_rect_m1(vp);
    if x < panel.x || x >= panel.right() || y < panel.y || y >= panel.bottom() {
        return SettingsHit::Outside;
    }
    // Header close × — sticky, hit-tested before any scrolled content.
    let close_m1 = bento_nano_app::settings_panel::settings_close_button_rect_m1(vp);
    if x >= close_m1.x && x < close_m1.right() && y >= close_m1.y && y < close_m1.bottom() {
        return SettingsHit::Close;
    }
    // Footer Cancel + Save — sticky.
    let cancel_btn = bento_nano_app::settings_panel::settings_cancel_button_rect(vp);
    if x >= cancel_btn.x && x < cancel_btn.right() && y >= cancel_btn.y && y < cancel_btn.bottom() {
        return SettingsHit::CancelSettings;
    }
    let save_btn = bento_nano_app::settings_panel::settings_save_button_rect(vp);
    if x >= save_btn.x && x < save_btn.right() && y >= save_btn.y && y < save_btn.bottom() {
        return SettingsHit::SaveSettings;
    }
    // Body scroll area — anything outside the body rect (i.e. inside footer
    // padding) eats as `Body` so phantom drags do not leak to underlying
    // surfaces.
    let body = bento_nano_app::settings_panel::settings_body_rect(vp);
    if x < body.x || x >= body.right() || y < body.y || y >= body.bottom() {
        return SettingsHit::Body;
    }
    let scroll_y = app.scroll_offset_y.get();
    // Top 5 toggles — index 0..=4.
    for index in 0..bento_nano_app::settings_panel::SETTINGS_TOP_TOGGLE_COUNT {
        let hit = bento_nano_app::settings_panel::settings_top_toggle_hit_rect(vp, scroll_y, index);
        if x >= hit.x && x < hit.right() && y >= hit.y && y < hit.bottom() {
            return match index {
                0 => SettingsHit::ToggleDesktopEmbed,
                1 => SettingsHit::ToggleAutostart,
                2 => SettingsHit::ToggleShowInTaskbar,
                3 => SettingsHit::ToggleSmartLayout,
                4 => SettingsHit::TogglePortableMode,
                _ => SettingsHit::Body,
            };
        }
    }
    // Language dropdown chip.
    let lang_chip = bento_nano_app::settings_panel::settings_language_chip_rect(vp, scroll_y);
    if x >= lang_chip.x && x < lang_chip.right() && y >= lang_chip.y && y < lang_chip.bottom() {
        return SettingsHit::OpenLocaleMenu;
    }
    // G3 parity (2026-06-01) — the zone-display-mode picker radios moved OUT of
    // the General band into the §4 DisplayMode group (between §3 Appearance and
    // §5 Performance). Their hit-test therefore now lives below the reserve-delta
    // fold, alongside the perf-and-below sections (see the §4 block after the
    // fold). The radios no longer sit directly under the Language chip.
    // M1i fidelity — the §2 source list reflows to the LIVE source count;
    // the hit geometry must read the same count the renderer paints.
    let source_count = app.desktop_sources.borrow().len();
    // M1i fidelity — 桌面源 §2 refresh (`↻`) button. Now the LAST child of the
    // list, right-anchored BELOW the live card stack (not on the heading row).
    // Click re-resolves the desktop sources and repopulates the read-only list.
    // The source cards themselves are display-only (no per-card hit-box).
    let refresh = bento_nano_app::settings_panel::settings_sources_refresh_button_rect(
        vp,
        scroll_y,
        source_count,
    );
    if x >= refresh.x && x < refresh.right() && y >= refresh.y && y < refresh.bottom() {
        return SettingsHit::RefreshDesktopSources;
    }
    // Round-2 M2 — 桌面路径 input box (reflows below the live source stack).
    let path_box = bento_nano_app::settings_panel::settings_desktop_path_input_rect(
        vp,
        scroll_y,
        source_count,
    );
    if x >= path_box.x && x < path_box.right() && y >= path_box.y && y < path_box.bottom() {
        return SettingsHit::EditDesktopPath;
    }
    // Round-2 M2 — 监控值 textarea (reflows below the live source stack).
    let watch_box =
        bento_nano_app::settings_panel::settings_watch_textarea_rect(vp, scroll_y, source_count);
    if x >= watch_box.x && x < watch_box.right() && y >= watch_box.y && y < watch_box.bottom() {
        return SettingsHit::EditWatchValues;
    }
    // M1i fidelity — single-base-offset reflow (mirrors the renderer's `scroll`
    // shadow in `render.rs`). Everything from Performance §5 downward roots at
    // the fixed 4-card source reserve; fold the live reserve delta into
    // `scroll_y` so the hit geometry shifts UP by the height of the missing
    // source cards in lockstep with what is painted.
    let scroll_y =
        scroll_y + bento_nano_app::settings_panel::settings_sources_reserve_delta(source_count);
    // §4 DisplayMode group (G3 parity 2026-06-01) — zone-display-mode picker
    // radios. Promoted out of the General band into a standalone §4 group
    // between §3 Appearance and §5 Performance, so the hit-test now uses the
    // reserve-FOLDED `scroll_y` (the radios root at the same fixed source-reserve
    // baseline as Performance §5). Three right-anchored radio hit-boxes; each
    // dispatches `SetZoneDisplayMode(mode)`. Index → mode mirrors the renderer.
    for index in 0..bento_nano_app::settings_panel::SETTINGS_ZONE_DISPLAY_MODE_COUNT {
        let hit = bento_nano_app::settings_panel::settings_zone_display_mode_radio_rect(
            vp, scroll_y, index,
        );
        if x >= hit.x && x < hit.right() && y >= hit.y && y < hit.bottom() {
            let mode = match index {
                0 => bento_nano_app::ZoneDisplayMode::Hover,
                1 => bento_nano_app::ZoneDisplayMode::Always,
                2 => bento_nano_app::ZoneDisplayMode::Click,
                _ => return SettingsHit::Body,
            };
            return SettingsHit::SetZoneDisplayMode(mode);
        }
    }
    // M1d — Performance §5: 3 SliderRows (no conditionals). The slider track
    // band sits on the lower line of each row; a click anywhere on it starts
    // a drag carrying the quantized client x for the dispatcher's
    // track-x→value map.
    for index in 0..bento_nano_app::settings_panel::SETTINGS_PERF_ROW_COUNT {
        let track =
            bento_nano_app::settings_panel::settings_performance_slider_rect(vp, scroll_y, index);
        if x >= track.x && x < track.right() && y >= track.y && y < track.bottom() {
            return SettingsHit::DragPerformanceSlider {
                index,
                x_q: x.round() as i32,
            };
        }
    }
    // M1d — Startup §6. Two always-on toggles, two conditional steppers
    // (crash_restart), one always-on toggle, one conditional slider
    // (hibernation). The two gating bools are read from AppState so the
    // hit-test geometry matches whatever rows are currently painted.
    let crash_restart_on = app.crash_restart_enabled.get();
    let safe_start_on = app.safe_start_after_hibernation.get();
    // 高优先级启动 toggle (row 0).
    let high_row =
        bento_nano_app::settings_panel::settings_startup_high_priority_row_rect(vp, scroll_y);
    let high_hit = bento_nano_app::settings_panel::settings_startup_toggle_hit_rect(high_row);
    if x >= high_hit.x && x < high_hit.right() && y >= high_hit.y && y < high_hit.bottom() {
        return SettingsHit::ToggleStartupHighPriority;
    }
    // 崩溃自动重启 toggle (row 1).
    let crash_row = bento_nano_app::settings_panel::settings_crash_restart_row_rect(vp, scroll_y);
    let crash_hit = bento_nano_app::settings_panel::settings_startup_toggle_hit_rect(crash_row);
    if x >= crash_hit.x && x < crash_hit.right() && y >= crash_hit.y && y < crash_hit.bottom() {
        return SettingsHit::ToggleCrashRestart;
    }
    // Crash steppers (rows 2/3) — only when crash_restart_on.
    if crash_restart_on {
        let retries_row =
            bento_nano_app::settings_panel::settings_crash_max_retries_row_rect(vp, scroll_y);
        let r_plus = bento_nano_app::settings_panel::settings_stepper_plus_rect(retries_row);
        if x >= r_plus.x && x < r_plus.right() && y >= r_plus.y && y < r_plus.bottom() {
            return SettingsHit::IncCrashMaxRetries;
        }
        let r_minus = bento_nano_app::settings_panel::settings_stepper_minus_rect(retries_row);
        if x >= r_minus.x && x < r_minus.right() && y >= r_minus.y && y < r_minus.bottom() {
            return SettingsHit::DecCrashMaxRetries;
        }
        let window_row =
            bento_nano_app::settings_panel::settings_crash_window_row_rect(vp, scroll_y);
        let w_plus = bento_nano_app::settings_panel::settings_stepper_plus_rect(window_row);
        if x >= w_plus.x && x < w_plus.right() && y >= w_plus.y && y < w_plus.bottom() {
            return SettingsHit::IncCrashWindowSecs;
        }
        let w_minus = bento_nano_app::settings_panel::settings_stepper_minus_rect(window_row);
        if x >= w_minus.x && x < w_minus.right() && y >= w_minus.y && y < w_minus.bottom() {
            return SettingsHit::DecCrashWindowSecs;
        }
    }
    // 休眠安全恢复 toggle (row 4) — Y depends on crash_restart_on.
    let safe_row = bento_nano_app::settings_panel::settings_safe_start_row_rect(
        vp,
        scroll_y,
        crash_restart_on,
    );
    let safe_hit = bento_nano_app::settings_panel::settings_startup_toggle_hit_rect(safe_row);
    if x >= safe_hit.x && x < safe_hit.right() && y >= safe_hit.y && y < safe_hit.bottom() {
        return SettingsHit::ToggleSafeStartHibernation;
    }
    // 恢复延迟 hibernate slider (row 5) — only when safe_start_on.
    if safe_start_on {
        let track = bento_nano_app::settings_panel::settings_hibernate_slider_rect(
            vp,
            scroll_y,
            crash_restart_on,
        );
        if x >= track.x && x < track.right() && y >= track.y && y < track.bottom() {
            return SettingsHit::DragHibernateDelay(x.round() as i32);
        }
    }

    // M1e — Stealth §7 buttons ([Refresh][Reapply]). The buttons-row Y depends
    // on the conditional retry/error rows above it, so read the same cached
    // `stealth_status` snapshot the renderer paints from (so paint geometry
    // and hit geometry agree). Only the two buttons are interactive — the
    // status/value rows and the OneDrive text block are non-interactive.
    let (stealth_has_retry, stealth_has_error) = match &*app.stealth_status.borrow() {
        Some(s) => (s.retry_count > 0, s.last_error.is_some()),
        None => (false, false),
    };
    let stealth_btn_row = bento_nano_app::settings_panel::settings_stealth_buttons_row_rect(
        vp,
        scroll_y,
        crash_restart_on,
        safe_start_on,
        stealth_has_retry,
        stealth_has_error,
    );
    let refresh_btn =
        bento_nano_app::settings_panel::settings_stealth_refresh_button_rect(stealth_btn_row);
    if x >= refresh_btn.x
        && x < refresh_btn.right()
        && y >= refresh_btn.y
        && y < refresh_btn.bottom()
    {
        return SettingsHit::RefreshStealth;
    }
    let reapply_btn =
        bento_nano_app::settings_panel::settings_stealth_reapply_button_rect(stealth_btn_row);
    if x >= reapply_btn.x
        && x < reapply_btn.right()
        && y >= reapply_btn.y
        && y < reapply_btn.bottom()
    {
        return SettingsHit::ReapplyStealth;
    }

    // M1f — Updater §8 actions/prefs. The card's row Ys depend on the
    // Startup+Stealth gating flags AND the live updater status family (which
    // drives the version/progress/error middle-block height), so build the
    // same `SettingsBodyFlags` the renderer paints from (so paint geometry and
    // hit geometry agree). Interactive: 3 action buttons + frequency chip +
    // auto-download toggle. The status/version/progress/error blocks are
    // non-interactive. Action-button column indices match the renderer: col 0
    // = 检查更新 (always), col 1 = 下载/安装并重启 (gated), col 2 = 跳过此版本 (gated).
    let updater_status = app.settings_updater_status.borrow();
    let updater_kind =
        bento_nano_app::business::settings::updater_card::updater_height_kind(&updater_status);
    let updater_flags = bento_nano_app::settings_panel::SettingsBodyFlags::new(
        crash_restart_on,
        safe_start_on,
        stealth_has_retry,
        stealth_has_error,
        updater_kind,
    );
    let upd_btn_row = bento_nano_app::settings_panel::settings_updater_buttons_row_rect(
        vp,
        scroll_y,
        &updater_flags,
    );
    // Col 0 — 检查更新 (always).
    let check_btn = bento_nano_app::settings_panel::settings_updater_button_rect(upd_btn_row, 0);
    if x >= check_btn.x && x < check_btn.right() && y >= check_btn.y && y < check_btn.bottom() {
        return SettingsHit::CheckForUpdates;
    }
    // Col 1 — 下载 (Available) or 安装并重启 (Ready) → RunUpdateAction.
    if bento_nano_app::business::settings::updater_card::updater_show_download(&updater_status)
        || bento_nano_app::business::settings::updater_card::updater_show_install(&updater_status)
    {
        let action_btn =
            bento_nano_app::settings_panel::settings_updater_button_rect(upd_btn_row, 1);
        if x >= action_btn.x
            && x < action_btn.right()
            && y >= action_btn.y
            && y < action_btn.bottom()
        {
            return SettingsHit::RunUpdateAction;
        }
    }
    // Col 2 — 跳过此版本 (Available/Ready) → SkipCurrentUpdate.
    if bento_nano_app::business::settings::updater_card::updater_show_skip(&updater_status) {
        let skip_btn = bento_nano_app::settings_panel::settings_updater_button_rect(upd_btn_row, 2);
        if x >= skip_btn.x && x < skip_btn.right() && y >= skip_btn.y && y < skip_btn.bottom() {
            return SettingsHit::SkipCurrentUpdate;
        }
    }
    // 检查频率 cycling chip → CycleUpdateFrequency.
    let upd_freq_row = bento_nano_app::settings_panel::settings_updater_frequency_row_rect(
        vp,
        scroll_y,
        &updater_flags,
    );
    let freq_chip =
        bento_nano_app::settings_panel::settings_updater_frequency_chip_rect(upd_freq_row);
    if x >= freq_chip.x && x < freq_chip.right() && y >= freq_chip.y && y < freq_chip.bottom() {
        return SettingsHit::CycleUpdateFrequency;
    }
    // 后台静默下载 toggle → ToggleUpdateAutoDownload.
    let upd_auto_row = bento_nano_app::settings_panel::settings_updater_auto_download_row_rect(
        vp,
        scroll_y,
        &updater_flags,
    );
    let auto_hit =
        bento_nano_app::settings_panel::settings_updater_auto_download_hit_rect(upd_auto_row);
    if x >= auto_hit.x && x < auto_hit.right() && y >= auto_hit.y && y < auto_hit.bottom() {
        return SettingsHit::ToggleUpdateAutoDownload;
    }

    // M1g — Backup §9 buttons. The card's row Ys depend on the same
    // Startup+Stealth+Updater flags as the renderer PLUS the variable backup
    // row count (capped), so build the same `SettingsBodyFlags` the renderer
    // paints from via `with_backup_rows` (so paint geometry and hit geometry
    // agree). Interactive: 立即备份 (always) + per-row 恢复
    // (one per visible entry). The title/description/status/empty rows are
    // non-interactive. The per-row 恢复 carries the newest-first list index;
    // the dispatch arm maps index → entry → backup_id.
    let backup_entries = app.settings_backup_entries.borrow();
    let backup_visible =
        bento_nano_app::business::settings::backup_card::backup_visible_row_count(&backup_entries);
    let backup_flags = updater_flags
        .with_backup_rows(backup_visible)
        .with_backup_status(app.settings_backup_status.borrow().is_some())
        .with_encryption_status(app.settings_encryption_status.borrow().is_some());
    let backup_actions = bento_nano_app::settings_panel::settings_backup_actions_row_rect(
        vp,
        scroll_y,
        &backup_flags,
    );
    let create_btn =
        bento_nano_app::settings_panel::settings_backup_create_button_rect(backup_actions);
    if x >= create_btn.x && x < create_btn.right() && y >= create_btn.y && y < create_btn.bottom() {
        return SettingsHit::CreateSettingsBackup;
    }
    // Per-row 恢复 buttons — only the visible (non-empty, capped) entries.
    if !bento_nano_app::business::settings::backup_card::backup_list_is_empty(&backup_entries) {
        for entry_index in 0..backup_visible {
            let entry_row = bento_nano_app::settings_panel::settings_backup_entry_row_rect(
                vp,
                scroll_y,
                &backup_flags,
                entry_index,
            );
            let restore_btn =
                bento_nano_app::settings_panel::settings_backup_restore_button_rect(entry_row);
            if x >= restore_btn.x
                && x < restore_btn.right()
                && y >= restore_btn.y
                && y < restore_btn.bottom()
            {
                return SettingsHit::RestoreSettingsBackup(entry_index);
            }
        }
    }

    // M7 — Encryption §10 inline card. Slots between Backup §9 and Plugins §11
    // (Tauri `<BackupCard/><EncryptionCard/>` adjacency). Fixed-height, so it
    // reuses the `backup_flags` (no variable rows of its own). Interactive: the
    // 3 mode buttons (None / DPAPI / Passphrase) + the masked passphrase input
    // box. The label/desc/current-mode/hint/status rows are non-interactive.
    // Uses the identical `settings_encryption_*_rect` helpers the renderer
    // paints from (paint geometry == hit geometry).
    for index in 0..bento_nano_app::settings_panel::SETTINGS_ENCRYPTION_MODE_COUNT {
        let btn = bento_nano_app::settings_panel::settings_encryption_mode_button_rect(
            vp,
            scroll_y,
            &backup_flags,
            index,
        );
        if x >= btn.x && x < btn.right() && y >= btn.y && y < btn.bottom() {
            return match index {
                0 => SettingsHit::SelectEncryptionModeNone,
                1 => SettingsHit::SelectEncryptionModeDpapi,
                _ => SettingsHit::SelectEncryptionModePassphrase,
            };
        }
    }
    let enc_input = bento_nano_app::settings_panel::settings_encryption_passphrase_input_rect(
        vp,
        scroll_y,
        &backup_flags,
    );
    if x >= enc_input.x && x < enc_input.right() && y >= enc_input.y && y < enc_input.bottom() {
        return SettingsHit::FocusPassphraseField;
    }

    // M1h — Plugins §11 inline section. The card Ys depend on the same
    // Startup+Stealth+Updater+Backup+Encryption flags as the renderer PLUS the
    // variable plugin row count (capped), so build the same `SettingsBodyFlags`
    // the renderer paints from via `with_plugin_rows` (paint geometry == hit
    // geometry). Interactive: 安装插件... (always) + per-card enable toggle +
    // per-card 卸载. The title/author/desc/empty rows are non-interactive. The
    // per-card toggle/uninstall carry the list index; the dispatch arms map
    // index → entry → plugin id (and toggle flips the current enabled state).
    let plugin_entries = app.settings_plugin_entries.borrow();
    let plugin_visible =
        bento_nano_app::business::settings::plugins_section::plugin_visible_row_count(
            &plugin_entries,
        );
    let plugin_flags = backup_flags
        .with_plugin_rows(plugin_visible)
        .with_plugin_status(app.settings_plugin_status.borrow().is_some());
    let plugin_install = bento_nano_app::settings_panel::settings_plugins_install_button_rect(
        vp,
        scroll_y,
        &plugin_flags,
    );
    if x >= plugin_install.x
        && x < plugin_install.right()
        && y >= plugin_install.y
        && y < plugin_install.bottom()
    {
        return SettingsHit::InstallPlugin;
    }
    // Per-card enable toggle + 卸载 — only the visible (non-empty, capped) cards.
    if !bento_nano_app::business::settings::plugins_section::plugin_list_is_empty(&plugin_entries) {
        for card_index in 0..plugin_visible {
            let card = bento_nano_app::settings_panel::settings_plugin_card_rect(
                vp,
                scroll_y,
                &plugin_flags,
                card_index,
            );
            let toggle = bento_nano_app::settings_panel::settings_plugin_toggle_hit_rect(card);
            if x >= toggle.x && x < toggle.right() && y >= toggle.y && y < toggle.bottom() {
                return SettingsHit::TogglePlugin(card_index);
            }
            let uninstall =
                bento_nano_app::settings_panel::settings_plugin_uninstall_button_rect(card);
            if app.settings_plugin_uninstall_confirm.get() == Some(card_index) {
                let cancel =
                    bento_nano_app::settings_panel::settings_plugin_uninstall_cancel_button_rect(
                        card,
                    );
                if x >= cancel.x && x < cancel.right() && y >= cancel.y && y < cancel.bottom() {
                    return SettingsHit::CancelUninstallPlugin;
                }
                if x >= uninstall.x
                    && x < uninstall.right()
                    && y >= uninstall.y
                    && y < uninstall.bottom()
                {
                    return SettingsHit::ConfirmUninstallPlugin(card_index);
                }
            } else if x >= uninstall.x
                && x < uninstall.right()
                && y >= uninstall.y
                && y < uninstall.bottom()
            {
                return SettingsHit::UninstallPlugin(card_index);
            }
        }
    }

    // M6-UI — §3 Appearance inline theme grid. Flows after the Plugins section;
    // its anchor + content width come from `settings_panel` (shared with the
    // renderer so paint geometry == hit geometry), and the card / accent-swatch
    // rects come from `theme_picker::appearance_layout`. Card click →
    // `SelectTheme(id)` (live re-skin), accent swatch click → `SelectAccent`.
    let appearance_origin = bento_nano_app::settings_panel::settings_appearance_grid_origin(
        vp,
        scroll_y,
        &plugin_flags,
    );
    let appearance_inner_w = bento_nano_app::settings_panel::settings_appearance_inner_width(vp);
    let appearance =
        bento_nano_app::theme_picker::appearance_layout(appearance_origin, appearance_inner_w);
    match bento_nano_app::theme_picker::appearance_hit_test(&appearance, x, y) {
        Some(bento_nano_app::theme_picker::AppearanceHit::Card(id)) => {
            return SettingsHit::SelectTheme(id);
        }
        Some(bento_nano_app::theme_picker::AppearanceHit::Accent(idx)) => {
            return SettingsHit::SelectAccent(idx);
        }
        Some(bento_nano_app::theme_picker::AppearanceHit::AccentEditor) => {
            return SettingsHit::EditAccentColor;
        }
        Some(bento_nano_app::theme_picker::AppearanceHit::AccentPicker) => {
            return SettingsHit::OpenAccentColorPicker;
        }
        Some(bento_nano_app::theme_picker::AppearanceHit::AccentClear) => {
            return SettingsHit::ClearAccentColor;
        }
        None => {}
    }

    SettingsHit::Body
}
