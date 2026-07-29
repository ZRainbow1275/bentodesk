use super::*;

// ── M1g 2026-05-29 — Backup §9 card (`BackupCard.tsx`) ──────────────────
//
// Sits AFTER Updater in the Tauri body order
// (…→Stealth→Updater→Backup→Encryption→Plugins). Rows:
//   title                              (always) — 设置备份
//   description line                   (always)
//   立即备份 button + Refresh button     (always)
//   info/error line                    (only when settings_backup_status set)
//   backup-list:
//     N entry rows [file·size | 恢复]  (one per visible entry, capped)
//     OR a single backupEmpty row       (when the list is empty)
//
// The list is variable-length: its height grows one `SETTINGS_BACKUP_ROW_H`
// per visible row (capped at `SETTINGS_BACKUP_ROW_VISIBLE_MAX`), or a single
// placeholder row when empty. The capped row count + the status-present flag
// flow through `SettingsBodyFlags::backup_row_count` (built from
// `backup_card::backup_visible_row_count`) so the dynamic height + scroll
// clamp match what's painted. Geometry stays PURE — the count is passed in,
// nothing reads global state. Encryption §10 + Plugins §11 follow in a later
// chunk; this card leaves a trailing section gap for them.

/// M1g — max backup rows the list paints / hit-tests. Reuses the plugins
/// visible-cap rhythm (`SETTINGS_PLUGINS_ROW_VISIBLE_MAX`) so the compact
/// overlay never runs the list off the body. Matches the (now superseded) K1
/// `SETTINGS_BACKUP_ENTRY_VISIBLE_MAX` so the cap is unchanged from the K1
/// shell the runtime replaced.
pub const SETTINGS_BACKUP_ROW_VISIBLE_MAX: usize = 3;

/// M1g — compact backup label/value/description row height (matches the
/// Stealth/Updater 28-DIP status-line rhythm).
pub const SETTINGS_BACKUP_ROW_H: f32 = 28.0;

/// Backup-list entry row height. Tauri paints timestamp and size on separate
/// lines; 40 DIP also keeps the 32-DIP restore button inside the row.
pub const SETTINGS_BACKUP_ENTRY_ROW_H: f32 = 40.0;

/// M1g — gap between adjacent backup-list entry rows.
pub const SETTINGS_BACKUP_ENTRY_ROW_GAP: f32 = 6.0;

/// M1g — `立即备份 / Create now` button width (wider than a stepper so the
/// bilingual label fits) and the smaller per-row 恢复 / Refresh button width.
pub const SETTINGS_BACKUP_CREATE_BTN_W: f32 = 88.0;
pub const SETTINGS_BACKUP_REFRESH_BTN_W: f32 = 84.0;
pub const SETTINGS_BACKUP_RESTORE_BTN_W: f32 = 64.0;
pub const SETTINGS_BACKUP_BTN_GAP_M1: f32 = 8.0;

/// Tauri `.backup-card { gap: 10px }` between the actions/status/list blocks.
pub const SETTINGS_BACKUP_CONTENT_GAP: f32 = 10.0;

/// M1g — `设置备份 / Settings Backup` group title rect. Sits below the Updater
/// section + a section gap. Takes the full flag set so its Y follows whatever
/// Updater rows (status/version/progress/error + prefs) are currently visible.
pub fn settings_backup_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let body = settings_body_rect(viewport);
    // The Updater section's last laid-out element is always the auto-download
    // prefs row (always shown), so anchor off its bottom + a section gap.
    let updater_bottom =
        settings_updater_auto_download_row_rect(viewport, scroll_offset_y, flags).bottom();
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: updater_bottom + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M1g — description line rect (一段说明文字). Row 0, always shown, below the
/// title.
pub fn settings_backup_description_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let label = settings_backup_label_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_BACKUP_ROW_H,
    }
}

/// M1g — actions row rect (`立即备份`, `刷新`). Always shown; below the
/// description line.
pub fn settings_backup_actions_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let desc = settings_backup_description_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: desc.x,
        y: desc.bottom(),
        width: desc.width,
        height: SETTINGS_BACKUP_BTN_ROW_H,
    }
}

/// M1g — actions row height (shares the footer button height, like the other
/// card button rows).
pub const SETTINGS_BACKUP_BTN_ROW_H: f32 = SETTINGS_FOOTER_BTN_H;

/// M1g — `立即备份 / Create now` button rect (left), inside the actions row.
pub fn settings_backup_create_button_rect(row: Rect) -> Rect {
    Rect {
        x: row.x,
        y: row.y + (row.height - SETTINGS_FOOTER_BTN_H) * 0.5,
        width: SETTINGS_BACKUP_CREATE_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1g — `刷新 / Refresh` button rect (right of 立即备份), inside the actions
/// row. Re-lists the backup files (`ListSettingsBackups`).
pub fn settings_backup_refresh_button_rect(row: Rect) -> Rect {
    let create = settings_backup_create_button_rect(row);
    Rect {
        x: create.right() + SETTINGS_BACKUP_BTN_GAP_M1,
        y: create.y,
        width: SETTINGS_BACKUP_REFRESH_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1g — info/error line rect. Only painted when `settings_backup_status` is
/// set; below the actions row. Its presence does NOT change the list Y (the
/// list anchors off this rect's reserved slot regardless) so the geometry
/// stays a single linear chain — when no status is set the renderer simply
/// skips painting here and the list still lines up because both branches
/// anchor off `actions_row.bottom()`.
pub fn settings_backup_status_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let actions = settings_backup_actions_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: actions.x,
        y: actions.bottom() + SETTINGS_BACKUP_CONTENT_GAP,
        width: actions.width,
        height: SETTINGS_BACKUP_ROW_H,
    }
}

/// M1g — backup-list entry row rect for visible `entry_index` (0-based,
/// newest-first). Sits below the status line. When the list is empty the
/// renderer paints a single `backupEmpty` placeholder at `entry_index = 0`.
pub fn settings_backup_entry_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
    entry_index: usize,
) -> Rect {
    let actions = settings_backup_actions_row_rect(viewport, scroll_offset_y, flags);
    let list_y = if flags.backup_status_present {
        settings_backup_status_rect(viewport, scroll_offset_y, flags).bottom()
            + SETTINGS_BACKUP_CONTENT_GAP
    } else {
        actions.bottom() + SETTINGS_BACKUP_CONTENT_GAP
    };
    Rect {
        x: actions.x,
        y: list_y
            + (SETTINGS_BACKUP_ENTRY_ROW_H + SETTINGS_BACKUP_ENTRY_ROW_GAP) * entry_index as f32,
        width: actions.width,
        height: SETTINGS_BACKUP_ENTRY_ROW_H,
    }
}

/// M1g — right-anchored `恢复 / Restore` button rect inside an entry row.
pub fn settings_backup_restore_button_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - 12.0 - SETTINGS_BACKUP_RESTORE_BTN_W,
        y: row.y + (row.height - SETTINGS_FOOTER_BTN_H) * 0.5,
        width: SETTINGS_BACKUP_RESTORE_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1g — height the Backup §9 card contributes to
/// `settings_body_content_height`. The variable-length list makes it dynamic,
/// so the (already-capped) `backup_row_count` is the parameter (geometry never
/// reads global state). Always-present: title + description + actions +
/// reserved status line. The list adds either `n` entry rows (+ inter-row
/// gaps) or a single empty-placeholder row. A trailing section gap pads the
/// body bottom (and reserves room for the §10/§11 chunk to come).
pub fn settings_backup_content_height(backup_row_count: usize) -> f32 {
    settings_backup_content_height_for_status(backup_row_count, false)
}

pub(super) fn settings_backup_content_height_for_status(
    backup_row_count: usize,
    status_present: bool,
) -> f32 {
    let base = SETTINGS_SECTION_LABEL_H
        + SETTINGS_BACKUP_ROW_H            // description
        + SETTINGS_BACKUP_BTN_ROW_H        // actions
        + SETTINGS_BACKUP_CONTENT_GAP
        + if status_present {
            SETTINGS_BACKUP_ROW_H + SETTINGS_BACKUP_CONTENT_GAP
        } else {
            0.0
        };
    let rows = backup_row_count.min(SETTINGS_BACKUP_ROW_VISIBLE_MAX);
    let list = if rows == 0 {
        // Empty placeholder occupies one entry-row slot.
        SETTINGS_BACKUP_ENTRY_ROW_H
    } else {
        SETTINGS_BACKUP_ENTRY_ROW_H * rows as f32
            + SETTINGS_BACKUP_ENTRY_ROW_GAP * (rows as f32 - 1.0)
    };
    base + list + SETTINGS_SECTION_GAP
}
