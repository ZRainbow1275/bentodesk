//! Wave I2 (05-20 visual parity) — expanded zone body chrome geometry.
//!
//! Tauri 1.2.4 renders a clicked-open zone (`BentoPanel.tsx`) as a dark
//! translucent rounded panel with: a panel drop shadow, a header band
//! (folder glyph + title + item-count badge), a 1-DIP divider line between
//! the header and the item grid, and a grid of item cards. Geometry
//! constants live here so the renderer + future hit-test surfaces share one
//! source of truth.
//!
//! M2 (05-29 1:1 fixes): the old 16×16 sub-zone footer thumbnail strip
//! (E-01) was DELETED — Tauri's `BentoPanel` renders header + grid only,
//! with no footer node. The expanded green status dot (E-02) was likewise
//! removed; Tauri's `PanelHeader` carries a numeric count badge, not a dot.
//!
//! The item-card rectangles themselves are **not** owned by this module —
//! they are sourced from
//! `bento_nano_app::business::highlight_overlay::item_card_rect_for_grid`
//! which is the existing SSoT shared with `hit_test_zone_item` in the
//! shell. Wave I2 only adds the panel chrome surrounding the items.
//!
//! Spec §3.2 100% self-rolled / spec §8 no new crate deps / spec §10 zero
//! allocation hot-path: every helper here returns `Copy` rects, no `Vec`,
//! no `String`.

use bento_nano_style::Rect;
use bento_nano_style::tokens::{SHADOW, SPACING, TYPOGRAPHY};
use bento_nano_zone::Zone;

/// Header band height in DIPs — bound to the single grid-top SSoT
/// (`item_grid::ITEM_GRID_TOP_OFFSET_PX`) so the divider line lands exactly
/// on the seam between the header band and the first item row.
///
/// M2③ (05-31, ruling = A / 1:1): realigned from the legacy 30 to Tauri's
/// `.panel-header { height: 48px }` (PanelHeader.css:6). Because the band
/// height and the grid-top offset are now the same constant, every
/// header-derived offset (divider Y, badge centring, item-grid top, and the
/// shell hit-rects) cascades from this one change automatically.
pub const HEADER_BAND_HEIGHT: f32 = crate::business::item_grid::ITEM_GRID_TOP_OFFSET_PX;

/// Horizontal inset applied to the header band content (icon start, actions
/// right edge). GROUP-4 1:1 fix #6: realigned from the legacy 8 to Tauri's
/// `.panel-header { padding: 0 var(--spacing-lg) }` = 16 (PanelHeader.css:7,
/// `--spacing-lg = 16`). Every header child (icon, title, badge, action
/// buttons) now starts/ends 16 DIPs in from the panel edge.
pub const HEADER_INSET_X: f32 = SPACING.lg;

/// Horizontal inset of the divider line from the panel edges. GROUP-4 1:1
/// fix #5: Tauri's `.panel-header { border-bottom: 1px solid
/// rgba(255,255,255,0.05) }` is a CSS border-box rule spanning the FULL
/// header width (the 16px padding is *inside* the border-box), so the
/// hairline runs edge-to-edge between the panel's rounded corners. Dropped
/// from the legacy 16 to 0 so the divider no longer stops 16 DIPs short at
/// each end.
pub const DIVIDER_INSET_X: f32 = 0.0;

/// Gap between header flex children — Tauri `.panel-header { gap:
/// var(--spacing-sm) }` = 8 (PanelHeader.css:8). Sits between icon→title,
/// title→badge, and badge→actions.
pub const HEADER_GAP: f32 = SPACING.sm;

/// Zone glyph footprint in the header. Tauri `.panel-header__icon { font-size:
/// 18px }` with no background chip (PanelHeader.css:18-22). GROUP-4 1:1 fix #1
/// removed the bogus 68×18 chip; the icon is now a bare 18×18 glyph.
pub const HEADER_ICON_SIZE: f32 = 18.0;

/// Action button footprint. Tauri `.panel-header__btn { width: 28px; height:
/// 28px }` (PanelHeader.css:59-60).
pub const HEADER_BTN_SIZE: f32 = 28.0;

/// Gap inside the actions group between the search and close buttons. Tauri
/// `.panel-header__actions { gap: var(--spacing-xs) }` = 4 (PanelHeader.css:51).
pub const HEADER_BTN_GAP: f32 = SPACING.xs;

/// Action-button corner radius. Tauri `.panel-header__btn { border-radius:
/// 6px }` (PanelHeader.css:61).
pub const HEADER_BTN_RADIUS: f32 = 6.0;

/// The 14×14 SVG glyph inside each action button. Tauri renders the search /
/// close icons at `width="14" height="14"` (PanelHeader.tsx:71,86).
pub const HEADER_BTN_GLYPH_SIZE: f32 = 14.0;

/// Divider line thickness in DIPs. 1-DIP hairline; the renderer paints
/// it as a filled rect at `rgba(255,255,255,0.05)` (E-04).
pub const DIVIDER_THICKNESS: f32 = 1.0;

/// Layout slots for the expanded zone body chrome (everything except the
/// item cards themselves — those stay sourced from
/// `highlight_overlay::item_card_rect_for_grid` so hit-test geometry
/// remains a single source of truth).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpandedZoneLayout {
    /// Drop-shadow band painted under the panel. Derived from
    /// `SHADOW.expanded` with the same `(offset, blur)` spread rule the
    /// settings panel uses, so the visual weight matches across surfaces.
    pub panel_shadow: Rect,
    /// The panel rect itself — equal to the zone's outer rectangle in DIPs.
    pub panel: Rect,
    /// Header band at the top of the panel — folder icon + title + count
    /// badge live inside this rect. Bottom edge of this rect is the divider.
    pub header_band: Rect,
    /// 1-DIP horizontal line separating the header band from the item grid.
    pub divider: Rect,
    /// 18×18 zone-icon glyph slot, vertically centred in the band, at the
    /// `HEADER_INSET_X` left padding. GROUP-4 fix #1 — no background chip.
    pub header_icon: Rect,
    /// Item-count badge slot. Mirrors Tauri `.panel-header__badge` — radius
    /// `--radius-badge`, accent/`badge_bg` fill, 11px/600 count text, INTRINSIC
    /// width (`padding 2px 9px` + count text). GROUP-4 fix #4 — placed BEFORE
    /// the action buttons (no longer flush-right against the band).
    pub header_badge: Rect,
    /// 28×28 search action button (Tauri `.panel-header__btn`). GROUP-4 fix #2 —
    /// previously MISSING; left of the close button with a 4px gap.
    pub header_search_btn: Rect,
    /// 28×28 close action button (Tauri `.panel-header__btn--close`). GROUP-4
    /// fix #2 — right-aligned at `panel.right() - HEADER_INSET_X`.
    pub header_close_btn: Rect,
}

/// Height of the header count badge slot in DIPs. Tauri `.panel-header__badge`
/// sizes intrinsically from `font-size: 11px; line-height: 1.4; padding: 2px`
/// → `11 × 1.4 + 2 + 2 ≈ 20`. GROUP-4 fix #4.
pub const HEADER_BADGE_HEIGHT: f32 = 20.0;

/// Horizontal padding inside the badge per side. Tauri `.panel-header__badge
/// { padding: 2px 9px }` (PanelHeader.css:43).
pub const HEADER_BADGE_PAD_X: f32 = 9.0;

/// Intrinsic width of the header count badge for `count`. Tauri sizes the
/// badge to its text plus `9px` L/R padding. Allocation-free (§10): the width
/// is a digit-count lookup (matching `format_small_count`'s 1–3 digit / "999+"
/// output) times the 11px advance estimate, not a per-frame DWrite measure.
#[inline]
pub fn header_badge_width_for_count(count: usize) -> f32 {
    let glyphs = if count < 10 {
        1
    } else if count < 100 {
        2
    } else if count < 1000 {
        3
    } else {
        // `format_small_count` caps at the 4-char "999+".
        4
    };
    // 0.62 advance-per-glyph at the 11px (`--font-size-xs`) badge font — the
    // same per-digit estimate the collapsed-pill badge uses
    // (`zone_pill_geometry::badge_width_for_count`), so the two badges read at
    // a consistent width. Plus 9px padding on each side (Tauri `2px 9px`).
    let per_glyph = TYPOGRAPHY.xs.size_px * 0.62;
    (glyphs as f32) * per_glyph + HEADER_BADGE_PAD_X * 2.0
}

/// Build the expanded zone layout for `zone`. Pure / allocation-free /
/// `Copy` output. Safe to call every frame.
#[inline]
pub fn expanded_zone_layout(zone: &Zone) -> ExpandedZoneLayout {
    let panel = Rect {
        x: zone.x as f32,
        y: zone.y as f32,
        width: zone.w as f32,
        height: zone.h as f32,
    };
    expanded_zone_layout_for_rect(panel, zone.items.len())
}

/// Like [`expanded_zone_layout`] but takes the panel rect + item count
/// directly. `count` drives the intrinsic badge width (GROUP-4 fix #4). Used
/// by tests and any caller that has already computed the zone rect.
#[inline]
pub fn expanded_zone_layout_for_rect(panel: Rect, count: usize) -> ExpandedZoneLayout {
    // M6b — `SHADOW.expanded` is a multi-layer `ShadowStack`; the band uses the
    // dominant outer layer (== pre-M6b single `SHADOW.expanded`). The expanded
    // zone panel is per-theme-keyed at the render call-site (`render.rs`) which
    // passes the active theme's stack; this layout helper keeps the global
    // baseline geometry for the (theme-agnostic) drop-band rect computation.
    let shadow = SHADOW.expanded.outer();
    let spread = shadow.blur.max(0.0);
    let panel_shadow = Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    };
    let header_band = Rect {
        x: panel.x + HEADER_INSET_X,
        y: panel.y,
        width: (panel.width - HEADER_INSET_X * 2.0).max(0.0),
        height: HEADER_BAND_HEIGHT.min(panel.height.max(0.0)),
    };
    let divider = Rect {
        x: panel.x + DIVIDER_INSET_X,
        y: panel.y + HEADER_BAND_HEIGHT - DIVIDER_THICKNESS,
        width: (panel.width - DIVIDER_INSET_X * 2.0).max(0.0),
        height: DIVIDER_THICKNESS,
    };
    let band_h = header_band.height;
    // Helper: a `size`×`size` square vertically centred in the band.
    let centred_y = |size: f32| header_band.y + ((band_h - size) * 0.5).max(0.0);

    // GROUP-4 fix #1 — zone icon glyph (18×18, no chip) at the left padding.
    let header_icon = Rect {
        x: header_band.x,
        y: centred_y(HEADER_ICON_SIZE),
        width: HEADER_ICON_SIZE.min(header_band.width),
        height: HEADER_ICON_SIZE.min(band_h),
    };

    // GROUP-4 fix #2 — the two 28×28 action buttons, right-aligned. Tauri flex
    // order is `… badge · actions[search, close]`; the close button is the
    // rightmost child at `panel.right() - 16`, the search button sits to its
    // left with a 4px gap.
    let btn_size = HEADER_BTN_SIZE.min(band_h);
    let btn_y = centred_y(btn_size);
    let header_close_btn = Rect {
        x: header_band.right() - btn_size,
        y: btn_y,
        width: btn_size,
        height: btn_size,
    };
    let header_search_btn = Rect {
        x: header_close_btn.x - HEADER_BTN_GAP - btn_size,
        y: btn_y,
        width: btn_size,
        height: btn_size,
    };

    // GROUP-4 fix #4 — intrinsic-width count badge, placed BEFORE the actions
    // group (8px gap to the search button), vertically centred in the band.
    // On a pathologically narrow panel the ideal `search_btn.x - gap - badge_w`
    // anchor would run off the left edge, so the x is clamped to the header
    // band's left padding — the flex row collapses gracefully (the title slot
    // above shrinks/clips to the now-leftward badge), the badge never runs off
    // the band's left edge, and `header_badge_width_for_count` already clamped
    // the width to the band so the badge can't overflow on the right either.
    let badge_h = HEADER_BADGE_HEIGHT.min(band_h);
    let badge_w = header_badge_width_for_count(count).min(header_band.width);
    let badge_x = (header_search_btn.x - HEADER_GAP - badge_w).max(header_band.x);
    let header_badge = Rect {
        x: badge_x,
        y: centred_y(badge_h),
        width: badge_w,
        height: badge_h,
    };

    ExpandedZoneLayout {
        panel_shadow,
        panel,
        header_band,
        divider,
        header_icon,
        header_badge,
        header_search_btn,
        header_close_btn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use bento_nano_zone::ZoneId;

    fn fixture(x: i32, y: i32, w: i32, h: i32) -> Zone {
        Zone::new(ZoneId(1), Cow::Borrowed("Compiler"), x, y, w, h)
    }

    #[test]
    fn panel_rect_matches_zone_rect() {
        let zone = fixture(120, 80, 260, 200);
        let layout = expanded_zone_layout(&zone);
        assert_eq!(layout.panel.x, 120.0);
        assert_eq!(layout.panel.y, 80.0);
        assert_eq!(layout.panel.width, 260.0);
        assert_eq!(layout.panel.height, 200.0);
    }

    #[test]
    fn panel_shadow_extends_beyond_panel_on_all_sides() {
        // SHADOW.expanded blur is 48 — the band must spread by that amount
        // on every side so the renderer paints the soft drop under the
        // panel rather than truncated at the panel edge.
        let zone = fixture(100, 100, 200, 150);
        let layout = expanded_zone_layout(&zone);
        assert!(layout.panel_shadow.x < layout.panel.x);
        assert!(layout.panel_shadow.y < layout.panel.y);
        assert!(layout.panel_shadow.right() > layout.panel.right());
        assert!(layout.panel_shadow.bottom() > layout.panel.bottom());
    }

    #[test]
    fn header_band_sits_above_divider() {
        let zone = fixture(0, 0, 240, 180);
        let layout = expanded_zone_layout(&zone);
        // Header bottom touches the divider band bottom — divider rests
        // on the seam between header and items.
        assert!((layout.header_band.bottom() - layout.divider.bottom()).abs() < 0.01);
        // Divider sits inside the header band's vertical span.
        assert!(layout.divider.y >= layout.header_band.y);
        assert!(layout.divider.y < layout.header_band.bottom());
    }

    #[test]
    fn divider_is_a_one_dip_hairline_spanning_full_header_width() {
        let zone = fixture(50, 50, 300, 200);
        let layout = expanded_zone_layout(&zone);
        assert_eq!(layout.divider.height, DIVIDER_THICKNESS);
        // GROUP-4 fix #5 — the divider is a CSS `border-bottom` on the
        // border-box and therefore spans the FULL header width edge-to-edge
        // (`DIVIDER_INSET_X == 0`), no 16-DIP gap at the ends.
        assert_eq!(DIVIDER_INSET_X, 0.0);
        assert_eq!(layout.divider.x, layout.panel.x);
        assert_eq!(layout.divider.width, layout.panel.width);
        let divider_center = layout.divider.x + layout.divider.width * 0.5;
        let panel_center = layout.panel.x + layout.panel.width * 0.5;
        assert!((divider_center - panel_center).abs() < 0.01);
    }

    #[test]
    fn header_badge_sits_before_the_action_buttons() {
        // GROUP-4 fix #4 — the count badge is NO LONGER flush-right; Tauri
        // flex order is `… badge · actions[search, close]`, so the badge's
        // right edge is 8px (HEADER_GAP) left of the search button's left edge.
        let zone = fixture(0, 0, 300, 200);
        let layout = expanded_zone_layout(&zone);
        assert!(
            (layout.header_badge.right()
                - (layout.header_search_btn.x - HEADER_GAP))
                .abs()
                < 0.01
        );
        // Intrinsic width — "Compiler" fixture has 0 items → 1-glyph count.
        assert!((layout.header_badge.width - header_badge_width_for_count(0)).abs() < 0.01);
        assert_eq!(layout.header_badge.height, HEADER_BADGE_HEIGHT);
        // Vertically centred inside the band.
        let badge_center = layout.header_badge.y + layout.header_badge.height * 0.5;
        let band_center = layout.header_band.y + layout.header_band.height * 0.5;
        assert!((badge_center - band_center).abs() < 0.01);
        // Stays inside the panel and clears the action buttons.
        assert!(layout.header_badge.right() <= layout.header_search_btn.x + 0.01);
        assert!(layout.header_badge.bottom() <= layout.header_band.bottom() + 0.01);
    }

    #[test]
    fn header_icon_is_an_18px_glyph_at_the_left_padding_no_chip() {
        // GROUP-4 fix #1 — bare 18×18 glyph (no 68×18 chip) at the 16-DIP
        // header padding, vertically centred in the 48-DIP band.
        let zone = fixture(0, 0, 300, 200);
        let layout = expanded_zone_layout(&zone);
        assert_eq!(layout.header_icon.x, layout.panel.x + HEADER_INSET_X);
        assert_eq!(layout.header_icon.width, HEADER_ICON_SIZE);
        assert_eq!(layout.header_icon.height, HEADER_ICON_SIZE);
        let icon_center = layout.header_icon.y + layout.header_icon.height * 0.5;
        let band_center = layout.header_band.y + layout.header_band.height * 0.5;
        assert!((icon_center - band_center).abs() < 0.01);
    }

    #[test]
    fn header_action_buttons_are_28sq_right_aligned_with_4px_gap() {
        // GROUP-4 fix #2 — search + close 28×28 buttons; close is rightmost at
        // `panel.right() - 16`, search 4px to its left.
        let zone = fixture(0, 0, 300, 200);
        let layout = expanded_zone_layout(&zone);
        assert_eq!(layout.header_close_btn.width, HEADER_BTN_SIZE);
        assert_eq!(layout.header_close_btn.height, HEADER_BTN_SIZE);
        assert_eq!(layout.header_search_btn.width, HEADER_BTN_SIZE);
        // Close button right edge sits at the header band right (panel - 16).
        assert!(
            (layout.header_close_btn.right() - layout.header_band.right()).abs() < 0.01
        );
        assert!(
            (layout.header_close_btn.right() - (layout.panel.right() - HEADER_INSET_X)).abs()
                < 0.01
        );
        // 4px gap between search and close.
        assert!(
            (layout.header_close_btn.x - layout.header_search_btn.right() - HEADER_BTN_GAP).abs()
                < 0.01
        );
        // Both vertically centred in the band.
        for btn in [layout.header_search_btn, layout.header_close_btn] {
            let c = btn.y + btn.height * 0.5;
            let band_center = layout.header_band.y + layout.header_band.height * 0.5;
            assert!((c - band_center).abs() < 0.01);
        }
    }

    #[test]
    fn header_badge_width_grows_with_count() {
        // §10 allocation-free intrinsic width — wider for more digits, capped
        // at the 4-glyph "999+" budget.
        let w1 = header_badge_width_for_count(7);
        let w2 = header_badge_width_for_count(42);
        let w3 = header_badge_width_for_count(500);
        let w4 = header_badge_width_for_count(99_999);
        assert!(w1 < w2 && w2 < w3 && w3 < w4);
        // 1-digit width = 1×(11×0.62) + 18 padding.
        assert!((w1 - (TYPOGRAPHY.xs.size_px * 0.62 + HEADER_BADGE_PAD_X * 2.0)).abs() < 0.01);
        // Cap: 100000 and 1000 both render "999+" → same 4-glyph width.
        assert_eq!(
            header_badge_width_for_count(1000),
            header_badge_width_for_count(99_999)
        );
    }

    #[test]
    fn header_badge_clamps_for_narrow_panels() {
        // A pathologically narrow panel — badge width must clamp to the
        // header band so it never overflows the panel edge.
        let zone = fixture(0, 0, 40, 200);
        let layout = expanded_zone_layout(&zone);
        assert!(layout.header_badge.width <= layout.header_band.width + 0.01);
        assert!(layout.header_badge.x >= layout.header_band.x - 0.01);
    }

    #[test]
    fn layout_is_copy_and_allocation_free() {
        // Spec §10 — `ExpandedZoneLayout: Copy` so the renderer can stash it
        // by value without lifetimes. This both asserts the trait bound and
        // documents the contract.
        fn assert_copy<T: Copy>() {}
        assert_copy::<ExpandedZoneLayout>();
    }

    #[test]
    fn header_band_height_is_tauri_48_and_bound_to_grid_top_ssot() {
        // M2③ (1:1): Tauri `.panel-header { height: 48px }` (PanelHeader.css:6).
        assert!((HEADER_BAND_HEIGHT - 48.0).abs() < 0.01);
        // The band height MUST equal the single grid-top offset SSoT so the
        // divider lands exactly on the seam where the first item row begins —
        // and so the renderer + both shell hit-tests can never drift (V-13).
        assert!(
            (HEADER_BAND_HEIGHT
                - crate::business::item_grid::ITEM_GRID_TOP_OFFSET_PX)
                .abs()
                < 0.01
        );
    }

    #[test]
    fn divider_sits_at_the_48_dip_header_seam() {
        // The divider's bottom edge is the header/grid seam — at panel.y + 48.
        let zone = fixture(0, 0, 240, 200);
        let layout = expanded_zone_layout(&zone);
        assert!((layout.divider.bottom() - (layout.panel.y + HEADER_BAND_HEIGHT)).abs() < 0.01);
    }

    #[test]
    fn header_band_clamped_to_panel_height() {
        // A pathological tiny panel — header_band height must clamp so the
        // helper never produces a header that overflows the panel.
        let zone = fixture(0, 0, 240, 10);
        let layout = expanded_zone_layout(&zone);
        assert!(layout.header_band.height <= layout.panel.height + 0.01);
    }
}
