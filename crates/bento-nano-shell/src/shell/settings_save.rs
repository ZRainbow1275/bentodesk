//! Native shell owner: `settings_save`.

use super::*;

pub(super) fn save_settings_general(root: &AppRoot, _settings_hwnd: HWND) -> bool {
    let (dirty, desired, previous, accent_draft, accent_clear_requested) = {
        let app = root.app.borrow();
        (
            app.settings_dirty.get(),
            app.snapshot_settings(),
            app.settings_snapshot
                .borrow()
                .clone()
                .unwrap_or_else(|| app.snapshot_settings()),
            app.settings_valid_accent_draft(),
            app.settings_accent_clear_requested.get(),
        )
    };
    if !dirty {
        return false;
    }
    let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
    {
        let app = root.app.borrow();
        app.settings_save_error.borrow_mut().take();
    }
    let desired_sources = match validate_settings_sources(&desired) {
        Ok(sources) => sources,
        Err(error) => return settings_save_failure(root, error),
    };

    let Some(mtx) = bento_nano_backend::config_vault::Vault::global() else {
        tracing::warn!(
            target: "bentodesk::vault",
            "settings: SaveSettings rejected — vault global not installed"
        );
        return settings_save_failure(
            root,
            localized_message(
                zh,
                "设置存储尚未初始化",
                "Settings storage is not initialized",
            ),
        );
    };
    let original_values = {
        let Ok(mut vault) = mtx.lock() else {
            return settings_save_failure(
                root,
                localized_message(
                    zh,
                    "设置存储锁已损坏",
                    "Settings storage lock is unavailable",
                ),
            );
        };
        if vault.is_locked_passphrase() {
            return settings_save_failure(
                root,
                localized_message(
                    zh,
                    "请先解锁设置加密，再保存",
                    "Unlock encrypted settings before saving",
                ),
            );
        }
        let original_values = snapshot_vault_settings(&vault);
        persist_settings_snapshot_to_vault(
            &mut vault,
            &desired,
            accent_draft.as_ref(),
            accent_clear_requested,
        );
        if let Err(error) = vault.flush() {
            restore_vault_settings(&mut vault, &original_values);
            let _ = vault.flush();
            return settings_save_failure(
                root,
                localized_message(
                    zh,
                    format!("设置写入失败：{error}"),
                    format!("Unable to write settings: {error}"),
                ),
            );
        }
        original_values
    };

    if let Err(error) = apply_runtime_settings(root, &previous, &desired, &desired_sources) {
        if let Ok(mut vault) = mtx.lock() {
            restore_vault_settings(&mut vault, &original_values);
            if let Err(rollback_error) = vault.flush() {
                tracing::error!(
                    target: "bentodesk::vault",
                    %rollback_error,
                    "settings rollback flush failed"
                );
            }
        }
        let previous_sources =
            validate_settings_sources(&previous).unwrap_or_else(|_| desired_sources.clone());
        if let Err(rollback_error) =
            apply_runtime_settings(root, &desired, &previous, &previous_sources)
        {
            tracing::error!(
                target: "bentodesk::settings",
                %rollback_error,
                "native Settings side-effect rollback failed"
            );
        }
        return settings_save_failure(root, error);
    }

    let app = root.app.borrow();
    app.settings_dirty.set(false);
    app.settings_snapshot.borrow_mut().take();
    app.settings_save_error.borrow_mut().take();
    // M6-UI — fold the saved accent into the live `theme_base_accent` so the
    // ringed swatch + zone accents reflect the persisted value, then clear the
    // in-flight draft (Save consumes it).
    if accent_clear_requested {
        *app.theme_base_accent.borrow_mut() = None;
    } else if let Some(accent) = accent_draft {
        *app.theme_base_accent.borrow_mut() = Some(accent);
    }
    app.settings_draft_accent_color.borrow_mut().take();
    app.settings_accent_clear_requested.set(false);
    true
}

/// M1a 2026-05-29 — discard pending General-section edits by replaying the
/// snapshot taken on `OpenSettings`. Called by Cancel, Escape, Close × and
/// click-outside dismissals so a dropped panel never leaks unflushed
/// toggles into the vault on next save. Idempotent when no snapshot
/// exists (first call clears it).
pub(super) fn cancel_settings_general(root: &AppRoot) {
    let snapshot = {
        let app = root.app.borrow();
        let snapshot = app.settings_snapshot.borrow_mut().take();
        if let Some(snap) = snapshot.as_ref() {
            app.restore_settings(snap);
        }
        app.settings_dirty.set(false);
        app.settings_save_error.borrow_mut().take();
        // M6-UI — discard the in-flight §3 Appearance accent draft (Cancel reverts
        // the edit; the persisted `theme_base_accent` is untouched).
        app.settings_draft_accent_color.borrow_mut().take();
        app.settings_accent_clear_requested.set(false);
        // W-minor (#7 fix wave) — clear the focused-field caret on Cancel/Escape so
        // a stale focus/caret never leaks past the dismissal.
        app.settings_focused_field
            .set(bento_nano_app::SettingsTextField::None);
        snapshot
    };
    if let Some(snapshot) = snapshot {
        if let Err(error) = apply_active_theme_to_app(root, snapshot.active_theme_id.clone()) {
            tracing::warn!(
                target: "bentodesk::themes",
                theme_id = %snapshot.active_theme_id,
                %error,
                "Settings Cancel could not restore custom theme"
            );
        }
        request_theme_surface_redraw(root, false);
    }
}
