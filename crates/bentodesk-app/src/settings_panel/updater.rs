use super::*;

// ── M1f 2026-05-29 — Updater §8 card (`UpdaterCard.tsx`) ────────────────
//
// Sits AFTER Stealth in the Tauri body order
// (…→Performance→Startup→Stealth→Updater→Backup→Encryption→Plugins). Rows:
//   title                              (always) — 应用更新
//   status row  [label | pill]         (always)
//   version block [label : version]    (only Available/Ready/Installing/Skipped)
//   progress bar                       (only Downloading)
//   error line                         (only Error)
//   action buttons [Check][Dl/Install][Skip]  (Check always; others state-gated)
//   freq prefs row  [label | chip]     (always)
//   auto-download row [label | toggle] (always)
//
// The version/progress/error blocks are mutually exclusive by status family,
// captured as `UpdaterHeightKind`. Geometry takes that discriminant + the
// Startup/Stealth gating flags (so the title follows whatever Stealth rows are
// visible) — all passed in, never read from global state.

/// M1f — compact label/value/version row height (matches the Stealth row
/// rhythm).
pub const SETTINGS_UPDATER_ROW_H: f32 = 28.0;

/// M1f — status pill capsule size (reuses the Stealth pill footprint; the
/// "有可用更新"/"准备安装" labels are the widest so the pill is a touch wider).
pub const SETTINGS_UPDATER_PILL_W: f32 = 92.0;
pub const SETTINGS_UPDATER_PILL_H: f32 = 22.0;

/// M1f — progress-bar band height (the track sits vertically centred in it).
pub const SETTINGS_UPDATER_PROGRESS_H: f32 = 20.0;
/// M1f — progress-track thickness.
pub const SETTINGS_UPDATER_PROGRESS_TRACK_H: f32 = 6.0;

/// M1f — error line band height (single wrapped line).
pub const SETTINGS_UPDATER_ERROR_H: f32 = 32.0;

/// M1f — action buttons row height (shares the footer button height).
pub const SETTINGS_UPDATER_BTN_ROW_H: f32 = SETTINGS_FOOTER_BTN_H;

/// M1f — wider action button for the bilingual labels (检查更新 / 安装并重启
/// / Install and restart) which overflow the 84-DIP footer button width.
pub const SETTINGS_UPDATER_BTN_W: f32 = 104.0;
/// M1f — gap between adjacent action buttons.
pub const SETTINGS_UPDATER_BTN_GAP: f32 = 8.0;

/// M1f — frequency chip size (cycles Daily/Weekly/Manual). Mirrors the
/// language chip footprint so the prefs rows read as the same control band.
pub const SETTINGS_UPDATER_FREQ_CHIP_W: f32 = 96.0;
pub const SETTINGS_UPDATER_FREQ_CHIP_H: f32 = 28.0;

/// M1f — scroll-space bottom Y of the last laid-out Stealth element, the
/// anchor the Updater title hangs from. Mirrors the branch logic in
/// `settings_stealth_content_height` (buttons row always; OneDrive block only
/// when `has_retry`) so layout has a single source of truth.
fn settings_stealth_section_bottom(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
    has_retry: bool,
    has_error: bool,
) -> f32 {
    if has_retry {
        settings_stealth_onedrive_block_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
            has_retry,
            has_error,
        )
        .bottom()
    } else {
        settings_stealth_buttons_row_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
            has_retry,
            has_error,
        )
        .bottom()
    }
}

/// M1f — `应用更新 / App Updates` group title rect. Sits below the Stealth
/// section + a section gap. Takes all the Startup+Stealth gating flags so its
/// Y follows whatever rows are currently visible above it.
pub fn settings_updater_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
    stealth_has_retry: bool,
    stealth_has_error: bool,
) -> Rect {
    let body = settings_body_rect(viewport);
    let bottom = settings_stealth_section_bottom(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
        stealth_has_retry,
        stealth_has_error,
    );
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: bottom + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M1f — status row rect (label left + pill right). Row 0, always shown. Takes
/// the full flag set to chain off the dynamic title Y.
pub fn settings_updater_status_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let label = settings_updater_label_rect(
        viewport,
        scroll_offset_y,
        flags.crash_restart_enabled,
        flags.safe_start_after_hibernation,
        flags.stealth_has_retry,
        flags.stealth_has_error,
    );
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_UPDATER_ROW_H,
    }
}

/// M1f — right-anchored status-pill rect inside the status row.
pub fn settings_updater_pill_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_UPDATER_PILL_W,
        y: row.y + (row.height - SETTINGS_UPDATER_PILL_H) * 0.5,
        width: SETTINGS_UPDATER_PILL_W,
        height: SETTINGS_UPDATER_PILL_H,
    }
}

/// M1f — the conditional middle block (version / progress / error) rect. Its
/// height depends on `flags.updater_kind`; `StatusOnly` yields a zero-height
/// rect anchored at the status-row bottom (so the buttons row chains cleanly
/// with no gap). Sits directly below the status row.
pub fn settings_updater_middle_block_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let status = settings_updater_status_row_rect(viewport, scroll_offset_y, flags);
    let height = match flags.updater_kind {
        UpdaterHeightKind::StatusOnly => 0.0,
        UpdaterHeightKind::Versioned => SETTINGS_UPDATER_ROW_H,
        UpdaterHeightKind::Downloading => SETTINGS_UPDATER_PROGRESS_H,
        UpdaterHeightKind::Error => SETTINGS_UPDATER_ERROR_H,
    };
    Rect {
        x: status.x,
        y: status.bottom(),
        width: status.width,
        height,
    }
}

/// M1f — progress-track rect inside the middle block (only meaningful when
/// `flags.updater_kind == Downloading`; the renderer paints the filled portion
/// itself from the fraction). Vertically centred, full row width.
pub fn settings_updater_progress_track_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let block = settings_updater_middle_block_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: block.x,
        y: block.y + (block.height - SETTINGS_UPDATER_PROGRESS_TRACK_H) * 0.5,
        width: block.width,
        height: SETTINGS_UPDATER_PROGRESS_TRACK_H,
    }
}

/// M1f — action buttons row rect (`检查更新`, `下载/安装并重启`, `跳过此版本`).
/// Always shown (检查更新 is always visible); sits below the middle block.
pub fn settings_updater_buttons_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let block = settings_updater_middle_block_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: block.x,
        y: block.bottom() + 6.0,
        width: block.width,
        height: SETTINGS_UPDATER_BTN_ROW_H,
    }
}

/// M1f — action button rect for column `index` (0-based, left to right) inside
/// the buttons row. Buttons left-pack; visibility is decided by the caller
/// (`updater_show_*`), so callers must assign a stable column index to the
/// buttons they actually paint. The hit-tester reuses the same index→rect
/// mapping so paint and hit agree.
pub fn settings_updater_button_rect(row: Rect, index: u8) -> Rect {
    let x = row.x + (SETTINGS_UPDATER_BTN_W + SETTINGS_UPDATER_BTN_GAP) * index as f32;
    Rect {
        x,
        y: row.y + (row.height - SETTINGS_FOOTER_BTN_H) * 0.5,
        width: SETTINGS_UPDATER_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1f — `检查频率 / Check frequency` prefs row rect (label + cycling chip).
/// Always shown; sits below the action buttons row.
pub fn settings_updater_frequency_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let buttons = settings_updater_buttons_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: buttons.x,
        y: buttons.bottom() + 8.0,
        width: buttons.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1f — right-anchored frequency chip rect inside the frequency row.
pub fn settings_updater_frequency_chip_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_UPDATER_FREQ_CHIP_W,
        y: row.y + (row.height - SETTINGS_UPDATER_FREQ_CHIP_H) * 0.5,
        width: SETTINGS_UPDATER_FREQ_CHIP_W,
        height: SETTINGS_UPDATER_FREQ_CHIP_H,
    }
}

/// M1f — `后台静默下载 / Silent background download` toggle row rect. Always
/// shown; sits directly below the frequency row.
pub fn settings_updater_auto_download_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let freq = settings_updater_frequency_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: freq.x,
        y: freq.bottom(),
        width: freq.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1f — right-anchored toggle hit-box inside the auto-download row (mirrors
/// `SETTINGS_TOP_TOGGLE_HIT_*` so click ergonomics match the General toggles).
pub fn settings_updater_auto_download_hit_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_TOP_TOGGLE_HIT_W,
        y: row.y + (row.height - SETTINGS_TOP_TOGGLE_HIT_H) * 0.5,
        width: SETTINGS_TOP_TOGGLE_HIT_W,
        height: SETTINGS_TOP_TOGGLE_HIT_H,
    }
}

/// M1f — height the Updater §8 card contributes to
/// `settings_body_content_height`. Conditional middle block makes it dynamic,
/// so the status family drives it (pure — no global reads). Always-present
/// rows: title + status + buttons + 2 prefs rows. The middle block adds its
/// kind-specific height. A trailing section gap keeps the body bottom padded.
pub fn settings_updater_content_height(kind: UpdaterHeightKind) -> f32 {
    let middle = match kind {
        UpdaterHeightKind::StatusOnly => 0.0,
        UpdaterHeightKind::Versioned => SETTINGS_UPDATER_ROW_H,
        UpdaterHeightKind::Downloading => SETTINGS_UPDATER_PROGRESS_H,
        UpdaterHeightKind::Error => SETTINGS_UPDATER_ERROR_H,
    };
    SETTINGS_SECTION_LABEL_H
        + SETTINGS_UPDATER_ROW_H
        + middle
        + 6.0
        + SETTINGS_UPDATER_BTN_ROW_H
        + 8.0
        + SETTINGS_ROW_H_M1 * 2.0
        + SETTINGS_SECTION_GAP
}
