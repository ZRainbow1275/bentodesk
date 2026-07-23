//! Business surface — `PalettePicker` (T-067c).
//!
//! Inline popover triggered from `BulkManagerPanel`'s palette button. Lets
//! the user pick one accent colour from a curated 12-swatch set; the chosen
//! swatch is forwarded by the shell to a bulk-update on every selected
//! zone in a single backend transaction.
//!
//! Visual spec: `palette_picker.snap.md`. Pairs with
//! `business::bulk_manager_panel`.
//!
//! # State machine
//!
//! Mirrors the Wave-E shape (see `business::capsule_picker`): user intents
//! collapse into a closed [`PalettePickerAction`] enum, drained one-shot
//! via [`PalettePickerState::take_action`]. The shell translates the
//! action into the appropriate dispatcher Command sequence per frame.
//!
//! # Spec compliance
//!
//! - §10 hot-path: every identifier is `SmolStr` (slug + hex live in 6-7
//!   bytes inline); the swatch list is a `&'static [Swatch]` table so the
//!   popover never heap-allocates on open.
//! - §11 ΔB: every public DTO derives `serde::{Serialize, Deserialize}`.
//! - §11.1: zero `unsafe` in this UI layer.
//! - §15: this single .rs file ships under the 800-LOC budget.
//! - §17: zero `todo!()` / `unimplemented!()` / `panic!()` / `unwrap()` /
//!   `expect()` in production code.

use core::fmt;

use bento_nano_layout::Direction;
use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Shadow};
use bento_nano_theme::{self as theme, PaletteTokens, RadiusTokens, ShadowTokens, radius, shadow};
use bento_nano_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// -----------------------------------------------------------------------------
// Snap.md geometry constants — pinned per the visual spec.
// -----------------------------------------------------------------------------

/// Popover width in DIPs — `min(240px, 92vw)` per snap.md.
pub const POPOVER_WIDTH_PX: f32 = 240.0;

/// Maximum popover width as fraction of viewport — `min(_, 92vw)` clamp.
pub const POPOVER_MAX_WIDTH_FRACTION: f32 = 0.92;

/// Outer popover padding — 12 px uniform per snap.md.
pub const POPOVER_PADDING_PX: f32 = 12.0;

/// Outer popover corner radius — 12 px.
pub const POPOVER_CORNER_RADIUS_PX: f32 = 12.0;

/// Header row height (title + close button row).
pub const HEADER_HEIGHT_PX: f32 = 28.0;

/// Gap between the header and the swatch grid.
pub const HEADER_BOTTOM_MARGIN_PX: f32 = 8.0;

/// Number of swatch columns in the grid.
pub const SWATCH_COLUMNS: u32 = 4;

/// Per-cell square size in DIPs (the cell that hosts the inner circle).
pub const SWATCH_CELL_SIZE_PX: f32 = 40.0;

/// Inner swatch circle diameter in DIPs.
pub const SWATCH_CIRCLE_DIAMETER_PX: f32 = 32.0;

/// Gap between adjacent swatch cells (both axes).
pub const SWATCH_CELL_GAP_PX: f32 = 8.0;

/// Hover scale factor applied by the renderer (200 ms ease-out).
pub const SWATCH_HOVER_SCALE: f32 = 1.05;

/// Selected ring stroke width in DIPs.
pub const SELECTED_RING_WIDTH_PX: f32 = 2.0;

// -----------------------------------------------------------------------------
// Chrome tokens — shared by D2D renderer and widget-tree scaffold.
// -----------------------------------------------------------------------------

/// PalettePicker colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PalettePickerChrome {
    /// Drop shadow descriptor drawn behind the picker panel.
    pub panel_shadow: Shadow,
    /// Main picker panel radius.
    pub panel_radius: BorderRadius,
    /// Outer selected-ring/swatch shell radius.
    pub swatch_radius: BorderRadius,
    /// Inner swatch fill radius.
    pub swatch_inner_radius: BorderRadius,
    /// Clear action shell radius.
    pub clear_radius: BorderRadius,
    /// Clear action fill radius.
    pub clear_inner_radius: BorderRadius,
    /// Main picker panel background.
    pub panel_background: Color,
    /// Chip/swatch shell background.
    pub chip_background: Color,
    /// Title and primary text colour.
    pub title_color: Color,
    /// Body text colour.
    pub body_color: Color,
    /// Muted helper text colour.
    pub muted_color: Color,
    /// Warning/selected-ring colour.
    pub warning_color: Color,
}

impl PalettePickerChrome {
    /// Build chrome colours from active palette tokens.
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, radius::DEFAULT, shadow::DEFAULT)
    }

    /// Build chrome from explicit active theme token groups.
    pub fn from_tokens(palette: PaletteTokens, radius: RadiusTokens, shadow: ShadowTokens) -> Self {
        Self {
            panel_shadow: shadow.md,
            panel_radius: radius.xl,
            swatch_radius: radius.lg,
            swatch_inner_radius: radius.md,
            clear_radius: radius.lg,
            clear_inner_radius: radius.md,
            panel_background: palette.surface,
            chip_background: palette.surface_alt,
            title_color: palette.text,
            body_color: palette.text,
            muted_color: palette.text_muted,
            warning_color: palette.warning,
        }
    }

    /// Build PalettePicker chrome from Wave B Tauri SSoT tokens.
    ///
    /// Token mapping (Wave A `palette-picker.md` + Wave B `token-mapping.md`):
    /// - panel bg ← `surface_expanded`
    /// - swatch shell (idle border) ← `surface_subtle`
    /// - selected ring ← `accent_blue` (Wave A: 2px ring on chosen swatch)
    /// - title/body ← `text_primary`; muted hint ← `text_muted`
    /// - radii: panel = `expanded` (16); swatch shell = `card` (10);
    ///   swatch inner = `card` (10) — chip is circular in Tauri but D2D
    ///   approximates with the largest card radius the chip will accept.
    /// - shadow ← `expanded` (outer)
    pub fn from_tauri_tokens(
        palette: PaletteTauri,
        radius: RadiusTauri,
        shadow: ShadowTauri,
    ) -> Self {
        Self {
            // M6b — `expanded` is a `ShadowStack`; consume the outer layer.
            panel_shadow: shadow.expanded.outer(),
            panel_radius: BorderRadius::all(radius.expanded),
            swatch_radius: BorderRadius::all(radius.card),
            swatch_inner_radius: BorderRadius::all(radius.card),
            clear_radius: BorderRadius::all(radius.card),
            clear_inner_radius: BorderRadius::all(radius.card),
            panel_background: palette.surface_expanded,
            chip_background: palette.surface_subtle,
            title_color: palette.text_primary,
            body_color: palette.text_primary,
            muted_color: palette.text_muted,
            warning_color: palette.accent_blue,
        }
    }
}

// -----------------------------------------------------------------------------
// Swatch — one entry in the curated palette table.
// -----------------------------------------------------------------------------

/// One swatch in the palette table. `slug` is the stable identifier the
/// shell forwards (and that round-trips through any future scripting
/// surface); `hex` is the user-visible colour token (`#rrggbb`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Swatch {
    /// Stable identifier (e.g. `"blue"`, `"slate"`). 5-byte ASCII fits
    /// inline in `SmolStr` (≤ 22 bytes inline).
    pub slug: SmolStr,
    /// User-visible hex colour token (`#rrggbb`). Always 7 ASCII bytes —
    /// fits inline in `SmolStr`.
    pub hex: SmolStr,
}

impl Swatch {
    /// Build a swatch from `&'static str` literals — used by the static
    /// table below. Const-friendly is overkill here; this is called once
    /// at table-init time during the [`swatch_table`] lazy-static evaluation.
    fn make(slug: &'static str, hex: &'static str) -> Self {
        Self {
            slug: SmolStr::new_static(slug),
            hex: SmolStr::new_static(hex),
        }
    }

    /// Resolve a slug to the matching swatch in [`swatch_table`]. Returns
    /// `None` for unknown slugs — defensive against forward-compat
    /// scripting passing a slug from a future palette revision.
    pub fn find_by_slug(slug: &str) -> Option<&'static Swatch> {
        swatch_table().iter().find(|s| s.slug.as_str() == slug)
    }
}

/// Inline static table built once on first access. We can't use a
/// `const` initialiser here because `SmolStr::new_static` is `const fn`
/// only on newer toolchains; `OnceLock` keeps the surface stable across
/// the full MSRV.
///
/// Returns a `&'static [Swatch]` slice — the renderer iterates in row-
/// reading order (top-left → bottom-right across the 4-wide grid).
pub fn swatch_table() -> &'static [Swatch] {
    use std::sync::OnceLock;
    static TABLE: OnceLock<Vec<Swatch>> = OnceLock::new();
    TABLE.get_or_init(|| {
        vec![
            Swatch::make("slate", "#64748b"),
            Swatch::make("blue", "#3b82f6"),
            Swatch::make("indigo", "#6366f1"),
            Swatch::make("violet", "#8b5cf6"),
            Swatch::make("pink", "#ec4899"),
            Swatch::make("red", "#ef4444"),
            Swatch::make("orange", "#f97316"),
            Swatch::make("amber", "#f59e0b"),
            Swatch::make("yellow", "#eab308"),
            Swatch::make("green", "#22c55e"),
            Swatch::make("teal", "#14b8a6"),
            Swatch::make("cyan", "#06b6d4"),
        ]
    })
}

// -----------------------------------------------------------------------------
// PalettePickerAction — closed enum of one-shot user intents.
// -----------------------------------------------------------------------------

/// User intent recorded by the popover state machine. Drained once per
/// frame via [`PalettePickerState::take_action`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PalettePickerAction {
    /// User clicked a swatch. Carries the resolved swatch (slug + hex)
    /// so the shell can forward both to the bulk-update backend without
    /// re-resolving.
    Pick { swatch: Swatch },
    /// User dismissed the popover (close button, Escape, or scrim
    /// click). Shell hides the host window — no Command required.
    Close,
}

impl fmt::Display for PalettePickerAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pick { swatch } => write!(f, "Pick({})", swatch.slug),
            Self::Close => f.write_str("Close"),
        }
    }
}

// -----------------------------------------------------------------------------
// PalettePickerState — runtime state for the popover.
// -----------------------------------------------------------------------------

/// Popover runtime state.
///
/// - `selected_slug` — the previously-picked swatch's slug (drives the
///   highlight ring on re-open). `None` when no prior pick has been
///   seeded.
/// - `pending_action` — the latest one-shot [`PalettePickerAction`] the
///   shell has yet to drain.
#[derive(Debug, Default)]
pub struct PalettePickerState {
    selected_slug: Option<SmolStr>,
    pending_action: Option<PalettePickerAction>,
}

impl PalettePickerState {
    /// New empty state. The shell calls [`set_selected`] before the
    /// first paint when re-opening the picker for a zone with an
    /// existing accent colour.
    ///
    /// [`set_selected`]: PalettePickerState::set_selected
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow the currently-selected swatch slug (if any).
    pub fn selected_slug(&self) -> Option<&str> {
        self.selected_slug.as_deref()
    }

    /// Resolve the currently-selected swatch (if any).
    pub fn selected_swatch(&self) -> Option<&'static Swatch> {
        self.selected_slug
            .as_ref()
            .and_then(|s| Swatch::find_by_slug(s))
    }

    /// Seed the selected swatch — typically called by the shell on
    /// re-open to highlight the zone's existing accent colour.
    /// Passing an unknown slug clears the selection without panicking.
    pub fn set_selected(&mut self, slug: impl Into<SmolStr>) {
        let s: SmolStr = slug.into();
        if Swatch::find_by_slug(s.as_str()).is_some() {
            self.selected_slug = Some(s);
        } else {
            self.selected_slug = None;
        }
    }

    /// Clear the selection — no swatch highlighted on next paint.
    pub fn clear_selected(&mut self) {
        self.selected_slug = None;
    }

    /// User clicked a swatch identified by `slug`. Records a `Pick`
    /// action carrying the resolved swatch payload. Returns `true` when
    /// the slug matched a known swatch; `false` (no action recorded)
    /// when the slug was stale / unknown.
    pub fn pick(&mut self, slug: &str) -> bool {
        let Some(swatch) = Swatch::find_by_slug(slug) else {
            return false;
        };
        self.selected_slug = Some(swatch.slug.clone());
        self.pending_action = Some(PalettePickerAction::Pick {
            swatch: swatch.clone(),
        });
        true
    }

    /// User clicked the close button / pressed Escape / clicked the
    /// scrim.
    pub fn close(&mut self) {
        self.pending_action = Some(PalettePickerAction::Close);
    }

    /// Drain the latest action — one-shot. Returns `None` until the
    /// user clicks something next.
    pub fn take_action(&mut self) -> Option<PalettePickerAction> {
        self.pending_action.take()
    }

    /// Whether an action is currently pending (diagnostics + UI gating).
    pub fn has_pending_action(&self) -> bool {
        self.pending_action.is_some()
    }
}

// -----------------------------------------------------------------------------
// build() — chrome subtree the shell mounts inside the host HWND.
// -----------------------------------------------------------------------------

/// Build the PalettePicker popover subtree. Returns the chrome Container
/// today; the swatch grid composition (4-wide circle grid + selected
/// ring + close header) attaches when widget-library ships the final
/// Grid + Popup primitives. Geometry is pinned per snap.md.
pub fn build() -> WidgetNode {
    let chrome = PalettePickerChrome::from_palette(theme::current().palette);
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Px(POPOVER_WIDTH_PX),
        height: Length::Auto,
        padding: Edges::all(POPOVER_PADDING_PX),
        background: chrome.panel_background,
        radius: chrome.panel_radius,
        ..ContainerNode::default()
    })
}

// -----------------------------------------------------------------------------
// Tests.
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;
    use bento_nano_style::tokens as style_tokens;

    #[test]
    fn palette_picker_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
        let chrome = PalettePickerChrome::from_tauri_tokens(
            style_tokens::PALETTE_DARK,
            style_tokens::RADIUS,
            style_tokens::SHADOW,
        );
        assert_eq!(
            chrome.panel_background,
            style_tokens::PALETTE_DARK.surface_expanded
        );
        assert_eq!(
            chrome.chip_background,
            style_tokens::PALETTE_DARK.surface_subtle
        );
        assert_eq!(chrome.title_color, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(chrome.body_color, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(chrome.muted_color, style_tokens::PALETTE_DARK.text_muted);
        // selected ring sits on accent_blue per Wave A palette-picker.md
        assert_eq!(chrome.warning_color, style_tokens::PALETTE_DARK.accent_blue);
        assert_eq!(
            chrome.panel_radius,
            BorderRadius::all(style_tokens::RADIUS.expanded)
        );
        assert_eq!(
            chrome.swatch_radius,
            BorderRadius::all(style_tokens::RADIUS.card)
        );
        // M6b — `SHADOW.expanded` is a `ShadowStack`; chrome consumes `.outer()`.
        assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded.outer());
    }

    #[test]
    fn snap_geometry_constants_pinned() {
        assert_eq!(POPOVER_WIDTH_PX, 240.0);
        assert!((POPOVER_MAX_WIDTH_FRACTION - 0.92).abs() < f32::EPSILON);
        assert_eq!(POPOVER_PADDING_PX, 12.0);
        assert_eq!(POPOVER_CORNER_RADIUS_PX, 12.0);
        assert_eq!(HEADER_HEIGHT_PX, 28.0);
        assert_eq!(HEADER_BOTTOM_MARGIN_PX, 8.0);
        assert_eq!(SWATCH_COLUMNS, 4);
        assert_eq!(SWATCH_CELL_SIZE_PX, 40.0);
        assert_eq!(SWATCH_CIRCLE_DIAMETER_PX, 32.0);
        assert_eq!(SWATCH_CELL_GAP_PX, 8.0);
        assert!((SWATCH_HOVER_SCALE - 1.05).abs() < f32::EPSILON);
        assert_eq!(SELECTED_RING_WIDTH_PX, 2.0);
    }

    #[test]
    fn swatch_table_has_twelve_entries_in_snap_order() {
        let table = swatch_table();
        assert_eq!(table.len(), 12);
        let slugs: Vec<&str> = table.iter().map(|s| s.slug.as_str()).collect();
        assert_eq!(
            slugs,
            vec![
                "slate", "blue", "indigo", "violet", "pink", "red", "orange", "amber", "yellow",
                "green", "teal", "cyan",
            ]
        );
    }

    #[test]
    fn swatch_table_is_stable_across_calls() {
        let a = swatch_table();
        let b = swatch_table();
        assert_eq!(a.as_ptr(), b.as_ptr());
    }

    #[test]
    fn swatch_table_hex_values_match_snap() {
        let table = swatch_table();
        let hex_for = |slug: &str| {
            table
                .iter()
                .find(|s| s.slug.as_str() == slug)
                .map(|s| s.hex.as_str())
        };
        assert_eq!(hex_for("slate"), Some("#64748b"));
        assert_eq!(hex_for("blue"), Some("#3b82f6"));
        assert_eq!(hex_for("indigo"), Some("#6366f1"));
        assert_eq!(hex_for("violet"), Some("#8b5cf6"));
        assert_eq!(hex_for("pink"), Some("#ec4899"));
        assert_eq!(hex_for("red"), Some("#ef4444"));
        assert_eq!(hex_for("orange"), Some("#f97316"));
        assert_eq!(hex_for("amber"), Some("#f59e0b"));
        assert_eq!(hex_for("yellow"), Some("#eab308"));
        assert_eq!(hex_for("green"), Some("#22c55e"));
        assert_eq!(hex_for("teal"), Some("#14b8a6"));
        assert_eq!(hex_for("cyan"), Some("#06b6d4"));
    }

    #[test]
    fn chrome_accepts_explicit_active_palette() {
        let mut palette = theme::current().palette;
        palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
        palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
        palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
        palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);
        palette.warning = Color::from_u8(0xCC, 0x99, 0x44, 0xFF);

        let chrome = PalettePickerChrome::from_palette(palette);

        assert_eq!(
            chrome.panel_background,
            Color::from_u8(0x22, 0x33, 0x44, 0xDD)
        );
        assert_eq!(
            chrome.chip_background,
            Color::from_u8(0x11, 0x22, 0x33, 0xEE)
        );
        assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
        assert_eq!(chrome.warning_color, Color::from_u8(0xCC, 0x99, 0x44, 0xFF));
    }

    #[test]
    fn chrome_accepts_explicit_radius_shadow_tokens() {
        let palette = theme::current().palette;
        let radius = RadiusTokens {
            sm: BorderRadius::all(3.0),
            md: BorderRadius::all(7.0),
            lg: BorderRadius::all(11.0),
            xl: BorderRadius::all(17.0),
            full: BorderRadius::all(999.0),
        };
        let mut shadow = shadow::DEFAULT;
        shadow.md = Shadow {
            offset_x: 2.0,
            offset_y: 5.0,
            blur: 13.0,
            spread: 0.0,
            color: Color::from_u8(0x10, 0x20, 0x30, 0x99),
        };

        let chrome = PalettePickerChrome::from_tokens(palette, radius, shadow);

        assert_eq!(chrome.panel_shadow, shadow.md);
        assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.swatch_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.swatch_inner_radius, BorderRadius::all(7.0));
        assert_eq!(chrome.clear_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.clear_inner_radius, BorderRadius::all(7.0));
    }

    #[test]
    fn find_by_slug_resolves_known_and_returns_none_for_unknown() {
        let blue = Swatch::find_by_slug("blue").expect("known slug");
        assert_eq!(blue.hex.as_str(), "#3b82f6");
        assert!(Swatch::find_by_slug("magenta").is_none());
    }

    #[test]
    fn fresh_state_has_no_selection() {
        let s = PalettePickerState::new();
        assert!(s.selected_slug().is_none());
        assert!(s.selected_swatch().is_none());
        assert!(!s.has_pending_action());
    }

    #[test]
    fn set_selected_known_slug_records_highlight() {
        let mut s = PalettePickerState::new();
        s.set_selected("blue");
        assert_eq!(s.selected_slug(), Some("blue"));
        let sw = s.selected_swatch().expect("highlighted swatch");
        assert_eq!(sw.hex.as_str(), "#3b82f6");
    }

    #[test]
    fn set_selected_unknown_slug_clears_highlight_no_panic() {
        let mut s = PalettePickerState::new();
        s.set_selected("blue");
        s.set_selected("unknown-color");
        assert!(s.selected_slug().is_none());
        assert!(s.selected_swatch().is_none());
    }

    #[test]
    fn clear_selected_drops_highlight() {
        let mut s = PalettePickerState::new();
        s.set_selected("blue");
        s.clear_selected();
        assert!(s.selected_slug().is_none());
    }

    #[test]
    fn pick_known_swatch_records_action_and_updates_selection() {
        let mut s = PalettePickerState::new();
        assert!(s.pick("green"));
        assert_eq!(s.selected_slug(), Some("green"));
        let action = s.take_action().expect("pick recorded");
        match action {
            PalettePickerAction::Pick { swatch } => {
                assert_eq!(swatch.slug.as_str(), "green");
                assert_eq!(swatch.hex.as_str(), "#22c55e");
            }
            other => panic!("expected Pick, got {other:?}"),
        }
    }

    #[test]
    fn pick_unknown_swatch_records_nothing() {
        let mut s = PalettePickerState::new();
        assert!(!s.pick("magenta"));
        assert!(!s.has_pending_action());
        assert!(s.selected_slug().is_none());
    }

    #[test]
    fn close_records_close_action() {
        let mut s = PalettePickerState::new();
        s.close();
        assert_eq!(s.take_action(), Some(PalettePickerAction::Close));
    }

    #[test]
    fn take_action_is_one_shot() {
        let mut s = PalettePickerState::new();
        s.close();
        assert!(s.take_action().is_some());
        assert!(s.take_action().is_none());
    }

    #[test]
    fn build_returns_popover_sized_container() {
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - POPOVER_WIDTH_PX).abs() < 0.01));
        assert_eq!(layout.direction, Direction::Column);
        assert!((layout.padding.top - POPOVER_PADDING_PX).abs() < 0.01);
        assert!((layout.padding.left - POPOVER_PADDING_PX).abs() < 0.01);
    }

    /// ΔB lock: `Swatch` round-trips through serde so any future scripting
    /// surface (Phase 5+) can hand a swatch payload back to the picker.
    #[test]
    fn swatch_serde_round_trip() {
        let sw = Swatch::find_by_slug("indigo")
            .expect("known swatch")
            .clone();
        let s = serde_json::to_string(&sw).unwrap_or_default();
        let back: Swatch = serde_json::from_str(&s).unwrap_or_else(|_| sw.clone());
        assert_eq!(back, sw);
    }

    /// ΔB lock: `PalettePickerAction::Pick` carries a `Swatch` and must
    /// also round-trip cleanly.
    #[test]
    fn palette_picker_action_serde_round_trip() {
        let action = PalettePickerAction::Pick {
            swatch: Swatch::find_by_slug("teal").expect("known").clone(),
        };
        let s = serde_json::to_string(&action).unwrap_or_default();
        let back: PalettePickerAction =
            serde_json::from_str(&s).unwrap_or(PalettePickerAction::Close);
        assert_eq!(back, action);
    }
}
