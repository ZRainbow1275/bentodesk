use super::*;

// ── M1e 2026-05-29 — Stealth §7 card (`StealthModeCard.tsx`) ────────────
//
// Sits AFTER Startup in the Tauri body order
// (General→Paths→Appearance→Zone→Performance→Startup→Stealth→…). Rows:
//   title                            (always) — 桌面隐形模式
//   status row  [label | pill]       (always)
//   schema-version row [label|value] (always)
//   mirror-health row  [label|value] (always)
//   retry-count row    [label|value] (only when retry_count > 0)
//   last-error block   [label]/[err] (only when last_error.is_some())
//   buttons row  [Refresh][Reapply]  (always)
//   OneDrive warning block           (only when retry_count > 0)
//
// The two conditional flags (`has_retry`, `has_error`) flow as parameters so
// geometry stays pure — the shell passes the live `stealth::status()`
// snapshot. The retry row and OneDrive block are both gated on `has_retry`
// (the backend notes OneDrive typically holds the lock when retries pend).

/// M1e — compact `.settings-row` height for the Stealth label/value rows
/// (shorter than the 44-DIP toggle rows; matches Tauri's `.settings-row`
/// status-line rhythm).
pub const SETTINGS_STEALTH_ROW_H: f32 = 28.0;

/// M1e — Stealth status pill capsule size (reuses the source-card pill tone;
/// generalized colour bucket is `StatusLevel::derive`).
pub const SETTINGS_STEALTH_PILL_W: f32 = 76.0;
pub const SETTINGS_STEALTH_PILL_H: f32 = 22.0;

/// M1e — last-error block: a label line + a wrapped error-code line.
pub const SETTINGS_STEALTH_ERROR_BLOCK_H: f32 = 46.0;

/// M1e — buttons-row height (Refresh + Reapply share the footer button size).
pub const SETTINGS_STEALTH_BTN_ROW_H: f32 = SETTINGS_FOOTER_BTN_H;

/// M1e — OneDrive warning block height (multi-line informational text).
pub const SETTINGS_STEALTH_ONEDRIVE_H: f32 = 52.0;

/// M1e — scroll-space bottom Y of the last laid-out Startup element, the
/// anchor the Stealth title hangs from. Mirrors the branch logic in
/// `settings_perf_startup_content_height` so layout has a single source of
/// truth.
fn settings_startup_section_bottom(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> f32 {
    if safe_start_after_hibernation {
        settings_hibernate_slider_row_rect(viewport, scroll_offset_y, crash_restart_enabled)
            .bottom()
    } else {
        settings_safe_start_row_rect(viewport, scroll_offset_y, crash_restart_enabled).bottom()
            + SETTINGS_DESC_H
    }
}

/// M1e — `桌面隐形模式 / Desktop Stealth Mode` group title rect. Sits below
/// the Startup section + a section gap. Takes the Startup gating bools so its
/// Y follows whatever Startup rows are currently visible.
pub fn settings_stealth_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> Rect {
    let body = settings_body_rect(viewport);
    let bottom = settings_startup_section_bottom(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
    );
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: bottom + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M1e — status row rect (label left + pill right). Row 0, always shown.
pub fn settings_stealth_status_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> Rect {
    let label = settings_stealth_label_rect(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
    );
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_STEALTH_ROW_H,
    }
}

/// M1e — right-anchored status-pill rect inside the status row.
pub fn settings_stealth_pill_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_STEALTH_PILL_W,
        y: row.y + (row.height - SETTINGS_STEALTH_PILL_H) * 0.5,
        width: SETTINGS_STEALTH_PILL_W,
        height: SETTINGS_STEALTH_PILL_H,
    }
}

/// M1e — schema-version row rect (label + value). Row 1, always shown.
pub fn settings_stealth_schema_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> Rect {
    let prev = settings_stealth_status_row_rect(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
    );
    Rect {
        x: prev.x,
        y: prev.bottom(),
        width: prev.width,
        height: SETTINGS_STEALTH_ROW_H,
    }
}

/// M1e — mirror-health row rect (label + value). Row 2, always shown.
pub fn settings_stealth_mirror_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> Rect {
    let prev = settings_stealth_schema_row_rect(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
    );
    Rect {
        x: prev.x,
        y: prev.bottom(),
        width: prev.width,
        height: SETTINGS_STEALTH_ROW_H,
    }
}

/// M1e — retry-count row rect (label + value). Row 3, ONLY when
/// `retry_count > 0`. Sits directly below the mirror-health row.
pub fn settings_stealth_retry_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> Rect {
    let prev = settings_stealth_mirror_row_rect(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
    );
    Rect {
        x: prev.x,
        y: prev.bottom(),
        width: prev.width,
        height: SETTINGS_STEALTH_ROW_H,
    }
}

/// M1e — last-error block rect (label line + wrapped code line). Row 4, ONLY
/// when `last_error.is_some()`. Its Y depends on whether the retry row is
/// present, so the `has_retry` flag chains through.
pub fn settings_stealth_error_block_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
    has_retry: bool,
) -> Rect {
    let anchor = if has_retry {
        settings_stealth_retry_row_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
        )
    } else {
        settings_stealth_mirror_row_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
        )
    };
    Rect {
        x: anchor.x,
        y: anchor.bottom(),
        width: anchor.width,
        height: SETTINGS_STEALTH_ERROR_BLOCK_H,
    }
}

/// M1e — buttons row rect ([Refresh][Reapply]). Always shown; its Y depends
/// on the two conditional rows above (`has_retry`, `has_error`).
pub fn settings_stealth_buttons_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
    has_retry: bool,
    has_error: bool,
) -> Rect {
    let bottom = if has_error {
        settings_stealth_error_block_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
            has_retry,
        )
        .bottom()
    } else if has_retry {
        settings_stealth_retry_row_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
        )
        .bottom()
    } else {
        settings_stealth_mirror_row_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
        )
        .bottom()
    };
    let body = settings_body_rect(viewport);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: bottom + 6.0,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_STEALTH_BTN_ROW_H,
    }
}

/// M1e — Refresh button rect (left), inside the Stealth buttons row.
pub fn settings_stealth_refresh_button_rect(row: Rect) -> Rect {
    Rect {
        x: row.x,
        y: row.y + (row.height - SETTINGS_FOOTER_BTN_H) * 0.5,
        width: SETTINGS_FOOTER_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1e — Reapply button rect (right of Refresh), inside the buttons row.
pub fn settings_stealth_reapply_button_rect(row: Rect) -> Rect {
    let refresh = settings_stealth_refresh_button_rect(row);
    Rect {
        x: refresh.right() + SETTINGS_FOOTER_BTN_GAP,
        y: refresh.y,
        width: SETTINGS_FOOTER_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1e — OneDrive warning block rect. ONLY when `retry_count > 0`. Sits below
/// the buttons row. Informational text only (no button — there is no
/// OneDrive-exclusion probe / guide URL in the nano backend, so per §17 this
/// stays text-only rather than painting a dead button).
pub fn settings_stealth_onedrive_block_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
    has_retry: bool,
    has_error: bool,
) -> Rect {
    let buttons = settings_stealth_buttons_row_rect(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
        has_retry,
        has_error,
    );
    Rect {
        x: buttons.x,
        y: buttons.bottom() + 8.0,
        width: buttons.width,
        height: SETTINGS_STEALTH_ONEDRIVE_H,
    }
}

/// M1e — height the Stealth §7 card contributes to
/// `settings_body_content_height`. Conditional rows make it dynamic, so the
/// two flags are parameters (geometry never reads global state). The base
/// rows (title + status + schema + mirror) are always present; retry adds one
/// row, error adds the error block, and retry additionally adds the OneDrive
/// block. A trailing section gap keeps the body bottom padded.
pub fn settings_stealth_content_height(has_retry: bool, has_error: bool) -> f32 {
    let mut h = SETTINGS_SECTION_LABEL_H + SETTINGS_STEALTH_ROW_H * 3.0;
    if has_retry {
        h += SETTINGS_STEALTH_ROW_H;
    }
    if has_error {
        h += SETTINGS_STEALTH_ERROR_BLOCK_H;
    }
    h += 6.0 + SETTINGS_STEALTH_BTN_ROW_H;
    if has_retry {
        h += 8.0 + SETTINGS_STEALTH_ONEDRIVE_H;
    }
    h + SETTINGS_SECTION_GAP
}
