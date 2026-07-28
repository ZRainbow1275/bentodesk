//! Command handlers for the `window` domain.

use super::*;

pub(super) fn dispatch(
    root: &AppRoot,
    hwnd: HWND,
    command: Command,
    effects: &mut DispatchEffects,
) {
    match command {
        Command::TogglePin => {
            let app = root.app.borrow();
            let next = !app.is_pinned.get();
            app.is_pinned.set(next);
            let z = if next { HWND_TOPMOST } else { HWND_NOTOPMOST };
            // SAFETY: SetWindowPos canonical; flags skip move/size/activate.
            unsafe {
                SetWindowPos(
                    hwnd,
                    z,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
        Command::ToggleSettings => {
            let next_open = !root.app.borrow().settings_open.get();
            if next_open {
                let _shown = show_settings_surface(root);
            } else {
                let _hidden = close_settings_surface(root);
            }
            effects.needs_redraw = true;
        }
        Command::CloseSettings => {
            let _hidden = close_settings_surface(root);
            effects.needs_redraw = true;
        }
        Command::OpenSettings => {
            let _shown = show_settings_surface(root);
            effects.needs_redraw = true;
        }
        Command::OpenAbout => {
            {
                let app = root.app.borrow();
                app.about_open.set(true);
                app.settings_open.set(false);
                reset_settings_transient_state(&app);
            }
            let _about = bentodesk_app::business::about::build();
            if let Some(target) = ensure_aux_window(root, WindowKind::About) {
                center_about_aux_window(target);
                // SAFETY: target is a live About HWND owned by this UI
                // thread. The shared focus helper handles foreground input
                // queue attachment and detaches immediately afterwards.
                unsafe { ShowWindow(target, SW_SHOW) };
                focus_window_for_keyboard(target);
            } else {
                tracing::warn!(
                    target: "bentodesk::about",
                    "OpenAbout: ensure_aux_window failed; overlay remains visible on main window"
                );
            }
            effects.needs_redraw = true;
        }
        Command::CloseAbout => {
            {
                let app = root.app.borrow();
                app.about_open.set(false);
            }
            if let Some(target) = find_aux_window(root, WindowKind::About) {
                // SAFETY: ShowWindow with SW_HIDE on a HWND we own.
                unsafe { ShowWindow(target, SW_HIDE) };
            }
            effects.needs_redraw = true;
        }
        Command::ToggleDebugOverlay => {
            let next_visible = {
                let app = root.app.borrow();
                let mut overlay = app.debug_overlay.borrow_mut();
                overlay.toggle();
                overlay.visible
            };
            if let Some(mtx) = bentodesk_backend::config_vault::Vault::global() {
                match mtx.lock() {
                    Ok(mut vault) => {
                        vault.set_setting(
                            SETTING_DEBUG_OVERLAY,
                            bentodesk_backend::config_vault::SettingValue::Bool(next_visible),
                        );
                        if let Err(e) = vault.flush() {
                            tracing::warn!(
                                target: "bentodesk::debug_overlay",
                                error = %e,
                                "ToggleDebugOverlay: persisted setting flush failed"
                            );
                        }
                    }
                    Err(_poisoned) => {
                        tracing::warn!(
                            target: "bentodesk::debug_overlay",
                            "ToggleDebugOverlay: vault mutex poisoned; runtime toggle kept"
                        );
                    }
                }
            } else {
                tracing::warn!(
                    target: "bentodesk::debug_overlay",
                    "ToggleDebugOverlay: vault not initialised; runtime toggle kept"
                );
            }
            tracing::info!(
                target: "bentodesk::debug_overlay",
                "ToggleDebugOverlay: visible={}",
                next_visible
            );
            log_static(format!("ToggleDebugOverlay: visible={next_visible}\n").as_str());
            effects.needs_redraw = true;
        }
        Command::ToggleLocale => {
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                bentodesk_style::set_locale(&bentodesk_style::EN_US);
            } else {
                bentodesk_style::set_locale(&bentodesk_style::ZH_CN);
            }
            effects.needs_redraw = true;
        }
        Command::HideWindow(kind) => {
            match kind {
                WindowKind::Main => {
                    // SAFETY: ShowWindow canonical. Tray icon survives.
                    unsafe { ShowWindow(hwnd, SW_HIDE) };
                }
                // F2-02 — aux windows: hide via SW_HIDE so the slot
                // stays registered and T-099 hibernates the swap chain
                // ~500 ms later. A subsequent ShowWindow(kind) reuses
                // the same HWND without paying the create cost again.
                other => {
                    if let Some(target) = find_aux_window(root, other) {
                        // SAFETY: ShowWindow canonical for any owned HWND.
                        unsafe { ShowWindow(target, SW_HIDE) };
                    } else {
                        tracing::debug!(
                            target: "bentodesk::dispatcher",
                            ?other,
                            "HideWindow: no aux HWND of this kind currently registered"
                        );
                    }
                }
            }
        }
        Command::ShowWindow(kind) => {
            match kind {
                WindowKind::Main => {
                    // Frosted-backdrop — the Main overlay is transitioning to
                    // visible (tray click / hotkey ToggleMain). The desktop
                    // behind it may have changed while it was hidden (the
                    // user switched apps / wallpaper / virtual desktop), so
                    // mark the captured snapshot stale; the next Main paint
                    // re-captures. Reach the Main renderer via the registry
                    // by kind (robust to whichever HWND is pumping).
                    if let Some(slot) = root
                        .registry
                        .borrow_mut()
                        .iter_mut()
                        .find(|slot| slot.kind == WindowKind::Main)
                    {
                        slot.renderer.mark_backdrop_dirty();
                    }
                    // SAFETY: canonical sequence — show then bring to front.
                    unsafe {
                        ShowWindow(hwnd, SW_SHOW);
                        SetForegroundWindow(hwnd);
                    }
                }
                // F2-02 — aux windows: lazy-spawn on first show via the
                // factory; subsequent shows reuse the registered HWND.
                // ContextMenu / Tooltip / DragPreview / MiniBar use
                // WS_EX_NOACTIVATE so SetForegroundWindow is skipped
                // (the kind picks WS_EX_NOACTIVATE in `ex_style_for`,
                // and SetForegroundWindow on a NoActivate window is a
                // no-op that still steals the activation tick).
                other => {
                    if let Some(target) = ensure_aux_window(root, other) {
                        // SAFETY: ShowWindow canonical for any owned HWND.
                        unsafe { ShowWindow(target, SW_SHOW) };
                        if !matches!(
                            other,
                            WindowKind::ContextMenu
                                | WindowKind::Tooltip
                                | WindowKind::DragPreview
                                | WindowKind::MiniBar
                        ) {
                            // SAFETY: SetForegroundWindow canonical for
                            //         focusable kinds (Picker / Settings).
                            unsafe { SetForegroundWindow(target) };
                        }
                    }
                }
            }
        }
        Command::ShowTrayMenu => {
            // SAFETY: TrackPopupMenu loop is canonical — see Ruling B.
            unsafe { show_tray_menu(root, hwnd) };
        }
        _ => unreachable!("command routed to the wrong window dispatcher"),
    }
}
