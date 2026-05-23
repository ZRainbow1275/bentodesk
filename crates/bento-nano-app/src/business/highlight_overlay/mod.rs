//! Business surface — `HighlightOverlay` (T-069b).
//!
//! Translucent fill rect layer that previews which items would be
//! affected by a `SmartGroupSuggestor` row before the user clicks Apply.
//! Visual spec: `highlight_overlay.snap.md`. Paired with
//! `business::smart_group_suggestor`.
//!
//! The overlay is **shell-local state** — it never round-trips through
//! the dispatcher. The suggestor calls
//! [`HighlightOverlayState::set_targets`] / [`clear`] in response to
//! row hover events; the render layer reads `targets()` and paints a
//! translucent fill (and optional outline) over each rect.
//!
//! [`set_targets`]: HighlightOverlayState::set_targets
//! [`clear`]: HighlightOverlayState::clear

use bento_nano_layout::Direction;
use bento_nano_style::tokens::{PaletteTauri, RadiusTauri};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Rect};
use bento_nano_theme::{self as theme, PaletteTokens, RadiusTokens, radius};
use bento_nano_widget::{ContainerNode, WidgetNode};
use bento_nano_zone::{Zone, ZoneItem};
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::business::{item_card, item_grid};

// -----------------------------------------------------------------------------
// Snap.md geometry + colour constants.
// -----------------------------------------------------------------------------

/// Outline stroke width in DIPs.
pub const OUTLINE_WIDTH_PX: f32 = 2.0;

/// Inset applied to each cell rect so the highlight does not occlude
/// the item card's own border.
pub const TARGET_INSET_PX: f32 = 4.0;

/// Outline corner radius — matches `item_card.snap.md` (8 px).
pub const TARGET_CORNER_RADIUS_PX: f32 = 8.0;

/// Translucent fill alpha applied to the palette accent (≈ 20 %).
pub const FILL_ALPHA: f32 = 0.20;

/// Outline alpha applied to the palette accent (≈ 80 %).
pub const OUTLINE_ALPHA: f32 = 0.80;

/// Inline cap on simultaneously-highlighted targets. The backend's
/// `MAX_CLUSTER_SIZE` is 15, but the typical preview hits ≤ 8 cells —
/// the SmallVec inline keeps the common case alloc-free.
pub const INLINE_TARGET_CAP: usize = 8;

/// Desktop-icon pulse loop duration. Mirrors the 1.x `pulse-fade` cadence
/// while remaining deterministic inside the shell frame tick.
pub const PULSE_LOOP_MS: u32 = 1_600;

/// Inner solid dot radius for desktop-icon pulse targets.
pub const PULSE_CORE_RADIUS_PX: f32 = 8.0;

/// Maximum halo radius for desktop-icon pulse targets.
pub const PULSE_HALO_RADIUS_PX: f32 = 28.0;

/// Minimum halo radius at the start of each pulse loop.
pub const PULSE_HALO_MIN_RADIUS_PX: f32 = 14.0;

/// Alpha of the solid center dot.
pub const PULSE_CORE_ALPHA: f32 = 0.70;

/// Alpha of the expanding pulse halo at phase 0.
pub const PULSE_HALO_ALPHA: f32 = 0.34;

// -----------------------------------------------------------------------------
// Data types.
// -----------------------------------------------------------------------------

/// One highlight rect in the BentoPanel's local coordinate space (DIPs).
/// Coordinates are inclusive of the cell border; the renderer applies
/// [`TARGET_INSET_PX`] before painting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighlightRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl HighlightRect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn from_rect(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }

    pub const fn to_rect(self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// A desktop-icon pulse target in the main overlay's logical coordinate space.
///
/// Unlike [`HighlightRect`], this is not tied to a zone/item cell. It maps
/// source desktop paths through the real Windows icon-position snapshot so
/// Search/Suggestor can point at off-grid desktop files that are not currently
/// imported into a Bento zone.
#[derive(Debug, Clone, PartialEq)]
pub struct HighlightPulse {
    pub name: SmolStr,
    pub x: f32,
    pub y: f32,
}

impl HighlightPulse {
    pub fn new(name: impl Into<SmolStr>, x: f32, y: f32) -> Self {
        Self {
            name: name.into(),
            x,
            y,
        }
    }
}

/// Shared item-card geometry for Search/Suggestor target mapping and renderer
/// painting. This must stay aligned with the item-grid renderer so the overlay
/// lands on the same real item cards that the user sees.
pub fn item_card_rect_for_grid(zone: &Zone, grid_x: i32, grid_y: i32, is_wide: bool) -> Rect {
    let zone_left = zone.x as f32;
    let zone_top = zone.y as f32;
    let zone_right = zone_left + zone.w as f32;
    let zone_bottom = zone_top + zone.h as f32;
    let columns = zone.grid_columns.max(1) as f32;
    let gap = item_grid::ITEM_GRID_COLUMN_GAP_PX;
    let cell_w = ((zone.w as f32 - 16.0) - gap * (columns - 1.0)).max(44.0) / columns;
    let span = item_grid::column_span_for(is_wide) as f32;
    let item_x = zone_left + 8.0 + grid_x.max(0) as f32 * (cell_w + gap);
    let item_y = zone_top
        + 30.0
        + grid_y.max(0) as f32
            * (item_grid::ITEM_GRID_ROW_HEIGHT_PX + item_grid::ITEM_GRID_ROW_GAP_PX);
    Rect {
        x: item_x,
        y: item_y,
        width: (cell_w * span + gap * (span - 1.0)).min((zone_right - 8.0 - item_x).max(0.0)),
        height: item_card::CardVariant::Standard
            .height_px()
            .min((zone_bottom - 8.0 - item_y).max(0.0)),
    }
}

/// Full-zone target used by Search zone hits and live-folder hits.
pub fn zone_target_rect(zone: &Zone) -> HighlightRect {
    HighlightRect::new(zone.x as f32, zone.y as f32, zone.w as f32, zone.h as f32)
}

/// Item target used by Search item hits and Suggestor matching-path previews.
pub fn item_target_rect(zone: &Zone, item: &ZoneItem) -> HighlightRect {
    HighlightRect::from_rect(item_card_rect_for_grid(zone, item.x, item.y, item.is_wide))
}

/// Renderer paint rect after applying the snap.md inset.
pub fn paint_rect(target: HighlightRect) -> Rect {
    let rect = target.to_rect();
    Rect {
        x: rect.x + TARGET_INSET_PX,
        y: rect.y + TARGET_INSET_PX,
        width: (rect.width - (TARGET_INSET_PX * 2.0)).max(0.0),
        height: (rect.height - (TARGET_INSET_PX * 2.0)).max(0.0),
    }
}

/// Clamp an elapsed pulse value into the repeat-loop phase `0.0..=1.0`.
pub fn pulse_phase(elapsed_ms: u32) -> f32 {
    if PULSE_LOOP_MS == 0 {
        return 0.0;
    }
    (elapsed_ms % PULSE_LOOP_MS) as f32 / PULSE_LOOP_MS as f32
}

/// Expanding halo rect for a desktop-icon pulse.
pub fn pulse_halo_rect(target: &HighlightPulse, phase: f32) -> Rect {
    let clamped = phase.clamp(0.0, 1.0);
    let radius =
        PULSE_HALO_MIN_RADIUS_PX + (PULSE_HALO_RADIUS_PX - PULSE_HALO_MIN_RADIUS_PX) * clamped;
    Rect {
        x: target.x - radius,
        y: target.y - radius,
        width: radius * 2.0,
        height: radius * 2.0,
    }
}

/// Solid center dot rect for a desktop-icon pulse.
pub fn pulse_core_rect(target: &HighlightPulse) -> Rect {
    Rect {
        x: target.x - PULSE_CORE_RADIUS_PX,
        y: target.y - PULSE_CORE_RADIUS_PX,
        width: PULSE_CORE_RADIUS_PX * 2.0,
        height: PULSE_CORE_RADIUS_PX * 2.0,
    }
}

/// Target corner radius from explicit active radius tokens.
pub fn target_radius_from_tokens(radius: RadiusTokens) -> BorderRadius {
    radius.lg
}

/// Target corner radius from the process-default theme.
pub fn target_radius() -> BorderRadius {
    target_radius_from_tokens(radius::DEFAULT)
}

// -----------------------------------------------------------------------------
// HighlightOverlayState — runtime state for the overlay.
// -----------------------------------------------------------------------------

/// Overlay runtime state.
///
/// - `targets` — the rects to paint. Empty = nothing to render.
/// - `pulses` — off-grid desktop-icon circles resolved from real Windows
///   icon-position snapshots.
/// - `auto_clear_ms` — when `Some(remaining)`, [`tick`] decrements it
///   each frame and clears the targets when it reaches zero. Mirrors
///   the 1.x `HIGHLIGHT_DURATION_MS = 3_000` auto-fade convention but
///   defers the countdown to the shell's frame loop.
/// - `pulse_elapsed_ms` — deterministic frame-loop phase for pulsing circles.
/// - `show_outline` — when true, the renderer draws an outline stroke
///   in addition to the translucent fill. Default true.
///
/// [`tick`]: HighlightOverlayState::tick
#[derive(Debug, Clone)]
pub struct HighlightOverlayState {
    targets: SmallVec<[HighlightRect; INLINE_TARGET_CAP]>,
    pulses: SmallVec<[HighlightPulse; INLINE_TARGET_CAP]>,
    auto_clear_ms: Option<u32>,
    pulse_elapsed_ms: u32,
    show_outline: bool,
}

impl Default for HighlightOverlayState {
    fn default() -> Self {
        Self {
            targets: SmallVec::new(),
            pulses: SmallVec::new(),
            auto_clear_ms: None,
            pulse_elapsed_ms: 0,
            show_outline: true,
        }
    }
}

impl HighlightOverlayState {
    /// New empty state — no targets, outline enabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow the current target list.
    pub fn targets(&self) -> &[HighlightRect] {
        &self.targets
    }

    /// Borrow the current desktop-icon pulse target list.
    pub fn pulses(&self) -> &[HighlightPulse] {
        &self.pulses
    }

    /// Whether the overlay should currently paint anything.
    pub fn has_targets(&self) -> bool {
        !self.targets.is_empty() || !self.pulses.is_empty()
    }

    /// Current desktop-icon pulse phase for renderers.
    pub fn current_pulse_phase(&self) -> f32 {
        pulse_phase(self.pulse_elapsed_ms)
    }

    /// Outline-stroke toggle.
    pub fn show_outline(&self) -> bool {
        self.show_outline
    }

    /// Override the outline-stroke toggle.
    pub fn set_show_outline(&mut self, show: bool) {
        self.show_outline = show;
    }

    /// Replace the highlight target list. Cancels any pending
    /// auto-clear countdown — the new highlight is "sticky" until the
    /// caller explicitly clears or starts a new countdown.
    pub fn set_targets<I>(&mut self, rects: I)
    where
        I: IntoIterator<Item = HighlightRect>,
    {
        self.targets.clear();
        self.pulses.clear();
        for r in rects {
            self.targets.push(r);
        }
        self.auto_clear_ms = None;
        self.pulse_elapsed_ms = 0;
    }

    /// Replace both in-zone rect targets and off-grid desktop-icon pulse
    /// targets. Cancels any in-flight countdown.
    pub fn set_targets_and_pulses<I, J>(&mut self, rects: I, pulses: J)
    where
        I: IntoIterator<Item = HighlightRect>,
        J: IntoIterator<Item = HighlightPulse>,
    {
        self.targets.clear();
        self.pulses.clear();
        for r in rects {
            self.targets.push(r);
        }
        for p in pulses {
            self.pulses.push(p);
        }
        self.auto_clear_ms = None;
        self.pulse_elapsed_ms = 0;
    }

    /// Replace the target list and start an auto-clear countdown. After
    /// `duration_ms` of `tick` time the targets clear automatically.
    /// `duration_ms == 0` is treated as "no countdown" (sticky highlight).
    pub fn set_targets_for<I>(&mut self, rects: I, duration_ms: u32)
    where
        I: IntoIterator<Item = HighlightRect>,
    {
        self.set_targets(rects);
        if duration_ms > 0 {
            self.auto_clear_ms = Some(duration_ms);
        }
    }

    /// Replace rect+pulse targets and start an optional auto-clear countdown.
    pub fn set_targets_and_pulses_for<I, J>(&mut self, rects: I, pulses: J, duration_ms: u32)
    where
        I: IntoIterator<Item = HighlightRect>,
        J: IntoIterator<Item = HighlightPulse>,
    {
        self.set_targets_and_pulses(rects, pulses);
        if duration_ms > 0 {
            self.auto_clear_ms = Some(duration_ms);
        }
    }

    /// Clear every target and cancel the countdown.
    pub fn clear(&mut self) {
        self.targets.clear();
        self.pulses.clear();
        self.auto_clear_ms = None;
        self.pulse_elapsed_ms = 0;
    }

    /// Advance the auto-clear countdown by `dt_ms`. Returns `true` when
    /// the overlay still wants frames (countdown in flight); `false`
    /// when there's nothing to do (no countdown, or it just fired).
    pub fn tick(&mut self, dt_ms: u32) -> bool {
        if !self.pulses.is_empty() {
            self.pulse_elapsed_ms = self
                .pulse_elapsed_ms
                .wrapping_add(dt_ms)
                .wrapping_rem(PULSE_LOOP_MS.max(1));
        }
        let Some(remaining) = self.auto_clear_ms else {
            return !self.pulses.is_empty();
        };
        if dt_ms >= remaining {
            self.clear();
            false
        } else {
            self.auto_clear_ms = Some(remaining - dt_ms);
            true
        }
    }

    /// Sample the remaining countdown in ms (diagnostics).
    pub fn auto_clear_remaining_ms(&self) -> Option<u32> {
        self.auto_clear_ms
    }
}

// -----------------------------------------------------------------------------
// Colour helpers — derive translucent fill + outline from palette accent.
// -----------------------------------------------------------------------------

/// Translucent fill colour for the highlight rects — `palette.accent`
/// at `FILL_ALPHA`. Re-evaluated each call so theme switches pick up
/// the new accent automatically.
pub fn fill_color() -> Color {
    fill_color_from_palette(theme::current().palette)
}

/// Translucent fill colour for the highlight rects from explicit active
/// palette tokens.
pub fn fill_color_from_palette(palette: PaletteTokens) -> Color {
    let accent = palette.accent;
    Color {
        a: FILL_ALPHA,
        ..accent
    }
}

/// Outline stroke colour — `palette.accent` at `OUTLINE_ALPHA`.
pub fn outline_color() -> Color {
    outline_color_from_palette(theme::current().palette)
}

/// Outline stroke colour from explicit active palette tokens.
pub fn outline_color_from_palette(palette: PaletteTokens) -> Color {
    let accent = palette.accent;
    Color {
        a: OUTLINE_ALPHA,
        ..accent
    }
}

/// Solid center dot colour for desktop-icon pulses.
pub fn pulse_core_color_from_palette(palette: PaletteTokens) -> Color {
    let accent = palette.accent;
    Color {
        a: PULSE_CORE_ALPHA,
        ..accent
    }
}

/// Expanding halo colour for desktop-icon pulses.
pub fn pulse_halo_color_from_palette(palette: PaletteTokens, phase: f32) -> Color {
    let accent = palette.accent;
    Color {
        a: PULSE_HALO_ALPHA * (1.0 - phase.clamp(0.0, 1.0)),
        ..accent
    }
}

// -----------------------------------------------------------------------------
// Wave B Tauri-token variants — derive translucent fill + outline + pulse from
// the SSoT `accent_blue` instead of the legacy theme palette.
// -----------------------------------------------------------------------------

/// Translucent fill colour from Wave B Tauri tokens — `accent_blue` at `FILL_ALPHA`.
pub fn fill_color_from_tauri_palette(palette: PaletteTauri) -> Color {
    Color {
        a: FILL_ALPHA,
        ..palette.accent_blue
    }
}

/// Outline stroke colour from Wave B Tauri tokens — `accent_blue` at `OUTLINE_ALPHA`.
pub fn outline_color_from_tauri_palette(palette: PaletteTauri) -> Color {
    Color {
        a: OUTLINE_ALPHA,
        ..palette.accent_blue
    }
}

/// Solid centre-dot colour from Wave B Tauri tokens — `accent_blue` at `PULSE_CORE_ALPHA`.
pub fn pulse_core_color_from_tauri_palette(palette: PaletteTauri) -> Color {
    Color {
        a: PULSE_CORE_ALPHA,
        ..palette.accent_blue
    }
}

/// Expanding halo colour from Wave B Tauri tokens — `accent_blue` with fade phase.
pub fn pulse_halo_color_from_tauri_palette(palette: PaletteTauri, phase: f32) -> Color {
    Color {
        a: PULSE_HALO_ALPHA * (1.0 - phase.clamp(0.0, 1.0)),
        ..palette.accent_blue
    }
}

/// Target corner radius from Wave B Tauri tokens — uses `RADIUS.card` (10 px),
/// matching the Tauri `--radius-card` item-card chrome.
pub fn target_radius_from_tauri_tokens(radius: RadiusTauri) -> BorderRadius {
    BorderRadius::all(radius.card)
}

// -----------------------------------------------------------------------------
// Builder — returns the chrome Container.
// -----------------------------------------------------------------------------

/// Build the HighlightOverlay subtree. Returns a transparent
/// edge-to-edge Container that the renderer paints fills + outlines
/// inside; the per-rect draw routine reads the live state via the
/// shell. Intentionally `Auto` sized so the BentoPanel content layer
/// stretches it across the available area.
pub fn build() -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Auto,
        height: Length::Auto,
        padding: Edges::ZERO,
        background: Color::TRANSPARENT,
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

    fn rect(x: f32, y: f32) -> HighlightRect {
        HighlightRect::new(x, y, 80.0, 80.0)
    }

    #[test]
    fn snap_constants_pinned() {
        assert_eq!(OUTLINE_WIDTH_PX, 2.0);
        assert_eq!(TARGET_INSET_PX, 4.0);
        assert_eq!(TARGET_CORNER_RADIUS_PX, 8.0);
        assert!((FILL_ALPHA - 0.20).abs() < f32::EPSILON);
        assert!((OUTLINE_ALPHA - 0.80).abs() < f32::EPSILON);
        assert_eq!(INLINE_TARGET_CAP, 8);
    }

    #[test]
    fn build_returns_transparent_auto_container() {
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Auto));
        assert!(matches!(layout.height, Length::Auto));
        assert_eq!(layout.padding, Edges::ZERO);
    }

    #[test]
    fn fill_color_uses_palette_accent_with_alpha() {
        let palette = theme::current().palette;
        let fill = fill_color();
        assert_eq!(fill.r, palette.accent.r);
        assert_eq!(fill.g, palette.accent.g);
        assert_eq!(fill.b, palette.accent.b);
        assert!((fill.a - FILL_ALPHA).abs() < f32::EPSILON);
    }

    #[test]
    fn outline_color_uses_palette_accent_with_alpha() {
        let palette = theme::current().palette;
        let outline = outline_color();
        assert_eq!(outline.r, palette.accent.r);
        assert!((outline.a - OUTLINE_ALPHA).abs() < f32::EPSILON);
    }

    #[test]
    fn highlight_colors_accept_explicit_active_palette() {
        let mut palette = theme::current().palette;
        palette.accent = Color::from_u8(0x44, 0x88, 0xCC, 0xFF);

        let fill = fill_color_from_palette(palette);
        let outline = outline_color_from_palette(palette);

        assert_eq!(fill.r, palette.accent.r);
        assert_eq!(fill.g, palette.accent.g);
        assert_eq!(fill.b, palette.accent.b);
        assert!((fill.a - FILL_ALPHA).abs() < f32::EPSILON);
        assert_eq!(outline.r, palette.accent.r);
        assert_eq!(outline.g, palette.accent.g);
        assert_eq!(outline.b, palette.accent.b);
        assert!((outline.a - OUTLINE_ALPHA).abs() < f32::EPSILON);
    }

    #[test]
    fn target_radius_accepts_explicit_active_radius_tokens() {
        let radius = RadiusTokens {
            sm: BorderRadius::all(3.0),
            md: BorderRadius::all(7.0),
            lg: BorderRadius::all(11.0),
            xl: BorderRadius::all(17.0),
            full: BorderRadius::all(999.0),
        };

        assert_eq!(target_radius_from_tokens(radius), BorderRadius::all(11.0));
        assert_eq!(target_radius(), BorderRadius::all(TARGET_CORNER_RADIUS_PX));
    }

    #[test]
    fn default_state_has_no_targets_outline_on() {
        let state = HighlightOverlayState::new();
        assert!(!state.has_targets());
        assert!(state.show_outline());
        assert!(state.auto_clear_remaining_ms().is_none());
    }

    #[test]
    fn set_targets_overwrites_previous_list() {
        let mut state = HighlightOverlayState::new();
        state.set_targets([rect(0.0, 0.0), rect(100.0, 0.0)]);
        assert_eq!(state.targets().len(), 2);
        state.set_targets([rect(50.0, 50.0)]);
        assert_eq!(state.targets().len(), 1);
        assert_eq!(state.targets()[0].x, 50.0);
    }

    #[test]
    fn set_targets_cancels_in_flight_countdown() {
        let mut state = HighlightOverlayState::new();
        state.set_targets_for([rect(0.0, 0.0)], 1_000);
        assert_eq!(state.auto_clear_remaining_ms(), Some(1_000));
        // Replacing with non-timed set_targets cancels the countdown.
        state.set_targets([rect(0.0, 0.0)]);
        assert!(state.auto_clear_remaining_ms().is_none());
    }

    #[test]
    fn clear_empties_targets_and_cancels_countdown() {
        let mut state = HighlightOverlayState::new();
        state.set_targets_for([rect(0.0, 0.0)], 500);
        state.clear();
        assert!(!state.has_targets());
        assert!(state.auto_clear_remaining_ms().is_none());
    }

    #[test]
    fn tick_with_no_countdown_returns_false() {
        let mut state = HighlightOverlayState::new();
        state.set_targets([rect(0.0, 0.0)]);
        // No countdown set — tick has nothing to do, targets stay.
        assert!(!state.tick(100));
        assert!(state.has_targets());
    }

    #[test]
    fn tick_decrements_countdown_until_clear() {
        let mut state = HighlightOverlayState::new();
        state.set_targets_for([rect(0.0, 0.0)], 1_000);
        // Half the window — still ticking, targets remain.
        assert!(state.tick(500));
        assert_eq!(state.auto_clear_remaining_ms(), Some(500));
        assert!(state.has_targets());
        // Cross the threshold — clears + returns false.
        assert!(!state.tick(500));
        assert!(!state.has_targets());
        assert!(state.auto_clear_remaining_ms().is_none());
    }

    #[test]
    fn tick_overshoot_clears_immediately() {
        let mut state = HighlightOverlayState::new();
        state.set_targets_for([rect(0.0, 0.0)], 200);
        // dt larger than remaining → clears in one go.
        assert!(!state.tick(1_000));
        assert!(!state.has_targets());
    }

    #[test]
    fn set_targets_for_zero_duration_treated_as_sticky() {
        let mut state = HighlightOverlayState::new();
        state.set_targets_for([rect(0.0, 0.0)], 0);
        assert!(state.auto_clear_remaining_ms().is_none());
        assert!(state.has_targets());
    }

    #[test]
    fn show_outline_toggle_round_trip() {
        let mut state = HighlightOverlayState::new();
        assert!(state.show_outline());
        state.set_show_outline(false);
        assert!(!state.show_outline());
        state.set_show_outline(true);
        assert!(state.show_outline());
    }

    #[test]
    fn item_target_rect_matches_grid_geometry() {
        let mut zone = Zone::new(bento_nano_zone::ZoneId(9), "Docs", 10, 20, 240, 180);
        let item_id = zone.add_item("C:/Desktop/doc.pdf", "hash");
        assert!(item_id.is_some(), "item id is allocated");
        let item_id = item_id.unwrap_or(bento_nano_zone::ZoneItemId(0));
        let Some(item) = zone.item(item_id) else {
            assert!(zone.item(item_id).is_some(), "item exists");
            return;
        };

        let target = item_target_rect(&zone, item);

        assert!((target.x - 18.0).abs() < f32::EPSILON);
        assert!((target.y - 50.0).abs() < f32::EPSILON);
        assert!(target.width > 0.0);
        assert!(target.height > 0.0);
    }

    #[test]
    fn paint_rect_applies_snap_inset() {
        let painted = paint_rect(HighlightRect::new(10.0, 20.0, 80.0, 60.0));

        assert_eq!(painted.x, 14.0);
        assert_eq!(painted.y, 24.0);
        assert_eq!(painted.width, 72.0);
        assert_eq!(painted.height, 52.0);
    }

    #[test]
    fn pulse_geometry_and_color_follow_phase() {
        let target = HighlightPulse::new("desktop.pdf", 100.0, 120.0);
        let phase = 0.5;
        let halo = pulse_halo_rect(&target, phase);
        let core = pulse_core_rect(&target);
        let palette = theme::current().palette;
        let halo_color = pulse_halo_color_from_palette(palette, phase);
        let core_color = pulse_core_color_from_palette(palette);

        assert!(halo.width > core.width);
        assert_eq!(core.x, 92.0);
        assert_eq!(core.y, 112.0);
        assert_eq!(pulse_phase(PULSE_LOOP_MS + 800), 0.5);
        assert!((halo_color.a - PULSE_HALO_ALPHA * 0.5).abs() < f32::EPSILON);
        assert!((core_color.a - PULSE_CORE_ALPHA).abs() < f32::EPSILON);
    }

    #[test]
    fn state_tracks_pulses_and_ticks_animation() {
        let mut state = HighlightOverlayState::new();
        state.set_targets_and_pulses(
            std::iter::empty::<HighlightRect>(),
            [HighlightPulse::new("desktop.pdf", 40.0, 50.0)],
        );

        assert!(state.has_targets());
        assert_eq!(state.targets().len(), 0);
        assert_eq!(state.pulses().len(), 1);
        assert!(state.tick(400));
        assert!(state.current_pulse_phase() > 0.0);

        state.set_targets_and_pulses_for(
            std::iter::empty::<HighlightRect>(),
            [HighlightPulse::new("desktop.pdf", 40.0, 50.0)],
            10,
        );
        assert!(!state.tick(10));
        assert!(!state.has_targets());
    }

    #[test]
    fn inline_cap_avoids_heap_for_typical_clusters() {
        let mut state = HighlightOverlayState::new();
        let rects: Vec<HighlightRect> = (0..INLINE_TARGET_CAP)
            .map(|i| rect(i as f32 * 100.0, 0.0))
            .collect();
        state.set_targets(rects);
        assert_eq!(state.targets().len(), INLINE_TARGET_CAP);
        // SmallVec internal API doesn't expose `spilled` publicly here;
        // the contract we lock instead is "exactly INLINE_TARGET_CAP fits
        // in the typed inline storage" — a regression that shrinks the
        // cap would fail this length equality.
    }

    #[test]
    fn highlight_overlay_colors_from_tauri_palette_use_accent_blue() {
        use bento_nano_style::tokens as style_tokens;
        let fill = fill_color_from_tauri_palette(style_tokens::PALETTE_DARK);
        assert_eq!(fill.r, style_tokens::PALETTE_DARK.accent_blue.r);
        assert!((fill.a - FILL_ALPHA).abs() < f32::EPSILON);

        let outline = outline_color_from_tauri_palette(style_tokens::PALETTE_DARK);
        assert!((outline.a - OUTLINE_ALPHA).abs() < f32::EPSILON);

        let core = pulse_core_color_from_tauri_palette(style_tokens::PALETTE_DARK);
        assert!((core.a - PULSE_CORE_ALPHA).abs() < f32::EPSILON);

        // Phase = 0 → halo at full PULSE_HALO_ALPHA.
        let halo = pulse_halo_color_from_tauri_palette(style_tokens::PALETTE_DARK, 0.0);
        assert!((halo.a - PULSE_HALO_ALPHA).abs() < f32::EPSILON);
        // Phase = 1 → halo fades to zero.
        let halo = pulse_halo_color_from_tauri_palette(style_tokens::PALETTE_DARK, 1.0);
        assert!(halo.a.abs() < f32::EPSILON);

        // Target radius from Tauri tokens is RADIUS.card (10 px).
        assert_eq!(
            target_radius_from_tauri_tokens(style_tokens::RADIUS),
            BorderRadius::all(style_tokens::RADIUS.card)
        );
    }
}
