use super::*;

// ── M7 2026-06-01 — Encryption §10 inline card (`EncryptionCard.tsx`) ────────
//
// Sits BETWEEN Backup §9 and Plugins §11 in the Tauri body order
// (…→Updater→Backup→**Encryption**→Plugins→footer), matching the Tauri
// `<BackupCard/><EncryptionCard/>` adjacency (`SettingsPanel.tsx:705-706`). The
// card is FIXED-HEIGHT (no variable rows), so unlike Backup/Plugins it adds a
// single constant additive term to `settings_body_content_height` and needs NO
// `SettingsBodyFlags` field. It anchors off the Backup card's last laid-out row
// (the backup list's last visible entry, or the empty placeholder) + a section
// gap; Plugins §11 then re-anchors off this card's status row so the offset
// chain reflows automatically. Layout (top-to-bottom, vertical column):
//   section label                                (always) — 设置加密
//   description line                             (always) — OneDrive sentence
//   current-mode row                             (always) — 当前模式: <mode>
//   3-button mode grid (None / DPAPI / Passphrase) (always)
//   passphrase row (label + masked input box)    (always)
//   hint line                                    (always) — never-stored
//   status banner                                (reserved; painted iff set)
// Both the renderer (paint) and `ui::settings_hit` (hit) call the identical
// helpers below so paint geometry == hit geometry (the project-wide SSoT rule).
// Geometry stays PURE — every helper is a function of (viewport, scroll, flags),
// returning `Copy` `Rect`s; no `AppState` reads (§10).

/// M7 — gap between mode cards in the responsive grid.
pub const SETTINGS_ENCRYPTION_BTN_GAP: f32 = 8.0;
/// Height of one encryption mode card (title + sub-label).
pub const SETTINGS_ENCRYPTION_BTN_H: f32 = 52.0;
/// At the 440-DIP Settings content width Tauri's
/// `repeat(auto-fit, minmax(160px, 1fr))` resolves to two columns, so the third
/// card wraps to a second row instead of being clipped into a three-column row.
pub const SETTINGS_ENCRYPTION_BTN_ROW_H: f32 =
    SETTINGS_ENCRYPTION_BTN_H * 2.0 + SETTINGS_ENCRYPTION_BTN_GAP;
/// M7 — encryption passphrase input row height (single-line masked box; shares
/// the §2 path-input rhythm).
pub const SETTINGS_ENCRYPTION_INPUT_ROW_H: f32 = 40.0;
/// M7 — encryption current-mode / hint / status compact row heights (match the
/// other card status-line rhythm).
pub const SETTINGS_ENCRYPTION_ROW_H: f32 = 28.0;
/// P13 (#7 fix wave 2026-06-01) — vertical gap between EVERY sibling row of the
/// §10 card (description / current / grid / passphrase-row / hint / status).
/// Tauri `.encryption-card { gap: 10px }` (`EncryptionCard.css:4`). The
/// mode-grid's INTERNAL button gap stays [`SETTINGS_ENCRYPTION_BTN_GAP`] (8px,
/// CSS:23) — this 10px is the inter-row rhythm only.
pub const SETTINGS_ENCRYPTION_ROW_GAP: f32 = 10.0;
/// P4 (#7 fix wave 2026-06-01) — width of the passphrase ROW's left label cell
/// (口令 / Passphrase). Tauri lays the row out `justify-content: space-between`
/// with the label on the left + the input filling the rest; this fixed cell is
/// the native-panel equivalent of the auto-sized `<span>`.
pub const SETTINGS_ENCRYPTION_PASS_LABEL_W: f32 = 64.0;
/// P4 — gap between the passphrase row's label cell and the input box.
pub const SETTINGS_ENCRYPTION_PASS_LABEL_GAP: f32 = 10.0;
/// M7 — number of mode buttons (None / DPAPI / Passphrase).
pub const SETTINGS_ENCRYPTION_MODE_COUNT: u8 = 3;

/// M7 — §10 Encryption card group title rect (设置加密 / Settings Encryption).
/// Anchors off the Backup card's last laid-out row (the backup list's last
/// visible entry, or the single placeholder at index 0) + a section gap — the
/// same anchor Plugins §11 used before this card landed. Takes the full flag
/// set so its Y follows whatever Backup/Updater/Stealth/Startup rows are
/// currently visible.
pub fn settings_encryption_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let body = settings_body_rect(viewport);
    let last_backup_index = flags
        .backup_row_count
        .min(SETTINGS_BACKUP_ROW_VISIBLE_MAX)
        .saturating_sub(1);
    let backup_bottom =
        settings_backup_entry_row_rect(viewport, scroll_offset_y, flags, last_backup_index)
            .bottom();
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: backup_bottom + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M7 — description line rect (the OneDrive sentence). Below the title.
/// P13 — separated from the title by the 10px inter-row gap.
pub fn settings_encryption_desc_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let label = settings_encryption_label_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: label.x,
        y: label.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: label.width,
        height: SETTINGS_ENCRYPTION_ROW_H,
    }
}

/// M7 — current-mode row rect (`当前模式: mode label`). Below the description.
/// P13 — separated by the 10px inter-row gap.
pub fn settings_encryption_current_mode_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let desc = settings_encryption_desc_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: desc.x,
        y: desc.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: desc.width,
        height: SETTINGS_ENCRYPTION_ROW_H,
    }
}

/// M7 — the 3-button mode-grid row rect (the band holding all three buttons).
/// Below the current-mode row. Use [`settings_encryption_mode_button_rect`] for
/// individual buttons inside this two-row band.
pub fn settings_encryption_mode_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let current = settings_encryption_current_mode_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: current.x,
        // P13 — separated from the current-mode row by the 10px inter-row gap.
        y: current.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: current.width,
        height: SETTINGS_ENCRYPTION_BTN_ROW_H,
    }
}

/// M7 — individual mode-button rect inside the grid for `index`
/// (0 = None, 1 = DPAPI, 2 = Passphrase). At the native panel's fixed content
/// width this mirrors Tauri's auto-fit grid as two cards on the first row and
/// the passphrase card on the second. PURE — no global state.
pub fn settings_encryption_mode_button_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
    index: u8,
) -> Rect {
    let row = settings_encryption_mode_row_rect(viewport, scroll_offset_y, flags);
    let i = index.min(SETTINGS_ENCRYPTION_MODE_COUNT - 1);
    let column = (i % 2) as f32;
    let grid_row = (i / 2) as f32;
    let btn_w = ((row.width - SETTINGS_ENCRYPTION_BTN_GAP) / 2.0).max(0.0);
    Rect {
        x: row.x + (btn_w + SETTINGS_ENCRYPTION_BTN_GAP) * column,
        y: row.y + (SETTINGS_ENCRYPTION_BTN_H + SETTINGS_ENCRYPTION_BTN_GAP) * grid_row,
        width: btn_w,
        height: SETTINGS_ENCRYPTION_BTN_H,
    }
}

/// P4 (#7 fix wave 2026-06-01) — the full passphrase ROW band (label cell +
/// input box), below the mode-button grid. Tauri `.encryption-passphrase-row`
/// is a `justify-content: space-between` flex row: a `<span>` label on the left
/// and the `<input>` filling the rest. The label/input sub-rects derive from
/// this band. P13 — separated from the grid by the 10px inter-row gap (was the
/// 8px button gap).
pub fn settings_encryption_passphrase_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let row = settings_encryption_mode_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: row.x,
        y: row.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: row.width,
        height: SETTINGS_ENCRYPTION_INPUT_ROW_H,
    }
}

/// P4 — passphrase ROW left label cell (口令 / Passphrase). The fixed-width
/// left cell of the space-between row; non-interactive.
pub fn settings_encryption_passphrase_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let row = settings_encryption_passphrase_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: row.x,
        y: row.y,
        width: SETTINGS_ENCRYPTION_PASS_LABEL_W.min(row.width),
        height: row.height,
    }
}

/// M7 — masked passphrase input box rect. P4 — now ONLY the input sub-rect on
/// the RIGHT of the passphrase row (the left label cell + a gap are reserved by
/// [`settings_encryption_passphrase_label_rect`]); the hit-test for
/// `FocusPassphraseField` targets this sub-rect only.
pub fn settings_encryption_passphrase_input_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let row = settings_encryption_passphrase_row_rect(viewport, scroll_offset_y, flags);
    let label_w = SETTINGS_ENCRYPTION_PASS_LABEL_W.min(row.width);
    let x = row.x + label_w + SETTINGS_ENCRYPTION_PASS_LABEL_GAP;
    Rect {
        x,
        y: row.y,
        width: (row.right() - x).max(0.0),
        height: row.height,
    }
}

/// M7 — hint line rect (the "never stored in plaintext" sentence). Below the
/// passphrase ROW. P13 — separated by the 10px inter-row gap. Spans the full
/// card width (not just the input sub-rect) like the Tauri `.encryption-hint`.
pub fn settings_encryption_hint_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let row = settings_encryption_passphrase_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: row.x,
        y: row.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: row.width,
        height: SETTINGS_ENCRYPTION_ROW_H,
    }
}

/// M7 — status banner rect (error/success). Reserved slot below the hint;
/// painted only when `settings_encryption_status` is `Some`. The presence of a
/// status does NOT change the next section's anchor (Plugins anchors off this
/// rect's reserved slot regardless), keeping the offset chain linear — same
/// pattern as the Backup card's reserved status row.
pub fn settings_encryption_status_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let hint = settings_encryption_hint_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: hint.x,
        // P13 — separated from the hint by the 10px inter-row gap.
        y: hint.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: hint.width,
        height: SETTINGS_ENCRYPTION_ROW_H,
    }
}

/// M7 — fixed height the §10 Encryption card contributes to
/// `settings_body_content_height`. No variable rows (unlike Backup/Plugins), so
/// this is a constant: label + desc + current-mode + button-row + passphrase
/// input + hint + reserved status + a trailing section gap.
pub fn settings_encryption_content_height() -> f32 {
    settings_encryption_content_height_for_status(false)
}

pub(super) fn settings_encryption_content_height_for_status(status_present: bool) -> f32 {
    SETTINGS_SECTION_LABEL_H
        + SETTINGS_ENCRYPTION_ROW_H            // description
        + SETTINGS_ENCRYPTION_ROW_H            // current-mode row
        + SETTINGS_ENCRYPTION_BTN_ROW_H        // mode-button grid
        + SETTINGS_ENCRYPTION_INPUT_ROW_H      // passphrase row (label + input)
        + SETTINGS_ENCRYPTION_ROW_H            // hint line
        // Five always-present gaps; a visible status adds its own row + gap.
        + SETTINGS_ENCRYPTION_ROW_GAP * 5.0
        + if status_present {
            SETTINGS_ENCRYPTION_ROW_H + SETTINGS_ENCRYPTION_ROW_GAP
        } else {
            0.0
        }
        + SETTINGS_SECTION_GAP
}
