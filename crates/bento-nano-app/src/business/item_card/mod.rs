//! Business surface — `ItemCard`, a single tile inside an `ItemGrid`.
//!
//! Visual spec: see `item_card.snap.md`. Two variants (`Standard`,
//! `Wide`) drive both layout direction and column span. The
//! `display_name` helper is the locked port of 1.x `displayName`
//! (strip `.lnk` / `.url` for shortcut files) and is exercised by tests.
//!
//! Status: scaffolding per Wave E Option-A. `build()` returns a typed
//! Container with the locked geometry; the inner ItemIcon + name +
//! missing-badge composition lands when widget-library ships FileIcon
//! and the Tooltip primitive surface lands. NOT a `todo!()` stub.

use bento_nano_layout::Direction;
use bento_nano_style::{BorderRadius, Color, Edges, Length};
use bento_nano_theme::{PaletteTokens, RadiusTokens, radius};
use bento_nano_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use super::item_grid::{ITEM_GRID_ROW_HEIGHT_PX, column_span_for};

/// Card layout variant — locked wire-format. `Standard` is the default;
/// `Wide` spans two grid columns and lays out icon-then-name horizontally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CardVariant {
    #[default]
    Standard,
    Wide,
}

/// D2D ItemCard chrome derived from the active theme palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemCardChrome {
    /// Item card and floating ghost radius.
    pub card_radius: BorderRadius,
    /// Normal card fill for a present item.
    pub normal_background: Color,
    /// Muted fill while the real item is the drag source.
    pub drag_source_background: Color,
    /// Floating drag ghost card fill.
    pub ghost_background: Color,
    /// Shadow behind the floating drag ghost.
    pub ghost_shadow: Color,
    /// Destructive fill for items whose backing file is missing.
    pub missing_background: Color,
    /// Primary item label text.
    pub text: Color,
    /// Icon glyph text.
    pub icon_text: Color,
}

impl ItemCardChrome {
    /// Build ItemCard chrome from explicit active palette tokens.
    pub fn from_palette(palette: PaletteTokens) -> Self {
        // Dark-default surface_subtle / text_secondary so callers that only
        // have a `PaletteTokens` keep the pre-M6a byte-exact dark card.
        use bento_nano_style::tokens::PALETTE_DARK;
        Self::from_tokens(
            palette,
            radius::DEFAULT,
            PALETTE_DARK.surface_subtle,
            PALETTE_DARK.text_secondary,
        )
    }

    /// Build ItemCard chrome from explicit active theme token groups.
    ///
    /// M2 E-03 (2026-05-29) — corrected to Tauri `ItemCard.css` 1:1.
    /// Radius is `--radius-card` = 10 (was `radius.md` = 6); normal bg is
    /// `--surface-subtle` = `rgba(255,255,255,0.03)` (was the warm/opaque
    /// `surface_alt @0.46`); name text is `--text-secondary` = `#c0c0cc`
    /// (was `text @0.82`); missing bg is softened toward Tauri's
    /// `rgba(239,68,68,0.08)` (was `danger @0.55`, far too strong).
    ///
    /// M6a (2026-05-29) — `surface_subtle` (normal card fill) and
    /// `text_secondary` (card name text) now arrive as explicit args from the
    /// renderer's live `PaletteTauri` (`pal.surface_subtle` / `pal.text_secondary`)
    /// so the card re-skins with the active theme. The dark-default values
    /// reproduce the prior static `PALETTE_DARK` bytes 1:1 (cfg(test) callers
    /// pass them explicitly to lock byte-parity). The leaf crate stays free of
    /// any theme dependency — these are plain `Color`s.
    pub fn from_tokens(
        palette: PaletteTokens,
        _radius: RadiusTokens,
        surface_subtle: Color,
        text_secondary: Color,
    ) -> Self {
        use bento_nano_style::tokens::RADIUS;
        Self {
            card_radius: BorderRadius::all(RADIUS.card),
            normal_background: surface_subtle,
            drag_source_background: with_alpha(palette.surface_alt, 0.18),
            ghost_background: with_alpha(palette.surface, 0.86),
            ghost_shadow: with_alpha(palette.scrim, 0.24),
            missing_background: with_alpha(palette.danger, 0.10),
            text: text_secondary,
            icon_text: with_alpha(palette.text, 0.94),
        }
    }
}

impl CardVariant {
    /// `is_wide` toggle in the 1.x prop shape. Kept as a method so the
    /// composition layer can fan out without touching the enum directly.
    pub const fn is_wide(self) -> bool {
        matches!(self, Self::Wide)
    }

    /// How many ItemGrid columns this card occupies — defers to
    /// `item_grid::column_span_for` so both surfaces stay in lockstep.
    pub const fn column_span(self) -> u32 {
        column_span_for(self.is_wide())
    }

    /// Card outer height in logical px. Always equal to one grid row so the
    /// rendered card pixel-aligns with the parent grid.
    pub const fn height_px(self) -> f32 {
        ITEM_GRID_ROW_HEIGHT_PX
    }

    /// Card layout direction.
    pub const fn direction(self) -> Direction {
        match self {
            Self::Standard => Direction::Column,
            Self::Wide => Direction::Row,
        }
    }
}

/// Strip trailing `.lnk` / `.url` from a shortcut file's display label.
/// Mirrors the 1.x `displayName` helper exactly. Match is ASCII-case-
/// insensitive on the four-character suffix; the on-disk name is never
/// mutated upstream.
pub fn display_name(name: &str) -> SmolStr {
    let len = name.len();
    if len < 4 {
        return SmolStr::new(name);
    }

    let suffix = &name.as_bytes()[len - 4..];
    let is_lnk = suffix.eq_ignore_ascii_case(b".lnk");
    let is_url = suffix.eq_ignore_ascii_case(b".url");
    if is_lnk || is_url {
        SmolStr::new(&name[..len - 4])
    } else {
        SmolStr::new(name)
    }
}

/// Build the card container at the default (Standard) variant.
pub fn build() -> WidgetNode {
    build_with(CardVariant::default())
}

/// Build the card container for an explicit variant. Locked geometry per
/// `item_card.snap.md`; inner children land when widget-library ships
/// `FileIcon` + `Tooltip`.
pub fn build_with(variant: CardVariant) -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: variant.direction(),
        width: Length::Auto,
        height: Length::Px(variant.height_px()),
        padding: match variant {
            CardVariant::Standard => Edges::all(6.0),
            CardVariant::Wide => Edges {
                top: 6.0,
                right: 8.0,
                bottom: 6.0,
                left: 8.0,
            },
        },
        ..ContainerNode::default()
    })
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;

    #[test]
    fn variant_is_wide() {
        assert!(!CardVariant::Standard.is_wide());
        assert!(CardVariant::Wide.is_wide());
    }

    #[test]
    fn variant_column_span_matches_grid_helper() {
        assert_eq!(CardVariant::Standard.column_span(), 1);
        assert_eq!(CardVariant::Wide.column_span(), 2);
    }

    #[test]
    fn variant_height_matches_grid_row() {
        assert!((CardVariant::Standard.height_px() - ITEM_GRID_ROW_HEIGHT_PX).abs() < 0.01);
        assert!((CardVariant::Wide.height_px() - ITEM_GRID_ROW_HEIGHT_PX).abs() < 0.01);
    }

    #[test]
    fn variant_direction_per_snap_md() {
        assert_eq!(CardVariant::Standard.direction(), Direction::Column);
        assert_eq!(CardVariant::Wide.direction(), Direction::Row);
    }

    #[test]
    fn display_name_strips_lnk_and_url_case_insensitive() {
        assert_eq!(display_name("Notes.lnk"), "Notes");
        assert_eq!(display_name("Notes.LNK"), "Notes");
        assert_eq!(display_name("Bookmark.URL"), "Bookmark");
        assert_eq!(display_name("Bookmark.url"), "Bookmark");
    }

    #[test]
    fn display_name_preserves_other_extensions() {
        assert_eq!(display_name("photo.png"), "photo.png");
        assert_eq!(display_name("readme.md"), "readme.md");
    }

    #[test]
    fn display_name_handles_short_and_empty() {
        assert_eq!(display_name(""), "");
        assert_eq!(display_name("a"), "a");
        assert_eq!(display_name(".md"), ".md"); // 3 chars, untouched
    }

    #[test]
    fn build_standard_is_column_oriented_and_row_height() {
        let node = build();
        let layout = node.layout();
        assert_eq!(layout.direction, Direction::Column);
        assert!(
            matches!(layout.height, Length::Px(h) if (h - ITEM_GRID_ROW_HEIGHT_PX).abs() < 0.01)
        );
    }

    #[test]
    fn build_wide_is_row_oriented() {
        let node = build_with(CardVariant::Wide);
        let layout = node.layout();
        assert_eq!(layout.direction, Direction::Row);
    }

    #[test]
    fn card_variant_serde_round_trip() {
        for v in [CardVariant::Standard, CardVariant::Wide] {
            let s = serde_json::to_string(&v).unwrap_or_default();
            let back: CardVariant = serde_json::from_str(&s).unwrap_or_default();
            assert_eq!(v, back);
        }
        assert_eq!(
            serde_json::to_string(&CardVariant::Wide).unwrap_or_default(),
            "\"wide\""
        );
    }

    #[test]
    fn item_card_chrome_accepts_explicit_active_palette() {
        let palette = PaletteTokens {
            bg: Color::from_u8(0x01, 0x02, 0x03, 0xFF),
            surface: Color::from_u8(0x11, 0x12, 0x13, 0xFF),
            surface_alt: Color::from_u8(0x21, 0x22, 0x23, 0xFF),
            border: Color::from_u8(0x31, 0x32, 0x33, 0xFF),
            text: Color::from_u8(0x41, 0x42, 0x43, 0xFF),
            text_muted: Color::from_u8(0x51, 0x52, 0x53, 0xFF),
            accent: Color::from_u8(0x61, 0x62, 0x63, 0xFF),
            accent_hover: Color::from_u8(0x71, 0x72, 0x73, 0xFF),
            danger: Color::from_u8(0x81, 0x82, 0x83, 0xFF),
            success: Color::from_u8(0x91, 0x92, 0x93, 0xFF),
            warning: Color::from_u8(0xA1, 0xA2, 0xA3, 0xFF),
            info: Color::from_u8(0xB1, 0xB2, 0xB3, 0xFF),
            scrim: Color::from_u8(0xC1, 0xC2, 0xC3, 0xFF),
            hover_overlay: Color::from_u8(0xD1, 0xD2, 0xD3, 0xFF),
            active_overlay: Color::from_u8(0xE1, 0xE2, 0xE3, 0xFF),
            selection: Color::from_u8(0xF1, 0xF2, 0xF3, 0xFF),
        };

        let chrome = ItemCardChrome::from_palette(palette);

        // M2 E-03 — card radius is the Tauri `--radius-card` (10), NOT the
        // live `radius.md` (6).
        assert_eq!(
            chrome.card_radius,
            BorderRadius::all(bento_nano_style::tokens::RADIUS.card)
        );
        // Normal bg is the Tauri `--surface-subtle` (white @ 0.03), not the
        // warm `surface_alt @ 0.46`.
        assert_eq!(
            chrome.normal_background,
            bento_nano_style::tokens::PALETTE_DARK.surface_subtle
        );
        assert_eq!(
            chrome.drag_source_background,
            with_alpha(palette.surface_alt, 0.18)
        );
        assert_eq!(chrome.ghost_background, with_alpha(palette.surface, 0.86));
        assert_eq!(chrome.ghost_shadow, with_alpha(palette.scrim, 0.24));
        // Missing fill softened toward Tauri `rgba(239,68,68,0.08)`.
        assert_eq!(chrome.missing_background, with_alpha(palette.danger, 0.10));
        // Name text is the Tauri `--text-secondary` (#c0c0cc).
        assert_eq!(
            chrome.text,
            bento_nano_style::tokens::PALETTE_DARK.text_secondary
        );
        assert_eq!(chrome.icon_text, with_alpha(palette.text, 0.94));
    }

    #[test]
    fn item_card_chrome_uses_tauri_card_radius_token() {
        // E-03 — `card_radius` is pinned to the static Tauri `--radius-card`
        // (10) regardless of the passed live `radius.md`, so the card corner
        // matches the reference exactly.
        let palette = bento_nano_theme::current().palette;
        let radius = RadiusTokens {
            sm: BorderRadius::all(3.0),
            md: BorderRadius::all(7.0),
            lg: BorderRadius::all(11.0),
            xl: BorderRadius::all(17.0),
            full: BorderRadius::all(999.0),
        };

        let chrome = ItemCardChrome::from_tokens(
            palette,
            radius,
            bento_nano_style::tokens::PALETTE_DARK.surface_subtle,
            bento_nano_style::tokens::PALETTE_DARK.text_secondary,
        );

        assert_eq!(
            chrome.card_radius,
            BorderRadius::all(bento_nano_style::tokens::RADIUS.card)
        );
    }
}
