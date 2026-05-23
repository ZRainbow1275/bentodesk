//! Wave I2 (05-20 visual parity) — expanded zone body chrome geometry.
//!
//! Tauri 1.2.4 renders a clicked-open zone as a dark translucent rounded
//! panel with: a panel drop shadow, a header band (folder glyph + title +
//! status dot), a 1-DIP divider line between the header and the item grid,
//! a grid of item cards, and (optionally for stack anchors) a footer strip
//! of 16×16 sub-zone thumbnails. Geometry constants live here so the
//! renderer + future hit-test surfaces share one source of truth.
//!
//! The item-card rectangles themselves are **not** owned by this module —
//! they are sourced from
//! `bento_nano_app::business::highlight_overlay::item_card_rect_for_grid`
//! which is the existing SSoT shared with `hit_test_zone_item` in the
//! shell. Wave I2 only adds the panel chrome surrounding the items.
//!
//! Spec §3.2 100% self-rolled / spec §8 no new crate deps / spec §10 zero
//! allocation hot-path: every helper here returns `Copy` rects, no `Vec`,
//! no `String`. `footer_thumbs` is a fixed-N array with an explicit count.

use bento_nano_style::Rect;
use bento_nano_style::tokens::SHADOW;
use bento_nano_zone::Zone;

/// Maximum number of 16×16 footer thumbnail slots reserved in the layout.
/// frame_010 shows ~4 sub-zone icons in the bottom strip of an expanded
/// stack-anchor body; four covers the common case allocation-free.
pub const FOOTER_THUMB_CAP: usize = 4;

/// Header band height in DIPs — matches the +30 vertical offset that
/// `highlight_overlay::item_card_rect_for_grid` already uses to push the
/// item grid below the title row. Keeping these in lockstep means the
/// divider line lands exactly on the seam between header and items.
pub const HEADER_BAND_HEIGHT: f32 = 30.0;

/// Horizontal inset applied to header content + divider. Same 8-DIP
/// padding the existing zone chrome uses for the icon chip / accent stripe.
pub const HEADER_INSET_X: f32 = 8.0;

/// Divider line thickness in DIPs. 1-DIP hairline; the renderer paints
/// it as a filled rect at `with_alpha(palette.text, 0.10)`.
pub const DIVIDER_THICKNESS: f32 = 1.0;

/// Side length of the top-right status dot, mirroring the collapsed pill's
/// `PILL_STATUS_DOT_SIZE` so the chrome reads at the same scale across
/// the morph.
pub const STATUS_DOT_SIZE: f32 = 6.0;

/// Inset of the status dot from the top-right corner of the panel.
pub const STATUS_DOT_INSET: f32 = 8.0;

/// Side length of a single footer thumbnail (Tauri reference: 16 DIPs).
pub const FOOTER_THUMB_SIZE: f32 = 16.0;

/// Vertical inset of the footer strip from the panel bottom edge.
pub const FOOTER_INSET_Y: f32 = 8.0;

/// Horizontal inset of the leftmost footer thumbnail from the panel edge.
pub const FOOTER_INSET_X: f32 = 8.0;

/// Gap between adjacent footer thumbnails in DIPs.
pub const FOOTER_THUMB_GAP: f32 = 6.0;

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
    /// Header band at the top of the panel — folder icon + title + status
    /// dot live inside this rect. Bottom edge of this rect is the divider.
    pub header_band: Rect,
    /// 1-DIP horizontal line separating the header band from the item grid.
    pub divider: Rect,
    /// Top-right status dot rectangle. Renderer paints it only when
    /// `zone.items.len() > 0` so empty zones stay calm.
    pub status_dot: Rect,
    /// Reserved slots for 16×16 sub-zone thumbnails in the bottom strip.
    /// `footer_thumb_count` indicates how many of these are valid.
    pub footer_thumbs: [Rect; FOOTER_THUMB_CAP],
    /// Number of valid entries in `footer_thumbs` (0..=FOOTER_THUMB_CAP).
    pub footer_thumb_count: usize,
}

/// Build the expanded zone layout for `zone` with `footer_thumb_count`
/// requested sub-zone thumbnails (e.g. the stack anchor's member count).
/// Pure / allocation-free / `Copy` output. Safe to call every frame.
#[inline]
pub fn expanded_zone_layout(zone: &Zone, footer_thumb_count: usize) -> ExpandedZoneLayout {
    let panel = Rect {
        x: zone.x as f32,
        y: zone.y as f32,
        width: zone.w as f32,
        height: zone.h as f32,
    };
    expanded_zone_layout_for_rect(panel, footer_thumb_count)
}

/// Like [`expanded_zone_layout`] but takes the panel rect directly. Used
/// by tests and any caller that has already computed the zone rect.
#[inline]
pub fn expanded_zone_layout_for_rect(
    panel: Rect,
    footer_thumb_count: usize,
) -> ExpandedZoneLayout {
    let shadow = SHADOW.expanded;
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
        x: panel.x + HEADER_INSET_X,
        y: panel.y + HEADER_BAND_HEIGHT - DIVIDER_THICKNESS,
        width: (panel.width - HEADER_INSET_X * 2.0).max(0.0),
        height: DIVIDER_THICKNESS,
    };
    let status_dot = Rect {
        x: panel.right() - STATUS_DOT_INSET - STATUS_DOT_SIZE,
        y: panel.y + STATUS_DOT_INSET,
        width: STATUS_DOT_SIZE,
        height: STATUS_DOT_SIZE,
    };

    let mut footer_thumbs = [Rect::ZERO; FOOTER_THUMB_CAP];
    let clamped_count = footer_thumb_count.min(FOOTER_THUMB_CAP);
    let footer_y = panel.bottom() - FOOTER_INSET_Y - FOOTER_THUMB_SIZE;
    // Suppress the footer if the panel is too short to clear the divider.
    let footer_clears_divider = footer_y > divider.bottom() + 2.0;
    if footer_clears_divider {
        for (i, slot) in footer_thumbs.iter_mut().take(clamped_count).enumerate() {
            *slot = Rect {
                x: panel.x
                    + FOOTER_INSET_X
                    + (i as f32) * (FOOTER_THUMB_SIZE + FOOTER_THUMB_GAP),
                y: footer_y,
                width: FOOTER_THUMB_SIZE,
                height: FOOTER_THUMB_SIZE,
            };
        }
    }
    let effective_count = if footer_clears_divider {
        clamped_count
    } else {
        0
    };

    ExpandedZoneLayout {
        panel_shadow,
        panel,
        header_band,
        divider,
        status_dot,
        footer_thumbs,
        footer_thumb_count: effective_count,
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
        let layout = expanded_zone_layout(&zone, 0);
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
        let layout = expanded_zone_layout(&zone, 0);
        assert!(layout.panel_shadow.x < layout.panel.x);
        assert!(layout.panel_shadow.y < layout.panel.y);
        assert!(layout.panel_shadow.right() > layout.panel.right());
        assert!(layout.panel_shadow.bottom() > layout.panel.bottom());
    }

    #[test]
    fn header_band_sits_above_divider() {
        let zone = fixture(0, 0, 240, 180);
        let layout = expanded_zone_layout(&zone, 0);
        // Header bottom touches the divider band bottom — divider rests
        // on the seam between header and items.
        assert!((layout.header_band.bottom() - layout.divider.bottom()).abs() < 0.01);
        // Divider sits inside the header band's vertical span.
        assert!(layout.divider.y >= layout.header_band.y);
        assert!(layout.divider.y < layout.header_band.bottom());
    }

    #[test]
    fn divider_is_a_one_dip_hairline_centered_horizontally() {
        let zone = fixture(50, 50, 300, 200);
        let layout = expanded_zone_layout(&zone, 0);
        assert_eq!(layout.divider.height, DIVIDER_THICKNESS);
        // Same horizontal inset as the header.
        assert_eq!(layout.divider.x, layout.header_band.x);
        assert_eq!(layout.divider.width, layout.header_band.width);
    }

    #[test]
    fn status_dot_sits_at_top_right_inside_panel() {
        let zone = fixture(0, 0, 240, 180);
        let layout = expanded_zone_layout(&zone, 0);
        assert!(layout.status_dot.right() <= layout.panel.right());
        assert!(layout.status_dot.y >= layout.panel.y);
        assert!(layout.status_dot.y < layout.header_band.bottom());
        // Sits flush with the right inset.
        assert!(
            (layout.status_dot.right() - (layout.panel.right() - STATUS_DOT_INSET)).abs()
                < 0.01
        );
    }

    #[test]
    fn status_dot_is_square_at_token_size() {
        let zone = fixture(0, 0, 240, 180);
        let layout = expanded_zone_layout(&zone, 0);
        assert_eq!(layout.status_dot.width, STATUS_DOT_SIZE);
        assert_eq!(layout.status_dot.height, STATUS_DOT_SIZE);
    }

    #[test]
    fn footer_thumbs_lay_in_a_row_at_panel_bottom() {
        let zone = fixture(0, 0, 240, 200);
        let layout = expanded_zone_layout(&zone, 3);
        assert_eq!(layout.footer_thumb_count, 3);
        // All thumbs share the same y row and sit inside the panel.
        let first_y = layout.footer_thumbs[0].y;
        for slot in &layout.footer_thumbs[..layout.footer_thumb_count] {
            assert_eq!(slot.y, first_y);
            assert_eq!(slot.width, FOOTER_THUMB_SIZE);
            assert_eq!(slot.height, FOOTER_THUMB_SIZE);
            assert!(slot.bottom() <= layout.panel.bottom());
            assert!(slot.x >= layout.panel.x);
        }
        // Strict spacing between adjacent thumbs.
        let dx = layout.footer_thumbs[1].x - layout.footer_thumbs[0].x;
        assert!((dx - (FOOTER_THUMB_SIZE + FOOTER_THUMB_GAP)).abs() < 0.01);
    }

    #[test]
    fn footer_thumbs_clamp_to_cap() {
        let zone = fixture(0, 0, 240, 200);
        // Asking for more than the cap saturates to FOOTER_THUMB_CAP.
        let layout = expanded_zone_layout(&zone, FOOTER_THUMB_CAP + 99);
        assert_eq!(layout.footer_thumb_count, FOOTER_THUMB_CAP);
    }

    #[test]
    fn footer_thumbs_zero_when_panel_too_short() {
        // Panel barely taller than the header band — no room for footer.
        let zone = fixture(0, 0, 240, (HEADER_BAND_HEIGHT as i32) + 8);
        let layout = expanded_zone_layout(&zone, 3);
        assert_eq!(layout.footer_thumb_count, 0);
    }

    #[test]
    fn footer_thumbs_count_zero_leaves_slots_zeroed() {
        let zone = fixture(0, 0, 240, 200);
        let layout = expanded_zone_layout(&zone, 0);
        assert_eq!(layout.footer_thumb_count, 0);
        // All four slots are Rect::ZERO so accidental paint loops are no-ops.
        for slot in &layout.footer_thumbs {
            assert_eq!(*slot, Rect::ZERO);
        }
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
    fn header_band_clamped_to_panel_height() {
        // A pathological tiny panel — header_band height must clamp so the
        // helper never produces a header that overflows the panel.
        let zone = fixture(0, 0, 240, 10);
        let layout = expanded_zone_layout(&zone, 0);
        assert!(layout.header_band.height <= layout.panel.height + 0.01);
    }
}
