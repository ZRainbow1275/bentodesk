use super::*;
use crate::settings_panel::*;

impl Renderer {
    pub(super) fn draw_settings_backup(
        &mut self,
        app: &AppState,
        context: SettingsRenderContext,
        scroll: f32,
        updater_flags: SettingsBodyFlags,
    ) -> Result<SettingsBodyFlags, RenderError> {
        let SettingsRenderContext {
            viewport,
            body,
            palette,
            title_color,
            label_color,
            accent_on,
            ..
        } = context;
        let row_visible =
            |row: Rect, body: Rect| -> bool { row.bottom() > body.y && row.y < body.bottom() };
        let controls = palette.control_palette();
        // ── M1g — Backup §9 card (`BackupCard.tsx`) ─────────────────────
        //
        // Sits after Updater in the Tauri body order. Reads the live
        // `app.settings_backup_entries` snapshot (populated on Settings open +
        // after every create/restore by the shell). The list is
        // variable-length, capped at SETTINGS_BACKUP_ROW_VISIBLE_MAX; the
        // capped count threads through the same `SettingsBodyFlags` the
        // hit-tester + scroll-clamp use (via `with_backup_rows`) so paint and
        // hit geometry agree. Size + empty-state + the capped count come from
        // the lib helpers in `business::settings::backup_card`.
        use crate::business::settings::backup_card as bkp;
        // Snapshot the entries + status text out of the RefCells BEFORE the
        // fallible paint calls so no borrow spans them (mirrors the Stealth
        // snapshot pattern above).
        let backup_entries = app.settings_backup_entries.borrow().clone();
        let backup_status_snapshot = app.settings_backup_status.borrow().clone();
        let backup_visible = bkp::backup_visible_row_count(&backup_entries);
        let backup_flags = updater_flags
            .with_backup_rows(backup_visible)
            .with_backup_status(backup_status_snapshot.is_some())
            .with_encryption_status(app.settings_encryption_status.borrow().is_some());
        let backup_label = settings_backup_label_rect(viewport, scroll, &backup_flags);
        if row_visible(backup_label, body) {
            self.draw_text_no_wrap_with_style(
                bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::BACKUP_CARD_TITLE),
                backup_label,
                title_color,
                15.0,
                600,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        // Description line — always shown.
        let backup_desc = settings_backup_description_rect(viewport, scroll, &backup_flags);
        if row_visible(backup_desc, body) {
            self.draw_text_with_style(
                bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::BACKUP_CARD_DESCRIPTION),
                backup_desc,
                label_color,
                12.0,
                400,
                1.0,
            )?;
        }
        // Tauri exposes one create action; list refresh remains automatic.
        let backup_actions = settings_backup_actions_row_rect(viewport, scroll, &backup_flags);
        if row_visible(backup_actions, body) {
            let create_btn = settings_backup_create_button_rect(backup_actions);
            let create_radius = BorderRadius::all(6.0);
            let create_accent = accent_on;
            self.fill_rounded_rect(create_btn, with_alpha(create_accent, 0.18), create_radius)?;
            self.stroke_rounded_rect(create_btn, controls.border, create_radius, 1.0)?;
            self.draw_settings_button_text(
                bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::BACKUP_CREATE_NOW),
                create_btn,
                create_accent,
                12.0,
                500,
            )?;
        }
        // Info/error line — only when a status is set. Success → green, error
        // → red (mirrors the widget-tree card's status colours).
        if let Some(status) = backup_status_snapshot.as_ref() {
            let backup_status_row = settings_backup_status_rect(viewport, scroll, &backup_flags);
            if row_visible(backup_status_row, body) {
                let is_error = matches!(status, crate::state::SettingsBackupStatus::Error(_));
                let status_color = if is_error {
                    with_alpha(palette.accent_red, 0.9)
                } else {
                    with_alpha(palette.accent_green, 0.9)
                };
                self.draw_settings_text(
                    bkp::backup_status_text(status),
                    backup_status_row,
                    status_color,
                )?;
            }
        }
        // Backup list — N entry rows (file·size + 恢复) or one backupEmpty
        // placeholder. Both branches anchor off the reserved status slot so the
        // list lines up whether or not a status line painted.
        if bkp::backup_list_is_empty(&backup_entries) {
            let empty_row = settings_backup_entry_row_rect(viewport, scroll, &backup_flags, 0);
            if row_visible(empty_row, body) {
                self.draw_text_no_wrap_with_style(
                    bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::BACKUP_EMPTY),
                    empty_row,
                    palette.text_muted,
                    12.0,
                    400,
                    1.0,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
            }
        } else {
            for (entry_index, entry) in backup_entries
                .iter()
                .take(SETTINGS_BACKUP_ROW_VISIBLE_MAX)
                .enumerate()
            {
                let entry_row =
                    settings_backup_entry_row_rect(viewport, scroll, &backup_flags, entry_index);
                if !row_visible(entry_row, body) {
                    continue;
                }
                let entry_radius = BorderRadius::all(6.0);
                self.fill_rounded_rect(entry_row, palette.neutral_overlay(0.04), entry_radius)?;
                let restore_btn = settings_backup_restore_button_rect(entry_row);
                let info_width = (restore_btn.x - entry_row.x - 20.0).max(0.0);
                let timestamp_rect = bentodesk_style::Rect {
                    x: entry_row.x + 12.0,
                    y: entry_row.y + 5.0,
                    width: info_width,
                    height: 16.0,
                };
                let timestamp = bkp::format_timestamp(entry.id.as_str());
                self.draw_text_no_wrap_with_style(
                    timestamp.as_str(),
                    timestamp_rect,
                    title_color,
                    12.0,
                    400,
                    1.0,
                    dwrite::TextAlign::DEFAULT,
                )?;
                let size_rect = bentodesk_style::Rect {
                    x: timestamp_rect.x,
                    y: timestamp_rect.bottom(),
                    width: info_width,
                    height: 14.0,
                };
                let size = bkp::format_size(entry.size_bytes);
                self.draw_text_no_wrap_with_style(
                    size.as_str(),
                    size_rect,
                    palette.text_muted,
                    11.0,
                    400,
                    1.0,
                    dwrite::TextAlign::DEFAULT,
                )?;
                let restore_radius = BorderRadius::all(6.0);
                self.fill_rounded_rect(restore_btn, controls.fill, restore_radius)?;
                self.stroke_rounded_rect(restore_btn, controls.border, restore_radius, 1.0)?;
                self.draw_settings_button_text(
                    bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::BACKUP_RESTORE),
                    restore_btn,
                    title_color,
                    12.0,
                    400,
                )?;
            }
        }
        Ok(backup_flags)
    }
}
