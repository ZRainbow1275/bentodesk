use super::*;

// Round-2 M1 — Dark Settings shell.
//
// Tauri 1.2.4 `frame_060.png` baseline. The Wave K1 light panel above is
// kept alive as orphan (Ruling B) — render.rs no longer paints those rows
// but their rect helpers stay so the dispatch graph compiles.
// =============================================================================

/// Round-2 M1 — panel width in DIPs. TL Ruling 5 2026-05-21: 400→420 so the
/// panel reads ≥420 DIP across all DPI (was 400; with the K1 paint-leak fixed
/// the visual perception of "tiny" came from leak debris, not real geometry,
/// but a 20-DIP bump still leaves room for M3 slider track + value chip pairs).
pub const SETTINGS_PANEL_WIDTH_M1: f32 = 480.0;
/// V21-N90 maximum panel height. The Settings HWND host stays tall for scroll
/// reachability, but N89 proved the visible surface was over-occupying that
/// host. Large viewports therefore use a compact centred cap while small
/// legacy viewports still clamp to the available height.
pub const SETTINGS_PANEL_HEIGHT_MAX: f32 = 768.0;
/// Tauri SettingsPanel uses `max-height: 80vh`. The shell applies this fraction
/// to the target monitor work area before allocating the narrow auxiliary HWND.
pub const SETTINGS_PANEL_MAX_WORKAREA_FRAC: f32 = 0.80;
/// Large Settings hosts centre the visible modal vertically. The previous
/// lower-biased anchor placed the card against the bottom edge of the desktop.
pub const SETTINGS_PANEL_LARGE_VIEWPORT_Y_FRAC: f32 = 0.5;
/// V21-G18 - keep the legacy 800x600 fallback path on the old available-height
/// behavior. Runtime Settings exceeds this height through its capped aux host.
pub const SETTINGS_PANEL_OVERLAY_MIN_VIEWPORT_H: f32 = 600.0;
/// Round-2 M1 — panel corner radius. Tauri `--radius-expanded: 16px`
/// (`SettingsPanel.css`); M1b parity bump 14→16 (was 14 per frame_060).
pub const SETTINGS_PANEL_RADIUS: f32 = 16.0;
/// V-5 (TL re-issue 2026-05-21) — alpha for the 8-DIP outer drop-shadow ring
/// painted around the panel. The shadow is a hard-edged `fill_rounded_rect`
/// (no D2D gaussian blur on the hot path per spec §10) so ANY non-zero alpha
/// reads as a visible halo / "mask ring" against the wallpaper — Tauri 1.2.4
/// achieves the lifted-modal look via CSS gaussian blur which has no D2D
/// equivalent on our render path. Re-issued V-5 contract: "panel 外只露
/// 桌面 wallpaper, 不出现任何 BentoDesk-painted overlay 圈" → alpha = 0.0.
/// `fill_rounded_rect` in `render.rs` early-returns at `color.a <= 0.0`, so
/// this also short-circuits the paint call (zero allocation, zero D2D cost).
/// Re-introducing a non-zero value would resurrect the mask-ring regression;
/// the unit test below pins this at exactly 0.0. Pre-fix v1 was 0.45, v2 was
/// 0.15 (still produced a faint visible ring per V-5 re-audit).
pub const SETTINGS_PANEL_SHADOW_ALPHA: f32 = 0.0;
/// Round-2 M1 — sticky header band height (title + close ×).
pub const SETTINGS_HEADER_H_M1: f32 = 52.0;
/// Round-2 M1 — sticky footer band height ([取消] [保存]).
pub const SETTINGS_FOOTER_H: f32 = 56.0;
/// Round-2 M1 — single row height in the scrollable body.
pub const SETTINGS_ROW_H_M1: f32 = 44.0;
/// Round-2 M1 — horizontal padding inside body rows.
pub const SETTINGS_ROW_PAD_X: f32 = 20.0;
/// Tauri `.settings-panel__body` top padding plus the compact General title
/// band (`20px + 24px`). Keeping this in geometry means paint and hit-testing
/// move together instead of the title being a decorative overlay on row 0.
pub const SETTINGS_BODY_TOP_INSET: f32 = 44.0;
/// Tauri `.settings-panel__body` bottom padding. This prevents the final plugin
/// card from touching the sticky footer when the body reaches max scroll.
pub const SETTINGS_BODY_BOTTOM_INSET: f32 = 20.0;
/// Round-2 M1 — vertical gap between logical sections inside the body.
pub const SETTINGS_SECTION_GAP: f32 = 24.0;
/// V21-T1 (2026-06-21) - compact Settings-only label text role. The global
/// typography table stays unchanged for capsules / expanded panels; Settings
/// uses explicit small roles because the modal is a high-density control panel.
pub const SETTINGS_TEXT_LABEL_SIZE: f32 = 13.0;
/// V21-T1 - label weight mirrors Tauri's regular settings copy, not the global
/// `md` medium weight used by larger shell labels.
pub const SETTINGS_TEXT_LABEL_WEIGHT: u16 = 400;
/// V21-T1 - compact no-wrap value / button text role for Settings chrome.
pub const SETTINGS_TEXT_VALUE_SIZE: f32 = 12.0;
/// V21-T1 - value/button text keeps medium weight so controls remain scannable
/// after the size reduction.
pub const SETTINGS_TEXT_VALUE_WEIGHT: u16 = 500;
/// V21-T1 - short Settings labels use a tight line box; long-form copy still
/// has explicit per-card styles where needed.
pub const SETTINGS_TEXT_LINE_HEIGHT: f32 = 1.0;
/// Tauri `.settings-group__title`: compact uppercase section marker.
pub const SETTINGS_GROUP_TITLE_SIZE: f32 = 10.0;
pub const SETTINGS_GROUP_TITLE_WEIGHT: u16 = 600;
pub const SETTINGS_GROUP_TITLE_TRACKING: f32 = 1.2;
/// Round-2 M1 — top-toggle hit box remains wider than the 44 DIP track.
pub const SETTINGS_TOP_TOGGLE_HIT_W: f32 = 60.0;
/// Round-2 M1 — top-toggle row right-anchored hit-box height.
pub const SETTINGS_TOP_TOGGLE_HIT_H: f32 = 28.0;
/// Round-2 M1 — language dropdown chip width.
pub const SETTINGS_LANGUAGE_CHIP_W: f32 = 130.0;
/// Round-2 M1 — language dropdown chip height.
pub const SETTINGS_LANGUAGE_CHIP_H: f32 = 28.0;
/// Round-2 M1 — chevron column width inside the language chip.
pub const SETTINGS_LANGUAGE_CHEVRON_W: f32 = 14.0;
/// Round-2 M1 — header close-× hit-box size (square).
///
/// V21-G20 — Tauri `.settings-panel__close` is 32×32 px with 8 px radius.
pub const SETTINGS_CLOSE_X_SIZE: f32 = 32.0;
/// Tauri's WebKit scrollbar is a narrow 4-DIP affordance inside the body.
pub const SETTINGS_SCROLLBAR_W: f32 = 4.0;
/// Keep the thumb clear of the sticky header/footer hairlines.
pub const SETTINGS_SCROLLBAR_INSET_Y: f32 = 4.0;
/// A long settings document must still expose a comfortably visible thumb.
pub const SETTINGS_SCROLLBAR_MIN_THUMB_H: f32 = 36.0;
/// Round-2 M1 — footer button width (Cancel + Save share a width).
pub const SETTINGS_FOOTER_BTN_W: f32 = 84.0;
/// Tauri footer actions size to their two-character label plus 20-DIP side
/// padding (`.settings-btn { padding: 8px 20px }`), which resolves to 68 DIPs
/// with the Settings 13-DIP text role. Wider inline actions keep the generic
/// [`SETTINGS_FOOTER_BTN_W`] above so longer labels never clip.
pub const SETTINGS_FOOTER_ACTION_BTN_W: f32 = 68.0;
/// Round-2 M1 — footer button height.
pub const SETTINGS_FOOTER_BTN_H: f32 = 32.0;
/// Round-2 M1 — gap between Cancel and Save in the footer.
pub const SETTINGS_FOOTER_BTN_GAP: f32 = 8.0;

/// Round-2 M1 — number of top-section toggle rows (5 toggles + 1 language
/// row living inside the same logical section). Pinned by a test below.
pub const SETTINGS_TOP_TOGGLE_COUNT: u8 = 5;

/// α4 (Wave I-α, 2026-05-25) / Tauri parity (2026-07-15) —
/// zone-display-mode picker geometry. Tauri's `.settings-display-mode` is a
/// right-aligned vertical stack of three full-width option cards, not three
/// compressed inline radios. Shared geometry keeps paint, hit-testing and
/// scroll height in lockstep.
/// Number of choices in the zone-display-mode picker.
pub const SETTINGS_ZONE_DISPLAY_MODE_COUNT: u8 = 3;
/// Outer circle diameter (DIP) of one radio.
pub const SETTINGS_RADIO_OUTER_D: f32 = 14.0;
/// Inner dot diameter (DIP) when a radio is selected.
pub const SETTINGS_RADIO_INNER_D: f32 = 6.0;
/// Tauri `.settings-display-mode { min-width: 220px }`.
pub const SETTINGS_RADIO_W: f32 = 220.0;
/// 13-DIP label line plus Tauri's `8px` vertical option padding.
pub const SETTINGS_RADIO_H: f32 = 36.0;
/// Vertical gap between adjacent option cards.
pub const SETTINGS_RADIO_GAP: f32 = 8.0;
/// Horizontal inset inside each option card.
pub const SETTINGS_DISPLAY_MODE_OPTION_PAD_X: f32 = 12.0;
/// Gap between the radio circle and its full option label.
pub const SETTINGS_DISPLAY_MODE_OPTION_LABEL_GAP: f32 = 10.0;
/// Breathing room between the left explanatory copy and the option stack.
pub const SETTINGS_DISPLAY_MODE_COPY_GAP: f32 = 16.0;
/// Left-side primary copy line height.
pub const SETTINGS_DISPLAY_MODE_COPY_LABEL_H: f32 = 18.0;
/// Left-side two-line hint band.
pub const SETTINGS_DISPLAY_MODE_HINT_H: f32 = 34.0;
/// Gap between the primary copy and hint.
pub const SETTINGS_DISPLAY_MODE_HINT_GAP: f32 = 4.0;

/// Round-2 M2 — height of a section label band (the dim header text above
/// 桌面源 / 桌面路径 / 监控值).
pub const SETTINGS_SECTION_LABEL_H: f32 = 24.0;

/// Round-2 M2 / M1i fidelity (2026-05-29) — height of one 桌面源 card row.
/// Tauri `.desktop-source-card` is `padding: 8px 10px` around a 28-DIP icon /
/// two-line body (13 + 11 px text + 2 px internal gap). 8 (top pad) + 28
/// (icon, the tallest flex child) + 8 (bottom pad) = 44.
pub const SETTINGS_SOURCE_ROW_H: f32 = 44.0;

/// M1i 2026-05-29 — number of source-card slots the §2 Paths layout caps the
/// list at (and the max cards the renderer paints). The list is dynamic, fed
/// from `desktop_sources::all_desktop_dirs`; the realistic Windows ceiling is
/// User + Public + OneDrive + Custom = 4. M1i fidelity fix — the list now
/// reflows to the LIVE count (Tauri's flex column): every section below it
/// shifts up/down by the height of the missing/extra cards. The renderer
/// paints `min(live_count, MAX)` cards. Mirrors the visible-cap rhythm of
/// [`SETTINGS_PLUGINS_ROW_VISIBLE_MAX`] / [`SETTINGS_BACKUP_ROW_VISIBLE_MAX`].
pub const SETTINGS_SOURCE_ROW_VISIBLE_MAX: u8 = 4;

/// Round-2 M2 / M1i fidelity — vertical gap between two source cards. Tauri
/// `.desktop-source-list { gap: 6px }`.
pub const SETTINGS_SOURCE_GAP: f32 = 6.0;

/// M1i fidelity — the `.desktop-source-empty` placeholder height (one italic
/// 11-px line, `padding: 6px 4px`). 6 + ~12 (line box) + 6 = 24.
pub const SETTINGS_SOURCE_EMPTY_H: f32 = 24.0;

/// M1i fidelity — the `.desktop-source-refresh` button row. The button is the
/// LAST child of the list (`align-self: flex-end`), below the cards. Tauri
/// `padding: 4px 10px; min-width: 32px; font-size: 14px`. 4 + ~16 + 4 = 24
/// tall.
pub const SETTINGS_SOURCE_REFRESH_BTN_W: f32 = 36.0;
pub const SETTINGS_SOURCE_REFRESH_BTN_H: f32 = 24.0;
/// M1i fidelity — gap between the last card (or empty placeholder) and the
/// right-anchored refresh button.
pub const SETTINGS_SOURCE_REFRESH_GAP: f32 = 6.0;

/// Round-2 M2 — text input row (single-line) used by 桌面路径.
pub const SETTINGS_INPUT_ROW_H: f32 = 40.0;

/// Round-2 M2 — multi-line textarea used by 监控值. 4 visible lines worth
/// of dark surface — enough to hint at scroll without taking over.
pub const SETTINGS_TEXTAREA_H: f32 = 96.0;

/// Round-2 M1 — logical sections inside the Settings dialog body.
///
/// M1 only paints `TopToggles + Language`; M2/M3/M4 add the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSectionId {
    /// Top 5 toggle rows: desktop_embed / autostart / show_in_taskbar /
    /// smart_layout / speed_mode.
    TopToggles,
    /// Language dropdown row (语言 / Language [中文 ▾]).
    Language,
}

/// Whether the Settings card occupies the complete native HWND client area.
///
/// Production uses a 480-DIP panel-sized popup. Wider viewports remain useful
/// for deterministic overlay/layout tests and legacy embedded callers.
#[inline]
pub fn settings_panel_fills_host(viewport: Size) -> bool {
    viewport.width <= SETTINGS_PANEL_WIDTH_M1 + 0.5
}

/// Round-2 M1 — compute the dark Settings panel rect for the supplied
/// viewport. The production panel-sized HWND is filled exactly; legacy wider
/// viewports keep the centred overlay geometry.
pub fn settings_panel_rect_m1(viewport: Size) -> Rect {
    if settings_panel_fills_host(viewport) {
        return Rect {
            x: 0.0,
            y: 0.0,
            width: viewport.width.max(0.0),
            height: viewport.height.max(0.0),
        };
    }
    let panel_w = SETTINGS_PANEL_WIDTH_M1.min(viewport.width);
    let available_h = (viewport.height - SETTINGS_PANEL_TOP_MARGIN * 2.0).max(0.0);
    let css_max_h = if viewport.height > SETTINGS_PANEL_OVERLAY_MIN_VIEWPORT_H {
        viewport.height * SETTINGS_PANEL_MAX_WORKAREA_FRAC
    } else {
        available_h
    };
    let panel_h = SETTINGS_PANEL_HEIGHT_MAX.min(available_h).min(css_max_h);
    let panel_y = if viewport.height > SETTINGS_PANEL_OVERLAY_MIN_VIEWPORT_H {
        ((viewport.height - panel_h) * SETTINGS_PANEL_LARGE_VIEWPORT_Y_FRAC)
            .max(SETTINGS_PANEL_TOP_MARGIN)
    } else {
        SETTINGS_PANEL_TOP_MARGIN
    };
    Rect {
        x: ((viewport.width - panel_w) * 0.5).max(0.0),
        y: panel_y,
        width: panel_w,
        height: panel_h,
    }
}

/// Round-2 M1 — sticky header band (title + close ×, NOT scrolled).
pub fn settings_header_rect(viewport: Size) -> Rect {
    let p = settings_panel_rect_m1(viewport);
    Rect {
        x: p.x,
        y: p.y,
        width: p.width,
        height: SETTINGS_HEADER_H_M1,
    }
}

/// Round-2 M1 — sticky footer band (Cancel + Save, NOT scrolled).
pub fn settings_footer_rect(viewport: Size) -> Rect {
    let p = settings_panel_rect_m1(viewport);
    Rect {
        x: p.x,
        y: p.bottom() - SETTINGS_FOOTER_H,
        width: p.width,
        height: SETTINGS_FOOTER_H,
    }
}

/// Round-2 M1 — scroll-clipped body rect (between header and footer).
pub fn settings_body_rect(viewport: Size) -> Rect {
    let p = settings_panel_rect_m1(viewport);
    let body_top = p.y + SETTINGS_HEADER_H_M1;
    let body_bottom = p.bottom() - SETTINGS_FOOTER_H;
    Rect {
        x: p.x,
        y: body_top,
        width: p.width,
        height: (body_bottom - body_top).max(0.0),
    }
}

/// Round-2 M1 — header close-× hit-box (sticky, NOT scrolled).
pub fn settings_close_button_rect_m1(viewport: Size) -> Rect {
    let header = settings_header_rect(viewport);
    Rect {
        x: header.right() - SETTINGS_ROW_PAD_X - SETTINGS_CLOSE_X_SIZE,
        y: header.y + (header.height - SETTINGS_CLOSE_X_SIZE) * 0.5,
        width: SETTINGS_CLOSE_X_SIZE,
        height: SETTINGS_CLOSE_X_SIZE,
    }
}

/// Round-2 M1 — footer Save button (right-aligned, accent fill).
pub fn settings_save_button_rect(viewport: Size) -> Rect {
    let footer = settings_footer_rect(viewport);
    Rect {
        x: footer.right() - SETTINGS_ROW_PAD_X - SETTINGS_FOOTER_ACTION_BTN_W,
        y: footer.y + (footer.height - SETTINGS_FOOTER_BTN_H) * 0.5,
        width: SETTINGS_FOOTER_ACTION_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// Round-2 M1 — footer Cancel button (left of Save).
pub fn settings_cancel_button_rect(viewport: Size) -> Rect {
    let save = settings_save_button_rect(viewport);
    Rect {
        x: save.x - SETTINGS_FOOTER_BTN_GAP - SETTINGS_FOOTER_ACTION_BTN_W,
        y: save.y,
        width: SETTINGS_FOOTER_ACTION_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// Round-2 M1 — content-space origin for body content. Subtract this from
/// the absolute paint Y of a body row to get its position in scroll space.
pub(super) fn settings_body_content_origin(viewport: Size, scroll_offset_y: f32) -> f32 {
    settings_body_rect(viewport).y + SETTINGS_BODY_TOP_INSET - scroll_offset_y
}

/// Tauri General-group title inside the scrollable body. The title starts at
/// the body's 20-DIP inset; the first toggle row begins after the complete
/// 44-DIP top/title band through [`settings_body_content_origin`].
pub fn settings_general_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let body = settings_body_rect(viewport);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: body.y + 20.0 - scroll_offset_y,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// Round-2 M1 — full row rect for the top-section toggle at `index`
/// (`0..SETTINGS_TOP_TOGGLE_COUNT`). Honours scroll offset.
pub fn settings_top_toggle_row_rect(viewport: Size, scroll_offset_y: f32, index: u8) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + SETTINGS_ROW_H_M1 * index as f32,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_ROW_H_M1,
    }
}

/// Round-2 M1 — right-anchored toggle hit-box inside a top-section row.
pub fn settings_top_toggle_hit_rect(viewport: Size, scroll_offset_y: f32, index: u8) -> Rect {
    let row = settings_top_toggle_row_rect(viewport, scroll_offset_y, index);
    Rect {
        x: row.right() - SETTINGS_TOP_TOGGLE_HIT_W,
        y: row.y + (row.height - SETTINGS_TOP_TOGGLE_HIT_H) * 0.5,
        width: SETTINGS_TOP_TOGGLE_HIT_W,
        height: SETTINGS_TOP_TOGGLE_HIT_H,
    }
}

/// Round-2 M1 — full row rect for the language dropdown row. Sits directly
/// below the 5 toggle rows.
pub fn settings_language_row_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + SETTINGS_ROW_H_M1 * SETTINGS_TOP_TOGGLE_COUNT as f32,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_ROW_H_M1,
    }
}

/// Round-2 M1 — language dropdown chip rect (right-anchored inside the
/// language row).
pub fn settings_language_chip_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let row = settings_language_row_rect(viewport, scroll_offset_y);
    Rect {
        x: row.right() - SETTINGS_LANGUAGE_CHIP_W,
        y: row.y + (row.height - SETTINGS_LANGUAGE_CHIP_H) * 0.5,
        width: SETTINGS_LANGUAGE_CHIP_W,
        height: SETTINGS_LANGUAGE_CHIP_H,
    }
}

/// Round-2 M1 — chevron sub-rect on the right of the language chip.
pub fn settings_language_chevron_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let chip = settings_language_chip_rect(viewport, scroll_offset_y);
    Rect {
        x: chip.right() - SETTINGS_LANGUAGE_CHEVRON_W - 6.0,
        y: chip.y + (chip.height - 16.0) * 0.5,
        width: SETTINGS_LANGUAGE_CHEVRON_W,
        height: 16.0,
    }
}

/// Round-2 M1 — label sub-rect inside the language chip (chevron excluded).
pub fn settings_language_chip_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let chip = settings_language_chip_rect(viewport, scroll_offset_y);
    Rect {
        x: chip.x + 10.0,
        y: chip.y + (chip.height - 16.0) * 0.5,
        width: (chip.width - SETTINGS_LANGUAGE_CHEVRON_W - 14.0).max(0.0),
        height: 16.0,
    }
}

/// G3 parity (2026-06-01) — the §4 DisplayMode group title rect (`显示模式 /
/// Display Mode`). The zone-display-mode picker was promoted out of the General
/// band into its own `settings-group` §4 between §3 Appearance and §5
/// Performance, matching Tauri's body order (`SettingsPanel.tsx:538-598`
/// `<section class="settings-group"><h3>{settingsGroupDisplayMode}</h3>`).
/// Anchors off the §3 Appearance section bottom (its last element is the accent
/// swatch row, whose bottom is `appearance_content_height − section_gap` below
/// the Appearance title) plus a section gap. The `flags` arg is unused (the
/// section roots at the fixed source-reserve baseline like Appearance §5) but
/// kept for signature symmetry with the Appearance helpers.
///
/// G3 reuses the existing bilingual `SETTINGS_ZONE_DISPLAY_MODE_LABEL`
/// (StringId 140 — "默认显示模式" / "Default display mode") as the group title
/// rather than minting a new StringId (spec §8 — no new strings unless
/// required). The renderer paints the group title here and the radios below;
/// the per-row caption that previously sat to the left of the radios is dropped
/// (the group title now carries the section name).
pub fn settings_display_mode_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    // §3 Appearance roots at `settings_appearance_origin_y_offset`; its content
    // height (title + picker label + grid + trailing gap) minus the trailing gap
    // is the section bottom. §4 DisplayMode sits one section gap below it.
    let appearance_bottom = settings_appearance_origin_y_offset()
        + settings_appearance_content_height(viewport)
        - SETTINGS_SECTION_GAP;
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + appearance_bottom + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// α4 (Wave I-α, 2026-05-25) / G3 parity (2026-06-01) — full row rect for the
/// zone-display-mode picker (the 3 radios). Now sits directly below the §4
/// DisplayMode group title ([`settings_display_mode_label_rect`]) as a standalone
/// section between §3 Appearance and §5 Performance — promoted out of the
/// General band per Tauri parity. Honours scroll offset the same way every other
/// body row does.
pub fn settings_zone_display_mode_picker_row_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let label = settings_display_mode_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_RADIO_H * SETTINGS_ZONE_DISPLAY_MODE_COUNT as f32
            + SETTINGS_RADIO_GAP * (SETTINGS_ZONE_DISPLAY_MODE_COUNT - 1) as f32,
    }
}

/// α4 — sub-rect for radio `index` (0 = Hover, 1 = Always, 2 = Click).
/// The three 220-DIP cards right-anchor and stack vertically with an 8-DIP
/// gap, matching Tauri's `.settings-display-mode` control.
pub fn settings_zone_display_mode_radio_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let row = settings_zone_display_mode_picker_row_rect(viewport, scroll_offset_y);
    Rect {
        x: row.right() - SETTINGS_RADIO_W,
        y: row.y + (SETTINGS_RADIO_H + SETTINGS_RADIO_GAP) * index as f32,
        width: SETTINGS_RADIO_W,
        height: SETTINGS_RADIO_H,
    }
}

/// α4 — outer-circle paint rect for radio `index`. Sits inside the
/// hit-box, left-anchored, vertically centred.
pub fn settings_zone_display_mode_radio_outer_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let hit = settings_zone_display_mode_radio_rect(viewport, scroll_offset_y, index);
    Rect {
        x: hit.x + SETTINGS_DISPLAY_MODE_OPTION_PAD_X,
        y: hit.y + (hit.height - SETTINGS_RADIO_OUTER_D) * 0.5,
        width: SETTINGS_RADIO_OUTER_D,
        height: SETTINGS_RADIO_OUTER_D,
    }
}

/// α4 — inner-dot paint rect for radio `index` (only painted when this
/// mode is the current `zone_display_mode`).
pub fn settings_zone_display_mode_radio_inner_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let outer = settings_zone_display_mode_radio_outer_rect(viewport, scroll_offset_y, index);
    Rect {
        x: outer.x + (SETTINGS_RADIO_OUTER_D - SETTINGS_RADIO_INNER_D) * 0.5,
        y: outer.y + (SETTINGS_RADIO_OUTER_D - SETTINGS_RADIO_INNER_D) * 0.5,
        width: SETTINGS_RADIO_INNER_D,
        height: SETTINGS_RADIO_INNER_D,
    }
}

/// α4 — label rect for radio `index`. Sits to the right of the outer
/// circle with Tauri's 10-DIP gap and keeps the trailing 12-DIP card padding.
pub fn settings_zone_display_mode_radio_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let hit = settings_zone_display_mode_radio_rect(viewport, scroll_offset_y, index);
    let outer = settings_zone_display_mode_radio_outer_rect(viewport, scroll_offset_y, index);
    let label_x = outer.right() + SETTINGS_DISPLAY_MODE_OPTION_LABEL_GAP;
    Rect {
        x: label_x,
        y: hit.y + (hit.height - 16.0) * 0.5,
        width: (hit.right() - SETTINGS_DISPLAY_MODE_OPTION_PAD_X - label_x).max(0.0),
        height: 16.0,
    }
}

/// Left-side primary copy (`Zone 唤醒方式` / `How zones reveal`).
pub fn settings_display_mode_copy_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let row = settings_zone_display_mode_picker_row_rect(viewport, scroll_offset_y);
    let copy_h = SETTINGS_DISPLAY_MODE_COPY_LABEL_H
        + SETTINGS_DISPLAY_MODE_HINT_GAP
        + SETTINGS_DISPLAY_MODE_HINT_H;
    Rect {
        x: row.x,
        y: row.y + (row.height - copy_h) * 0.5,
        width: (row.width - SETTINGS_RADIO_W - SETTINGS_DISPLAY_MODE_COPY_GAP).max(0.0),
        height: SETTINGS_DISPLAY_MODE_COPY_LABEL_H,
    }
}

/// Left-side explanatory hint, directly below the primary copy.
pub fn settings_display_mode_hint_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let label = settings_display_mode_copy_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.x,
        y: label.bottom() + SETTINGS_DISPLAY_MODE_HINT_GAP,
        width: label.width,
        height: SETTINGS_DISPLAY_MODE_HINT_H,
    }
}

/// G3 parity (2026-06-01) — height the §4 DisplayMode group contributes to
/// `settings_body_content_height`: the group title + the picker (radio) row +
/// a trailing section gap. PURE — no global state. Mirrors the term rhythm of
/// the other section content-height helpers so the scroll clamp stays exact.
pub fn settings_display_mode_content_height() -> f32 {
    SETTINGS_SECTION_LABEL_H
        + SETTINGS_RADIO_H * SETTINGS_ZONE_DISPLAY_MODE_COUNT as f32
        + SETTINGS_RADIO_GAP * (SETTINGS_ZONE_DISPLAY_MODE_COUNT - 1) as f32
        + SETTINGS_SECTION_GAP
}

/// M1f — which Updater §8 status family is live, for height purposes only.
/// The updater card height depends on whether the version block, the progress
/// bar, and/or the error line are shown — and those three are mutually
/// determined by the status family. Collapsing the status to this 4-way
/// discriminant keeps [`SettingsBodyFlags`] `Copy` + tiny (no SmolStr) while
/// still driving an exact dynamic height. Pure data — geometry reads it, never
/// global state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpdaterHeightKind {
    /// Idle / Checking / UpToDate — status row only, no extra blocks.
    StatusOnly,
    /// Available / Ready / Installing / Skipped — adds the version block.
    Versioned,
    /// Downloading — adds the progress bar (no version block).
    Downloading,
    /// Error — adds the wrapped error line (no version block).
    Error,
}

/// M1d + M1e + M1f — the conditional-row gating flags fed into the dynamic
/// body-height + scroll-clamp geometry. Bundling them in one `Copy` struct
/// keeps `settings_body_content_height` / `settings_clamp_scroll` (and the
/// shell call sites) under clippy's `too_many_arguments` threshold as more
/// sections light up. Geometry stays PURE: every flag is passed in, nothing
/// inside reads global state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SettingsBodyFlags {
    /// Startup §6 — crash-auto-restart toggle on (shows the 2 stepper rows).
    pub crash_restart_enabled: bool,
    /// Startup §6 — safe-start-after-hibernation on (shows the resume slider).
    pub safe_start_after_hibernation: bool,
    /// Stealth §7 — `retry_count > 0` (shows the retry row + OneDrive block).
    pub stealth_has_retry: bool,
    /// Stealth §7 — `last_error.is_some()` (shows the error block).
    pub stealth_has_error: bool,
    /// Updater §8 — which status family is live (drives version/progress/error
    /// block visibility, hence the card height).
    pub updater_kind: UpdaterHeightKind,
    /// Backup §9 — number of backup rows the list paints (already capped at
    /// [`SETTINGS_BACKUP_ROW_VISIBLE_MAX`] by the caller via
    /// `backup_card::backup_visible_row_count`). The list is variable-length,
    /// so its height grows one [`SETTINGS_BACKUP_ROW_H`] per visible row; an
    /// empty list shows the single `backupEmpty` placeholder row instead.
    pub backup_row_count: usize,
    /// Backup §9 — whether the action status line is currently visible. Tauri
    /// does not reserve an empty status row, so this flag keeps list geometry,
    /// hit-testing and the scroll clamp aligned with the painted card.
    pub backup_status_present: bool,
    /// Encryption §10 — whether its success/error status line is visible.
    /// Plugins anchor directly after the hint when no status is present.
    pub encryption_status_present: bool,
    /// Plugins §11 — number of plugin cards the list paints (already capped at
    /// [`SETTINGS_PLUGINS_ROW_VISIBLE_MAX`] by the caller via
    /// `plugins_section::plugin_visible_row_count`). Like the backup list this
    /// is variable-length: each visible plugin card adds one
    /// [`SETTINGS_PLUGIN_CARD_H`] (+ inter-card gap); an empty list shows the
    /// single `pluginEmpty` placeholder row instead.
    pub plugin_row_count: usize,
    /// Plugins §11 — whether a real lifecycle status line is visible below the
    /// install button.
    pub plugin_status_present: bool,
    /// Paths §2 — number of dynamic desktop-source cards the §2 list paints
    /// (already capped at [`SETTINGS_SOURCE_ROW_VISIBLE_MAX`] by the caller).
    /// M1i fidelity (2026-05-29) — this section sits MID-body and now REFLOWS:
    /// the live count drives the source-block height
    /// ([`settings_sources_content_height`]) and, via
    /// [`settings_sources_reserve_delta`], shifts every section below it
    /// up/down (Tauri's flex column). The count threads into
    /// [`settings_body_content_height`] so the scroll clamp matches the live
    /// height, exactly like `with_backup_rows` / `with_plugin_rows`.
    pub source_row_count: usize,
}

impl SettingsBodyFlags {
    /// Convenience constructor used by tests + call sites that only vary the
    /// Startup/Stealth bools (updater idle, no backups). Keeps the common case
    /// terse; `with_backup_rows` layers the variable backup-list count on top.
    pub const fn new(
        crash_restart_enabled: bool,
        safe_start_after_hibernation: bool,
        stealth_has_retry: bool,
        stealth_has_error: bool,
        updater_kind: UpdaterHeightKind,
    ) -> Self {
        Self {
            crash_restart_enabled,
            safe_start_after_hibernation,
            stealth_has_retry,
            stealth_has_error,
            updater_kind,
            backup_row_count: 0,
            backup_status_present: false,
            encryption_status_present: false,
            plugin_row_count: 0,
            plugin_status_present: false,
            source_row_count: 0,
        }
    }

    /// M1g — return a copy with the Backup §9 visible-row count set. Builder
    /// style keeps `new()`'s arity fixed (no clippy `too_many_arguments`
    /// regression) while letting the shell + renderer feed the live capped
    /// count into the dynamic body height + scroll clamp.
    pub const fn with_backup_rows(mut self, backup_row_count: usize) -> Self {
        self.backup_row_count = backup_row_count;
        self
    }

    /// Return a copy with the Backup §9 status-row visibility set.
    pub const fn with_backup_status(mut self, present: bool) -> Self {
        self.backup_status_present = present;
        self
    }

    /// Return a copy with the Encryption §10 status-row visibility set.
    pub const fn with_encryption_status(mut self, present: bool) -> Self {
        self.encryption_status_present = present;
        self
    }

    /// M1h — return a copy with the Plugins §11 visible-row count set. Same
    /// builder rationale as [`Self::with_backup_rows`]: keeps `new()`'s arity
    /// fixed while feeding the live capped plugin-card count into the dynamic
    /// body height + scroll clamp so paint / hit / scroll all agree.
    pub const fn with_plugin_rows(mut self, plugin_row_count: usize) -> Self {
        self.plugin_row_count = plugin_row_count;
        self
    }

    /// Return a copy with the Plugins §11 lifecycle status-row visibility set.
    pub const fn with_plugin_status(mut self, present: bool) -> Self {
        self.plugin_status_present = present;
        self
    }

    /// M1i — return a copy with the Paths §2 source-card count set. Same
    /// builder rationale as [`Self::with_plugin_rows`]: keeps `new()`'s arity
    /// fixed while feeding the live capped desktop-source count through the
    /// shared `SettingsBodyFlags` so paint / hit / scroll all read one count.
    pub const fn with_source_rows(mut self, source_row_count: usize) -> Self {
        self.source_row_count = source_row_count;
        self
    }
}

/// Round-2 M1/M2 + M1d + M1e + M1f — total content height inside the body.
/// Grows with each milestone as sections light up. The Startup §6, Stealth §7
/// and Updater §8 sections all have conditional rows, so their gating lives in
/// [`SettingsBodyFlags`] (passed by ref) — geometry never reads global state,
/// the shell passes the live values.
pub fn settings_body_content_height(viewport: Size, flags: &SettingsBodyFlags) -> f32 {
    // G3 parity (2026-06-01) — terms summed in the NEW Tauri body order:
    // General(+Paths §2) → Appearance §3 → DisplayMode §4 → Performance §5 →
    // Startup §6 → Stealth §7 → Updater §8 → Backup §9 → Encryption §10 →
    // Plugins §11. The total is order-independent (it is a sum), but the
    // ordering is kept readable to mirror the laid-out chain.
    SETTINGS_BODY_TOP_INSET
        + settings_m2_content_height(viewport, flags.source_row_count)
        // §3 Appearance grid — body-width-driven (4-col card grid), now between
        // §2 Paths and §4 DisplayMode (was painted LAST pre-G3).
        + settings_appearance_content_height(viewport)
        // §4 DisplayMode — promoted out of the General band into its own group.
        + settings_display_mode_content_height()
        + settings_perf_startup_content_height(
            viewport,
            flags.crash_restart_enabled,
            flags.safe_start_after_hibernation,
        )
        + settings_stealth_content_height(flags.stealth_has_retry, flags.stealth_has_error)
        + settings_updater_content_height(flags.updater_kind)
        + settings_backup_content_height_for_status(
            flags.backup_row_count,
            flags.backup_status_present,
        )
        // M7 — §10 Encryption card slots between Backup §9 and Plugins §11
        // (Tauri `<BackupCard/><EncryptionCard/>` adjacency). Fixed-height, so a
        // single constant additive term (no `SettingsBodyFlags` field needed).
        + settings_encryption_content_height_for_status(flags.encryption_status_present)
        + settings_plugins_content_height_for_status(
            flags.plugin_row_count,
            flags.plugin_status_present,
        )
        + SETTINGS_BODY_BOTTOM_INSET
}

/// Round-2 M1 — clamp `requested_offset` to `[0, max_scroll]` where
/// `max_scroll = max(0, content_h - body_h)`. Returns 0 when the content
/// already fits, so the scroll Cell can never go negative.
pub fn settings_body_max_scroll(content_total_h: f32, viewport: Size) -> f32 {
    let body = settings_body_rect(viewport);
    (content_total_h - body.height).max(0.0)
}

/// Round-2 M1 + M1d + M1e + M1f — apply a wheel-delta `delta_y` (positive =
/// scroll down) to `current_offset` and clamp. Pure helper so the wheel
/// handler stays allocation-free. The [`SettingsBodyFlags`] feed the dynamic
/// content height so the max-scroll matches whatever conditional rows are
/// currently visible (Startup crash steppers + hibernate slider; Stealth
/// retry/error rows; Updater version/progress/error block).
pub fn settings_clamp_scroll(
    current_offset: f32,
    delta_y: f32,
    viewport: Size,
    flags: &SettingsBodyFlags,
) -> f32 {
    let next = (current_offset + delta_y).max(0.0);
    let content_h = settings_body_content_height(viewport, flags);
    let max = settings_body_max_scroll(content_h, viewport);
    next.min(max)
}
