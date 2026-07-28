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
use crate::expanded_zone_grid;

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

mod geometry;

pub use geometry::*;

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
mod tests;
