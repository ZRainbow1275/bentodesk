//! Settings pointer-command producer.

use super::*;

pub(super) fn handle_settings_lbutton_down(root: &AppRoot, hwnd: HWND, x: f32, y: f32) {
    let app = root.app.borrow();
    let settings_hit = ui::settings_hit(&app, x, y);
    let viewport = app.viewport;
    log_static(
        format!(
            "settings: lbutton_down x={x:.1} y={y:.1} viewport={:.1}x{:.1} hit={settings_hit:?}\n",
            viewport.width, viewport.height
        )
        .as_str(),
    );
    drop(app);
    // A tooltip is a hover affordance, not a second persistent window.
    // Dismiss it as soon as the represented Settings control is clicked.
    hide_tooltip(root);
    match settings_hit {
        ui::SettingsHit::SwitchLocale => {
            queue_locale_setting_toggle(root);
        }
        ui::SettingsHit::OpenKeybindings => {
            let app = root.app.borrow();
            app.settings_keybindings_open.set(true);
            app.settings_keybinding_recording.borrow_mut().take();
            app.settings_keybinding_feedback.borrow_mut().take();
            request_redraw(hwnd);
        }
        // M1h — `OpenPlugins` / `ClosePlugins` / `RefreshPlugins` arms were
        // removed: the Plugins surface is an always-inline §11 section (no
        // modal to open/close), and the list refreshes on Settings open via
        // `refresh_settings_plugins_for_root` in `show_settings_surface`
        // (mirroring how Stealth/Backup refresh). Install / Toggle /
        // Uninstall keep their real dispatch arms below.
        ui::SettingsHit::CloseKeybindings => {
            let app = root.app.borrow();
            app.settings_keybindings_open.set(false);
            app.settings_keybinding_recording.borrow_mut().take();
            request_redraw(hwnd);
        }
        ui::SettingsHit::InstallPlugin => {
            root.app
                .borrow()
                .settings_plugin_uninstall_confirm
                .set(None);
            let selected = unsafe { select_plugin_file_from_dialog(hwnd) };
            arm_settings_owned_dialog_release_guard(root);
            match selected {
                Ok(Some(path)) => {
                    root.dispatcher
                        .push(Command::InstallPlugin(SmolStr::new(path.to_string_lossy())));
                }
                Ok(None) => {
                    set_plugin_setting_success(
                        root,
                        SmolStr::new(bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_INSTALL_CANCELLED,
                        )),
                    );
                    request_redraw(hwnd);
                }
                Err(error) => {
                    set_plugin_setting_error(
                        root,
                        localized_plugin_message(
                            bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_INSTALL_FAILED_PREFIX,
                            error,
                        ),
                    );
                    request_redraw(hwnd);
                }
            }
        }
        ui::SettingsHit::TogglePlugin(row_index) => {
            let target = {
                let app = root.app.borrow();
                app.settings_plugin_uninstall_confirm.set(None);
                app.settings_plugin_entries
                    .borrow()
                    .get(row_index)
                    .map(|entry| (entry.id.clone(), !entry.enabled))
            };
            if let Some((plugin_id, enabled)) = target {
                root.dispatcher
                    .push(Command::TogglePlugin(plugin_id, enabled));
            }
        }
        ui::SettingsHit::UninstallPlugin(row_index) => {
            let exists = {
                let app = root.app.borrow();
                app.settings_plugin_entries
                    .borrow()
                    .get(row_index)
                    .is_some()
            };
            if exists {
                root.app
                    .borrow()
                    .settings_plugin_uninstall_confirm
                    .set(Some(row_index));
                request_redraw(hwnd);
            }
        }
        ui::SettingsHit::ConfirmUninstallPlugin(row_index) => {
            let plugin_id = {
                let app = root.app.borrow();
                app.settings_plugin_uninstall_confirm.set(None);
                app.settings_plugin_entries
                    .borrow()
                    .get(row_index)
                    .map(|entry| entry.id.clone())
            };
            if let Some(plugin_id) = plugin_id {
                root.dispatcher.push(Command::UninstallPlugin(plugin_id));
            }
        }
        ui::SettingsHit::CancelUninstallPlugin => {
            root.app
                .borrow()
                .settings_plugin_uninstall_confirm
                .set(None);
            request_redraw(hwnd);
        }
        ui::SettingsHit::RecordKeybinding(row_index) => {
            if let Some(action) = keybinding_action_at(row_index) {
                let app = root.app.borrow();
                app.settings_keybinding_recording
                    .borrow_mut()
                    .replace(SmolStr::new_static(action));
                app.settings_keybinding_feedback.borrow_mut().take();
                request_redraw(hwnd);
            }
        }
        ui::SettingsHit::ResetKeybinding(row_index) => {
            if let Some(action) = keybinding_action_at(row_index) {
                root.dispatcher.push(Command::ResetKeybinding {
                    action: SmolStr::new_static(action),
                });
            }
        }
        ui::SettingsHit::CycleUpdateFrequency => {
            queue_update_frequency_cycle(root);
        }
        ui::SettingsHit::CheckForUpdates => {
            root.dispatcher.push(Command::CheckForUpdates);
        }
        ui::SettingsHit::ToggleUpdateAutoDownload => {
            queue_update_auto_download_toggle(root);
        }
        ui::SettingsHit::RunUpdateAction => {
            queue_update_action(root);
        }
        ui::SettingsHit::SkipCurrentUpdate => {
            queue_update_skip(root, hwnd);
        }
        ui::SettingsHit::ToggleStealthEnabled => {
            queue_stealth_enabled_toggle(root);
        }
        ui::SettingsHit::SelectEncryptionModeNone => {
            // M7 — §10 None button → SetSetting{ "encryption.mode", "None" }
            // direct. Clear any in-flight passphrase capture + field focus.
            {
                let app = root.app.borrow();
                app.passphrase_entry_active.set(false);
                app.passphrase_draft.borrow_mut().clear();
                app.settings_focused_field
                    .set(bento_nano_app::SettingsTextField::None);
            }
            root.dispatcher.push(encryption_mode_setting_command_for(
                SettingsEncryptionMode::None,
            ));
        }
        ui::SettingsHit::SelectEncryptionModeDpapi => {
            // M7 — §10 DPAPI button → SetSetting{ "encryption.mode", "Dpapi" }
            // direct. Clear any in-flight passphrase capture + field focus.
            {
                let app = root.app.borrow();
                app.passphrase_entry_active.set(false);
                app.passphrase_draft.borrow_mut().clear();
                app.settings_focused_field
                    .set(bento_nano_app::SettingsTextField::None);
            }
            root.dispatcher.push(encryption_mode_setting_command_for(
                SettingsEncryptionMode::Dpapi,
            ));
        }
        ui::SettingsHit::FocusPassphraseField => {
            // P15 (#7 fix wave 2026-06-01) — clicking the passphrase INPUT is
            // PURE FOCUS (matching Tauri: the input's focus never applies a
            // mode; only the Passphrase BUTTON does). Delegates to the pure
            // `focus_passphrase_field` seam (unit-tested) which sets focus +
            // the char-capture flag WITHOUT switching the mode/purpose to an
            // apply and WITHOUT clearing the draft. P10 — no ASCII prompt
            // banner: the input PLACEHOLDER already serves as the prompt.
            {
                let app = root.app.borrow();
                focus_passphrase_field(&app);
            }
            request_redraw(hwnd);
        }
        ui::SettingsHit::SelectEncryptionModePassphrase => {
            // P15 (#7 fix wave) — the Passphrase BUTTON applies, mirroring
            // Tauri `applyMode("Passphrase")`. Delegates to the pure
            // `passphrase_button_command` seam (unit-tested): empty draft →                // sets the localized ENCRYPTION_REQUIRED error + returns `None`;
            // otherwise clears the in-flight capture + returns the
            // verify-probe→apply command (`SetEncryptionPassphrase` reopens the
            // vault with the passphrase, which IS the probe — a bad passphrase
            // fails the reopen and surfaces an error; Unlock routes to the
            // unlock command). P10 — no ASCII prompt banner.
            let command = {
                let app = root.app.borrow();
                passphrase_button_command(&app)
            };
            if let Some(command) = command {
                root.dispatcher.push(command);
            }
            request_redraw(hwnd);
        }
        ui::SettingsHit::OpenThemeBasePalette => {
            log_static("settings: OpenThemeBasePalette producer\n");
            root.dispatcher.push(Command::OpenPalettePicker {
                target: PaletteTarget::ThemeBase,
            });
        }
        ui::SettingsHit::ImportTheme => {
            let selected = unsafe { select_theme_file_from_dialog(hwnd) };
            arm_settings_owned_dialog_release_guard(root);
            match selected {
                Ok(Some(path)) => {
                    root.dispatcher
                        .push(Command::ImportTheme(SmolStr::new(path.to_string_lossy())));
                }
                Ok(None) => {
                    set_theme_setting_success(
                        root,
                        localized_current("已取消导入主题", "Theme import cancelled"),
                    );
                    request_redraw(hwnd);
                }
                Err(error) => {
                    set_theme_setting_error(
                        root,
                        localized_current(
                            format!("导入主题失败：{error}"),
                            format!("Theme import failed: {error}"),
                        ),
                    );
                    request_redraw(hwnd);
                }
            }
        }
        ui::SettingsHit::SelectTheme(id) => {
            // M6-UI — §3 Appearance grid: a ThemeCard click re-skins the
            // app live end-to-end. Resolve the preset's stable string
            // theme id and route it through M6a's `apply_active_theme_by_id`
            // (all 17 builtins resolve to a byte-exact PaletteTauri +
            // ThemeTokens). Also dispatch the backend `SetActiveTheme` so
            // the choice persists through the config vault, and mark the
            // panel dirty so Save lights up (Tauri `setTheme(id); setDirty`).
            if let Some(preset) = bento_nano_app::theme_picker::BUILTIN_THEMES
                .iter()
                .find(|p| p.id == id)
            {
                let theme_id = preset.theme_id;
                // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
                let now_ms = unsafe { GetTickCount() };
                let app = root.app.borrow();
                let transition_from = app.active_theme_card_id();
                let changed = app.apply_active_theme_by_id(theme_id).unwrap_or(false);
                let transition_started =
                    changed && app.start_theme_transition_from(transition_from, now_ms);
                app.settings_dirty.set(true);
                drop(app);
                if changed {
                    if transition_started {
                        log_static(
                            format!(
                                "theme: transition start id={theme_id} duration_ms={}\n",
                                bento_nano_app::state::THEME_TRANSITION_MS
                            )
                            .as_str(),
                        );
                        request_theme_surface_redraw(root, true);
                    } else {
                        request_theme_surface_redraw(root, false);
                    }
                    request_redraw(hwnd);
                } else {
                    // Re-selecting the active theme still re-arms Save +
                    // repaints the dirty footer.
                    request_redraw(hwnd);
                }
            }
        }
        ui::SettingsHit::SelectAccent(index) => {
            // M6-UI — §3 Appearance accent row (Control B): write the picked
            // VIBRANT swatch hex into the in-flight draft + mark dirty. The
            // value persists on Save via the `accent_color` config-vault key
            // (wired in the SaveSettings path below).
            if let Some(hex) = bento_nano_app::theme_picker::accent_swatch_hex(index as usize) {
                let app = root.app.borrow();
                app.set_settings_accent_color_from_picker(SmolStr::new_static(hex));
                drop(app);
                request_redraw(hwnd);
            }
        }
        ui::SettingsHit::EditAccentColor => {
            // V21-N15 — focus the inline `#rrggbb` accent editor. It shares
            // the Settings text-field producer path with Paths, but filters
            // input in AppState so only valid hex can persist on Save.
            {
                let app = root.app.borrow();
                app.focus_settings_accent_color();
                app.passphrase_entry_active.set(false);
            }
            request_redraw(hwnd);
        }
        ui::SettingsHit::OpenAccentColorPicker => {
            log_static("settings: OpenAccentColorPicker producer\n");
            if open_settings_native_accent_picker(root, hwnd) {
                request_redraw(hwnd);
            }
        }
        ui::SettingsHit::ClearAccentColor => {
            log_static("settings: ClearAccentColor producer\n");
            {
                let app = root.app.borrow();
                app.request_settings_accent_clear();
                app.passphrase_entry_active.set(false);
            }
            request_redraw(hwnd);
        }
        ui::SettingsHit::CycleZoneDisplayMode => {
            queue_zone_display_mode_cycle(root);
        }
        ui::SettingsHit::SetZoneDisplayMode(mode) => {
            queue_zone_display_mode_set(root, mode);
        }
        ui::SettingsHit::CreateSettingsBackup => {
            root.dispatcher.push(Command::CreateSettingsBackup);
        }
        ui::SettingsHit::ListSettingsBackups => {
            root.dispatcher.push(Command::ListSettingsBackups);
        }
        ui::SettingsHit::RestoreLatestSettingsBackup => {
            root.dispatcher.push(Command::RestoreLatestSettingsBackup);
        }
        ui::SettingsHit::CreateRecoveryBundle => {
            root.dispatcher.push(Command::CreateRecoveryBundle);
        }
        ui::SettingsHit::ExportRecoveryDiagnostics => {
            root.dispatcher.push(Command::ExportRecoveryDiagnostics);
        }
        ui::SettingsHit::RestoreRecoveryBundle => {
            root.dispatcher.push(Command::RestoreRecoveryBundle);
        }
        ui::SettingsHit::RestoreSettingsBackup(entry_index) => {
            let backup_id = {
                let app = root.app.borrow();
                app.settings_backup_entries
                    .borrow()
                    .get(entry_index)
                    .map(|entry| entry.id.clone())
            };
            if let Some(backup_id) = backup_id {
                root.dispatcher
                    .push(Command::RestoreSettingsBackup(backup_id));
            }
        }
        ui::SettingsHit::Close | ui::SettingsHit::Outside => {
            // M1a 2026-05-29 — Close × (header) and Outside (click-out)
            // discard pending General edits, matching Tauri's behaviour
            // where dismissing the panel without Save reverts the React
            // store (`SettingsPanel.tsx:208-214,250`).
            cancel_settings_general(root);
            root.dispatcher.push(Command::CloseSettings);
        }
        ui::SettingsHit::CancelSettings => {
            // M1a 2026-05-29 — revert any in-memory General toggle edits
            // from the snapshot taken at OpenSettings, clear dirty, then
            // close. Mirrors Tauri Cancel which discards the React store
            // diff (`SettingsPanel.tsx:208-214` `handleClose`).
            cancel_settings_general(root);
            root.dispatcher.push(Command::CloseSettings);
        }
        ui::SettingsHit::SaveSettings => {
            // M1a 2026-05-29 — persist General toggles to the vault, but
            // only when `settings_dirty` flips true. Save dims (alpha
            // 0.4) in the renderer when clean — clicking it through is
            // a no-op short-circuit, matching Tauri `disabled={!dirty()}`.
            if save_settings_general(root, hwnd) {
                root.dispatcher.push(Command::CloseSettings);
            } else {
                request_redraw(hwnd);
            }
        }
        ui::SettingsHit::ToggleDesktopEmbed => {
            let app = root.app.borrow();
            app.setting_desktop_embed
                .set(!app.setting_desktop_embed.get());
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::ToggleAutostart => {
            let app = root.app.borrow();
            // Save-gated like the Tauri panel: Cancel must never leave an
            // HKCU\Run mutation behind. The real registry side effect is
            // applied transactionally by `save_settings_general`.
            app.setting_autostart.set(!app.setting_autostart.get());
            app.settings_dirty.set(true);
            app.settings_save_error.borrow_mut().take();
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::ToggleShowInTaskbar => {
            let app = root.app.borrow();
            app.setting_show_in_taskbar
                .set(!app.setting_show_in_taskbar.get());
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::ToggleSmartLayout => {
            let app = root.app.borrow();
            app.setting_smart_layout
                .set(!app.setting_smart_layout.get());
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::TogglePortableMode => {
            let app = root.app.borrow();
            app.setting_portable_mode
                .set(!app.setting_portable_mode.get());
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::OpenLocaleMenu => {
            queue_locale_setting_toggle(root);
        }
        ui::SettingsHit::ScrollBodyDelta(delta) => {
            handle_settings_scroll_delta(root, hwnd, delta);
        }
        ui::SettingsHit::RefreshDesktopSources => {
            // M1i — re-resolve the real Desktop sources and repopulate the
            // cached read-only list, then redraw (Tauri `↻` parity).
            refresh_desktop_sources(root);
            request_redraw(hwnd);
        }
        ui::SettingsHit::EditDesktopPath => {
            // M7 — focus the 妗岄潰璺緞 single-line input for live keyboard
            // editing. Mutually exclusive with passphrase capture.
            {
                let app = root.app.borrow();
                app.settings_focused_field
                    .set(bento_nano_app::SettingsTextField::DesktopPath);
                app.passphrase_entry_active.set(false);
            }
            request_redraw(hwnd);
        }
        ui::SettingsHit::EditWatchValues => {
            // M7 — focus the 鐩戞帶鍊?multi-line textarea for live keyboard
            // editing. Mutually exclusive with passphrase capture.
            {
                let app = root.app.borrow();
                app.settings_focused_field
                    .set(bento_nano_app::SettingsTextField::WatchValues);
                app.passphrase_entry_active.set(false);
            }
            request_redraw(hwnd);
        }
        ui::SettingsHit::DragPerformanceSlider { index, x_q } => {
            use bento_nano_app::settings_panel::settings_performance_slider_rect;
            use bento_nano_app::state::{
                COLLAPSE_DELAY_MAX_MS, COLLAPSE_DELAY_MIN_MS, COLLAPSE_DELAY_STEP_MS,
                EXPAND_DELAY_MAX_MS, EXPAND_DELAY_MIN_MS, EXPAND_DELAY_STEP_MS, ICON_CACHE_MAX,
                ICON_CACHE_MIN, ICON_CACHE_STEP, slider_fraction_to_value,
            };
            let app = root.app.borrow();
            let vp = app.viewport;
            let scroll_y = app.scroll_offset_y.get();
            let track = settings_performance_slider_rect(vp, scroll_y, index);
            let span = track.width.max(1.0);
            let frac = (x_q as f32 - track.x) / span;
            let next = match index {
                0 => slider_fraction_to_value(
                    frac,
                    EXPAND_DELAY_MIN_MS,
                    EXPAND_DELAY_MAX_MS,
                    EXPAND_DELAY_STEP_MS,
                ),
                1 => slider_fraction_to_value(
                    frac,
                    COLLAPSE_DELAY_MIN_MS,
                    COLLAPSE_DELAY_MAX_MS,
                    COLLAPSE_DELAY_STEP_MS,
                ),
                _ => {
                    slider_fraction_to_value(frac, ICON_CACHE_MIN, ICON_CACHE_MAX, ICON_CACHE_STEP)
                }
            };
            match index {
                0 => app.expand_delay_ms.set(next),
                1 => app.collapse_delay_ms.set(next),
                _ => app.icon_cache_size.set(next),
            }
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::ToggleStartupHighPriority => {
            let app = root.app.borrow();
            app.startup_high_priority
                .set(!app.startup_high_priority.get());
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::ToggleCrashRestart => {
            let app = root.app.borrow();
            app.crash_restart_enabled
                .set(!app.crash_restart_enabled.get());
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::IncCrashMaxRetries => {
            use bento_nano_app::state::CRASH_MAX_RETRIES_MAX;
            let app = root.app.borrow();
            let next = (app.crash_max_retries.get() + 1).min(CRASH_MAX_RETRIES_MAX);
            app.crash_max_retries.set(next);
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::DecCrashMaxRetries => {
            use bento_nano_app::state::CRASH_MAX_RETRIES_MIN;
            let app = root.app.borrow();
            let next = (app.crash_max_retries.get() - 1).max(CRASH_MAX_RETRIES_MIN);
            app.crash_max_retries.set(next);
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::IncCrashWindowSecs => {
            use bento_nano_app::state::CRASH_WINDOW_SECS_MAX;
            let app = root.app.borrow();
            let next = (app.crash_window_secs.get() + 1).min(CRASH_WINDOW_SECS_MAX);
            app.crash_window_secs.set(next);
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::DecCrashWindowSecs => {
            use bento_nano_app::state::CRASH_WINDOW_SECS_MIN;
            let app = root.app.borrow();
            let next = (app.crash_window_secs.get() - 1).max(CRASH_WINDOW_SECS_MIN);
            app.crash_window_secs.set(next);
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::ToggleSafeStartHibernation => {
            let app = root.app.borrow();
            app.safe_start_after_hibernation
                .set(!app.safe_start_after_hibernation.get());
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::DragHibernateDelay(x_q) => {
            use bento_nano_app::settings_panel::settings_hibernate_slider_rect;
            use bento_nano_app::state::{
                HIBERNATE_DELAY_MAX_MS, HIBERNATE_DELAY_MIN_MS, HIBERNATE_DELAY_STEP_MS,
                slider_fraction_to_value,
            };
            let app = root.app.borrow();
            let vp = app.viewport;
            let scroll_y = app.scroll_offset_y.get();
            let crash_restart_on = app.crash_restart_enabled.get();
            let track = settings_hibernate_slider_rect(vp, scroll_y, crash_restart_on);
            let span = track.width.max(1.0);
            let frac = (x_q as f32 - track.x) / span;
            let next = slider_fraction_to_value(
                frac,
                HIBERNATE_DELAY_MIN_MS,
                HIBERNATE_DELAY_MAX_MS,
                HIBERNATE_DELAY_STEP_MS,
            );
            app.hibernate_resume_delay_ms.set(next);
            app.settings_dirty.set(true);
            drop(app);
            request_redraw(hwnd);
        }
        ui::SettingsHit::RefreshStealth => {
            // M1e — re-read the synchronous stealth status probe into the
            // cached snapshot the card paints from. Real backend call, no
            // no-op: `stealth::status()` reflects the live AttrGuard state.
            refresh_stealth_status(root);
            request_redraw(hwnd);
        }
        ui::SettingsHit::ReapplyStealth => {
            // M1e —重新应用: build the live StealthConfig and re-write the
            // HIDDEN+SYSTEM attributes via `reapply_hidden_on_startup`,
            // then refresh the cached status so the pill/rows update.
            // Graceful on a missing config (no panic): log + skip the
            // reapply, but still refresh the snapshot.
            match stealth_config_now(root) {
                Some(config) => {
                    match bento_nano_backend::stealth::sync::reapply_hidden_on_startup(&config) {
                        Ok(count) => {
                            tracing::info!(
                                target: "bentodesk::stealth",
                                files = count,
                                "Settings Reapply: re-applied stealth attributes"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "bentodesk::stealth",
                                error = %e,
                                "Settings Reapply: reapply_hidden_on_startup failed"
                            );
                        }
                    }
                }
                None => {
                    tracing::warn!(
                        target: "bentodesk::stealth",
                        "Settings Reapply: no desktop dir / stealth config; refreshing status only"
                    );
                }
            }
            refresh_stealth_status(root);
            request_redraw(hwnd);
        }
        ui::SettingsHit::Body => {}
    }
}
