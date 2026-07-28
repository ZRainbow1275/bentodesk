use super::*;

// ── M6-UI 2026-05-29 / G3 parity 2026-06-01 — §3 Appearance inline theme grid (`SettingsPanel.tsx:396-536`) ──
//
// G3 parity (2026-06-01): the §3 Appearance section now sits between §2 Paths
// and §4 DisplayMode — matching Tauri's body order General → Paths →
// **Appearance** → DisplayMode → Performance. Previously native painted it LAST
// (after Plugins §11). The grid geometry (group headings + 17 ThemeCards +
// accent swatch row) is owned by `crate::theme_picker::appearance_layout`,
// which is body-width-driven and fully `Copy` (fixed-cap `[Rect; N]`, no `Vec`
// — §10). These helpers only resolve the section's scroll-space ANCHOR (so
// paint / hit / scroll all agree) and the content-width fed to the layout.
//
// The section flows inside the body D2D scroll-clip (`push_clip(body_rect)`),
// so partial rows at the body edge are masked exactly like every other section.

/// G3 parity — scroll-space Y at which the §3 Appearance group title starts,
/// PINNED at the fixed [`SETTINGS_SOURCE_ROW_VISIBLE_MAX`] source-reserve
/// baseline (same single-base-offset reflow mechanism as
/// [`settings_perf_origin_y_offset`]). It sits one section gap below the §2
/// 监控值 textarea bottom (the last element of the Paths section), computed at
/// the full source reserve. Callers fold [`settings_sources_reserve_delta`]
/// into `scroll_offset_y` so the live source-count reflow shifts Appearance +
/// everything below it.
pub(super) fn settings_appearance_origin_y_offset() -> f32 {
    settings_m2_origin_y_offset()
        + settings_sources_content_height(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize)
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_INPUT_ROW_H
        + SETTINGS_SECTION_GAP
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_TEXTAREA_H
        + SETTINGS_SECTION_GAP
}

/// M6-UI / G3 parity — the §3 Appearance group title rect. Anchors off the §2
/// Paths section bottom (the 监控值 textarea) plus a section gap. The `flags`
/// arg is retained for call-site stability (renderer + hit-tester) but no
/// longer read — the section now roots at the fixed source-reserve baseline
/// like the Performance §5 chain, so its Y is independent of the Backup/Plugins
/// row counts below it.
pub fn settings_appearance_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    _flags: &SettingsBodyFlags,
) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + settings_appearance_origin_y_offset(),
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M6-UI — content width fed to `theme_picker::appearance_layout` (body width
/// minus the two row pads). The 4-col card width derives from this.
pub fn settings_appearance_inner_width(viewport: Size) -> f32 {
    let body = settings_body_rect(viewport);
    (body.width - SETTINGS_ROW_PAD_X * 2.0).max(0.0)
}

/// M6-UI — scroll-space origin (top-left of the grid, below the group title +
/// the picker label) for `theme_picker::appearance_layout`. The renderer + the
/// hit-tester both call this so the inline layout shares one anchor.
pub fn settings_appearance_grid_origin(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> crate::theme_picker::Point {
    let label = settings_appearance_label_rect(viewport, scroll_offset_y, flags);
    crate::theme_picker::Point {
        x: label.x,
        // Group title + a "Choose Theme" picker label line precede the grid.
        y: label.bottom() + SETTINGS_SECTION_LABEL_H,
    }
}

/// M6-UI — the "选择主题 / Choose Theme" picker-label rect (between the
/// Appearance group title and the grid).
pub fn settings_appearance_picker_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let label = settings_appearance_label_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M6-UI — height the §3 Appearance section contributes to
/// `settings_body_content_height`: the group title + the picker label + the
/// grid/accent layout `total_height` + a trailing section gap. PURE — the grid
/// height delegates to `theme_picker::appearance_content_height` (single source
/// of truth) for the given body width; no global state is read.
pub fn settings_appearance_content_height(viewport: Size) -> f32 {
    SETTINGS_SECTION_LABEL_H
        + SETTINGS_SECTION_LABEL_H
        + crate::theme_picker::appearance_content_height(settings_appearance_inner_width(viewport))
        + SETTINGS_SECTION_GAP
}
