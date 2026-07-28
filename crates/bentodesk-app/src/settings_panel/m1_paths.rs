use super::*;

/// Round-2 M2 — Y offset (scroll-space) at which the M2 §2 Paths sections
/// start. G3 parity (2026-06-01): the §4 zone-display-mode picker was promoted
/// out of the General band into its own group between §3 Appearance and §5
/// Performance (Tauri body order General → **Paths** → Appearance → DisplayMode
/// → Performance). So Paths §2 now anchors directly below the M1 toggle band +
/// the language row (the General band's last element) + a section gap — the
/// `+ 2.0` picker-row wedge is gone (now `+ 1.0`: 5 toggles + 1 language row).
pub(super) fn settings_m2_origin_y_offset() -> f32 {
    SETTINGS_ROW_H_M1 * (SETTINGS_TOP_TOGGLE_COUNT as f32 + 1.0) + SETTINGS_SECTION_GAP
}

/// Tauri §2 `Paths` group title. It owns the section gap after General; the
/// nested `桌面源` row label begins exactly at this title band's bottom.
pub fn settings_paths_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + settings_m2_origin_y_offset(),
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// Round-2 M2 — `桌面源` section label rect (the dim caption above the two
/// source cards).
pub fn settings_sources_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let paths = settings_paths_label_rect(viewport, scroll_offset_y);
    Rect {
        x: paths.x,
        y: paths.bottom(),
        width: paths.width,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// Round-2 M2 — full row rect for source card `index` (0=primary, 1=public).
pub fn settings_source_row_rect(viewport: Size, scroll_offset_y: f32, index: u8) -> Rect {
    let label = settings_sources_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.x,
        y: label.bottom() + (SETTINGS_SOURCE_ROW_H + SETTINGS_SOURCE_GAP) * index as f32,
        width: label.width,
        height: SETTINGS_SOURCE_ROW_H,
    }
}

/// M1i fidelity (2026-05-29) — refresh (`↻`) button. In Tauri it is the LAST
/// child of `.desktop-source-list` (`align-self: flex-end`), sitting directly
/// BELOW the cards / empty placeholder at the list's bottom-right — NOT on the
/// section heading row (`SettingsPanel.tsx:354-360`). Click re-runs
/// `all_desktop_dirs` and repopulates `AppState::desktop_sources`
/// (`RefreshDesktopSources`). Its Y follows the live card stack, so the hit
/// rect (`ui::settings_hit`) must pass the same live `source_row_count`.
pub fn settings_sources_refresh_button_rect(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> Rect {
    let last_bottom = settings_sources_cards_bottom(viewport, scroll_offset_y, source_row_count);
    let label = settings_sources_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.right() - SETTINGS_SOURCE_REFRESH_BTN_W,
        y: last_bottom + SETTINGS_SOURCE_REFRESH_GAP,
        width: SETTINGS_SOURCE_REFRESH_BTN_W,
        height: SETTINGS_SOURCE_REFRESH_BTN_H,
    }
}

/// M1i fidelity — scroll-space bottom Y of the live source-card stack (or the
/// empty placeholder when the list is empty). The refresh button hangs below
/// this, and the 桌面路径 section anchors off the refresh button's bottom.
/// `source_row_count` is the LIVE count (clamped to the cap).
pub fn settings_sources_cards_bottom(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> f32 {
    let label = settings_sources_label_rect(viewport, scroll_offset_y);
    let live = source_row_count.min(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    if live == 0 {
        // Empty `.desktop-source-empty` placeholder occupies one short line.
        label.bottom() + SETTINGS_SOURCE_EMPTY_H
    } else {
        let last = settings_source_row_rect(viewport, scroll_offset_y, (live - 1) as u8);
        last.bottom()
    }
}

/// Round-2 M2 / M1i fidelity — `桌面路径` section label rect. Anchors off the
/// refresh button bottom (the list's last child), which itself follows the
/// LIVE `source_row_count` — so this section reflows up/down with the live
/// source count (Tauri's flex column).
pub fn settings_desktop_path_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> Rect {
    let refresh = settings_sources_refresh_button_rect(viewport, scroll_offset_y, source_row_count);
    let body = settings_body_rect(viewport);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: refresh.bottom() + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// Round-2 M2 — `桌面路径` input rect (single-line dark rounded box).
pub fn settings_desktop_path_input_rect(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> Rect {
    let label = settings_desktop_path_label_rect(viewport, scroll_offset_y, source_row_count);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_INPUT_ROW_H,
    }
}

/// Round-2 M2 — `监控值` section label rect.
pub fn settings_watch_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> Rect {
    let input = settings_desktop_path_input_rect(viewport, scroll_offset_y, source_row_count);
    let body = settings_body_rect(viewport);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: input.bottom() + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// Round-2 M2 — `监控值` textarea rect (multi-line dark rounded box).
pub fn settings_watch_textarea_rect(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> Rect {
    let label = settings_watch_label_rect(viewport, scroll_offset_y, source_row_count);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_TEXTAREA_H,
    }
}

/// M1i fidelity (2026-05-29) — the live height of the inner card+refresh stack
/// of the 桌面源 §2 list, EXCLUDING the heading label and the trailing section
/// gap. Drives the reflow (Tauri's flex column): `live` cards plus the
/// `align-self: flex-end` refresh button below them, or the single empty
/// placeholder + refresh button when the list is empty. `source_row_count` is
/// clamped to [`SETTINGS_SOURCE_ROW_VISIBLE_MAX`].
fn settings_sources_stack_height(source_row_count: usize) -> f32 {
    let live = source_row_count.min(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    let cards = if live == 0 {
        // Empty `.desktop-source-empty` placeholder.
        SETTINGS_SOURCE_EMPTY_H
    } else {
        SETTINGS_SOURCE_ROW_H * live as f32 + SETTINGS_SOURCE_GAP * (live - 1) as f32
    };
    // Refresh button hangs below the cards / placeholder (list's last child).
    cards + SETTINGS_SOURCE_REFRESH_GAP + SETTINGS_SOURCE_REFRESH_BTN_H
}

/// M1i fidelity — height the 桌面源 §2 source block contributes to the body:
/// heading label + the LIVE card+refresh stack + the trailing section gap.
/// Unlike the old fixed-reserve version, this now REFLOWS with the live count
/// so the scroll clamp matches what is painted (Tauri's flex column). Single
/// source of truth for the source-block height, folded into
/// [`settings_m2_content_height`].
pub fn settings_sources_content_height(source_row_count: usize) -> f32 {
    SETTINGS_SECTION_LABEL_H
        + settings_sources_stack_height(source_row_count)
        + SETTINGS_SECTION_GAP
}

/// M1i fidelity — the scroll-space SHIFT (>= 0) every section below the 桌面源
/// block moves UP relative to the fixed [`SETTINGS_SOURCE_ROW_VISIBLE_MAX`]
/// reserve baseline, for the given live source count. The perf-and-below
/// geometry fns root at [`settings_perf_origin_y_offset`], which is pinned at
/// the max-reserve baseline; callers fold this delta into the `scroll_offset_y`
/// they pass to those fns (shifting content UP by `delta` is identical to
/// scrolling DOWN by `delta`). This is the single-base-offset reflow mechanism
/// — no per-section signature churn. The 桌面路径 / 监控值 rows take the live
/// count directly instead (they anchor off the refresh button), so the delta
/// is applied only from Performance §5 downward.
pub fn settings_sources_reserve_delta(source_row_count: usize) -> f32 {
    let max = settings_sources_content_height(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    let live = settings_sources_content_height(source_row_count);
    (max - live).max(0.0)
}

/// Round-2 M2 / M1i fidelity — total content height of M1 + M2 sections only.
/// M3 helpers extend this further. The source-block contribution is delegated
/// to [`settings_sources_content_height`] (single source of truth) and now
/// reflects the LIVE `source_row_count` so the scroll clamp shrinks/grows with
/// the rendered list.
pub fn settings_m2_content_height(_viewport: Size, source_row_count: usize) -> f32 {
    settings_m2_origin_y_offset()
        + SETTINGS_SECTION_LABEL_H
        + settings_sources_content_height(source_row_count)
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_INPUT_ROW_H
        + SETTINGS_SECTION_GAP
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_TEXTAREA_H
        + SETTINGS_SECTION_GAP
}

/// Visible body-scroll thumb. Returns `None` when the document fits, matching
/// CSS scrollbar behaviour without adding a separate interactive control.
pub fn settings_scrollbar_thumb_rect(
    viewport: Size,
    content_total_h: f32,
    scroll_offset_y: f32,
) -> Option<Rect> {
    let body = settings_body_rect(viewport);
    let max_scroll = settings_body_max_scroll(content_total_h, viewport);
    if max_scroll <= f32::EPSILON || body.height <= SETTINGS_SCROLLBAR_INSET_Y * 2.0 {
        return None;
    }

    let track_y = body.y + SETTINGS_SCROLLBAR_INSET_Y;
    let track_h = body.height - SETTINGS_SCROLLBAR_INSET_Y * 2.0;
    let visible_ratio = (body.height / content_total_h.max(body.height)).clamp(0.0, 1.0);
    let thumb_h = (track_h * visible_ratio)
        .max(SETTINGS_SCROLLBAR_MIN_THUMB_H)
        .min(track_h);
    let progress = (scroll_offset_y / max_scroll).clamp(0.0, 1.0);

    Some(Rect {
        x: body.right() - SETTINGS_SCROLLBAR_W - 2.0,
        y: track_y + (track_h - thumb_h) * progress,
        width: SETTINGS_SCROLLBAR_W,
        height: thumb_h,
    })
}
