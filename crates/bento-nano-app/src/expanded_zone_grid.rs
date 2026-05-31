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
use bento_nano_style::tokens::SHADOW;
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

/// Horizontal inset applied to the header band + count badge. Same 8-DIP
/// padding the existing zone chrome uses for the icon chip / accent stripe.
pub const HEADER_INSET_X: f32 = 8.0;

/// Horizontal inset of the divider line from the panel edges. M2 E-04
/// realigns this to Tauri's `--spacing-lg` (16) header padding, distinct
/// from the 8-DIP header content inset so the seam reads as Tauri's
/// `.panel-header` border-bottom without disturbing the (HELD) header
/// content geometry.
pub const DIVIDER_INSET_X: f32 = 16.0;

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
    /// Right-aligned item-count badge slot inside the header band (E-02).
    /// Mirrors Tauri `.panel-header__badge` — radius 10, accent/`badge_bg`
    /// fill, semibold count text. Renderer fills + labels this rect.
    pub header_badge: Rect,
}

/// Fixed width of the header count badge slot in DIPs. Wide enough for a
/// three-glyph count ("999+") at the 11px badge font; the renderer insets
/// the count text horizontally. Kept fixed so the layout stays `Copy` and
/// allocation-free (§10) — no per-frame text measurement.
pub const HEADER_BADGE_WIDTH: f32 = 34.0;

/// Height of the header count badge slot in DIPs.
pub const HEADER_BADGE_HEIGHT: f32 = 18.0;

/// Inset of the header count badge from the right edge of the header band.
pub const HEADER_BADGE_INSET_X: f32 = 0.0;

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
    expanded_zone_layout_for_rect(panel)
}

/// Like [`expanded_zone_layout`] but takes the panel rect directly. Used
/// by tests and any caller that has already computed the zone rect.
#[inline]
pub fn expanded_zone_layout_for_rect(panel: Rect) -> ExpandedZoneLayout {
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
    // Right-aligned within the header band, vertically centred in the band.
    let badge_w = HEADER_BADGE_WIDTH.min(header_band.width);
    let badge_h = HEADER_BADGE_HEIGHT.min(header_band.height);
    let header_badge = Rect {
        x: header_band.right() - HEADER_BADGE_INSET_X - badge_w,
        y: header_band.y + ((header_band.height - badge_h) * 0.5).max(0.0),
        width: badge_w,
        height: badge_h,
    };

    ExpandedZoneLayout {
        panel_shadow,
        panel,
        header_band,
        divider,
        header_badge,
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
    fn divider_is_a_one_dip_hairline_inset_horizontally() {
        let zone = fixture(50, 50, 300, 200);
        let layout = expanded_zone_layout(&zone);
        assert_eq!(layout.divider.height, DIVIDER_THICKNESS);
        // E-04 — divider sits at the wider `DIVIDER_INSET_X` (16) inset,
        // distinct from the 8-DIP header content inset, and stays centred.
        assert_eq!(layout.divider.x, layout.panel.x + DIVIDER_INSET_X);
        assert_eq!(
            layout.divider.width,
            (layout.panel.width - DIVIDER_INSET_X * 2.0).max(0.0)
        );
        let divider_center = layout.divider.x + layout.divider.width * 0.5;
        let panel_center = layout.panel.x + layout.panel.width * 0.5;
        assert!((divider_center - panel_center).abs() < 0.01);
    }

    #[test]
    fn header_badge_is_right_aligned_inside_header_band() {
        // E-02 count badge — sits flush with the header band's right edge,
        // vertically centred, at the locked badge size.
        let zone = fixture(0, 0, 300, 200);
        let layout = expanded_zone_layout(&zone);
        assert!(
            (layout.header_badge.right()
                - (layout.header_band.right() - HEADER_BADGE_INSET_X))
                .abs()
                < 0.01
        );
        assert_eq!(layout.header_badge.width, HEADER_BADGE_WIDTH);
        assert_eq!(layout.header_badge.height, HEADER_BADGE_HEIGHT);
        // Vertically centred inside the band.
        let badge_center = layout.header_badge.y + layout.header_badge.height * 0.5;
        let band_center = layout.header_band.y + layout.header_band.height * 0.5;
        assert!((badge_center - band_center).abs() < 0.01);
        // Stays inside the panel.
        assert!(layout.header_badge.right() <= layout.panel.right());
        assert!(layout.header_badge.bottom() <= layout.header_band.bottom() + 0.01);
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
