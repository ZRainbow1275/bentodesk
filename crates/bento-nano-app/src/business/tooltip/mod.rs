//! `Tooltip` — delayed-show hover label.
//!
//! 1.x source: `bentodesk/src/components/shared/Tooltip.tsx` +
//! `Tooltip.css`. Solid-JS portal-mounted floating element with a 400 ms
//! show delay, 150 ms fade-in animation, and below/above auto-flip when the
//! anchor is too close to the top of the viewport.
//!
//! Visual fidelity reference: `tooltip.snap.md`.
//!
//! # Hosting model
//!
//! In 1.x the tooltip mounted into `document.body` via `<Portal>`. In 2.0
//! the visual is a `WindowKind::Tooltip` HWND created by the dispatcher's
//! `Command::ShowTooltip` handler — the layered, click-through, NoActivate
//! tool window described in `bento-nano-platform::window`. This file owns
//! only the **descriptor** + **timing controller**; the HWND lifecycle is
//! the shell's responsibility.
//!
//! # Show / hide timing
//!
//! 1.x sequence:
//!   1. `mouseenter` → start a 400 ms timer.
//!   2. Timer fires → set `open = true` → portal renders → `requestAnimationFrame`
//!      positions the tip above (or below if no room) the anchor rect.
//!   3. `mouseleave` → clear timer + close immediately.
//!
//! 2.0 mirror:
//!   * [`TooltipController::on_anchor_enter`] starts the wall-clock-driven
//!     400 ms wait.
//!   * [`TooltipController::tick`] is called each paint pump tick with the
//!     elapsed `dt`; once the cumulative wait crosses the delay it returns
//!     [`TooltipPhase::Show`] so the caller dispatches `Command::ShowTooltip`.
//!   * [`TooltipController::on_anchor_leave`] cancels the pending timer
//!     and returns [`TooltipPhase::Hide`] when the tooltip was actually
//!     visible.
//!
//! No `std::thread`, no async runtime — purely a `Cell<f32>` wall-clock
//! accumulator. The shell pump already calls into widget controllers each
//! frame for `IconButton::tick`, so the cost is one extra branch.

use core::fmt;

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, SpacingTauri};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Rect, Size};
use bento_nano_theme::{self as theme, PaletteTokens, RadiusTokens, SpacingTokens};
use smol_str::SmolStr;

/// 1.x default of 400 ms before showing a tooltip after mouseenter.
pub const TOOLTIP_DEFAULT_DELAY_MS: u32 = 400;

/// Result of a [`TooltipController`] state transition. Caller acts on
/// `Show` / `Hide` by dispatching the matching `Command` to the bus;
/// `Idle` means no work was triggered this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TooltipPhase {
    /// No state change — nothing to dispatch.
    Idle,
    /// The delay elapsed; caller should dispatch `Command::ShowTooltip`.
    Show,
    /// The user moved out / cancelled; caller should dispatch
    /// `Command::HideTooltip` if a tooltip is currently on screen.
    Hide,
}

impl fmt::Display for TooltipPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => f.write_str("Idle"),
            Self::Show => f.write_str("Show"),
            Self::Hide => f.write_str("Hide"),
        }
    }
}

// -----------------------------------------------------------------------------
// Tooltip widget descriptor — the visual rendered inside the tooltip HWND.
// -----------------------------------------------------------------------------

/// Tooltip descriptor — the small dark pill drawn inside the dedicated
/// `WindowKind::Tooltip` HWND. The HWND itself is layered + click-through
/// per the platform window factory; this struct only owns the visible
/// chrome + label.
#[derive(Debug, Clone)]
pub struct Tooltip {
    /// Tooltip body — `SmolStr` keeps short hover labels inline (the
    /// 1.x truncation policy capped tip text at 320 px wide ≈ 36 chars
    /// in YaHei UI 12pt; well above the 22-byte SmolStr inline cap most
    /// of the time but the heap fallback is identical to other widgets).
    pub text: SmolStr,
    /// Tip background — `0xCC141416` in 1.x → `palette.surface_alt` with
    /// the `.95` alpha matched by the dark-default `surface_alt` token.
    pub background: Color,
    /// Tip text colour — `palette.text` with full alpha.
    pub text_color: Color,
    /// Border radius — `4 px` per `Tooltip.css`.
    pub border_radius: BorderRadius,
    /// Inset padding — `4 × 8 px` per `Tooltip.css`.
    pub padding: Edges,
    /// Width — `Length::Auto` so the layout engine sizes to the text run
    /// (clamped by the host HWND's 200 px default width per
    /// `default_size(WindowKind::Tooltip)`).
    pub width: Length,
    /// Height — `Length::Auto` (text height + padding).
    pub height: Length,
}

impl Tooltip {
    /// Construct from a label string using the process default theme.
    pub fn new(text: impl Into<SmolStr>) -> Self {
        let tokens = theme::current();
        Self::from_tokens(text, tokens.palette, tokens.radius, tokens.spacing)
    }

    /// Construct from a label string and explicit active theme tokens.
    pub fn from_tokens(
        text: impl Into<SmolStr>,
        palette: PaletteTokens,
        radius: RadiusTokens,
        spacing: SpacingTokens,
    ) -> Self {
        Self {
            text: text.into(),
            background: palette.surface_alt,
            text_color: palette.text,
            border_radius: radius.sm,
            padding: Edges {
                top: spacing.sm,
                right: spacing.md,
                bottom: spacing.sm,
                left: spacing.md,
            },
            width: Length::Auto,
            height: Length::Auto,
        }
    }

    /// Construct from a label string and Wave B Tauri SSoT tokens.
    ///
    /// Token mapping (Wave A `chrome.md` tooltip metrics):
    /// - background ← `tooltip_bg` (rgba(18,18,24,0.9), denser than surface_zen)
    /// - text colour ← `text_primary`
    /// - radius ← `tooltip` (8 px gap token)
    /// - padding ← 6 px vertical (`SPACING.s6`) × 12 px horizontal (`SPACING.md`)
    pub fn from_tauri_tokens(
        text: impl Into<SmolStr>,
        palette: PaletteTauri,
        radius: RadiusTauri,
        spacing: SpacingTauri,
    ) -> Self {
        Self {
            text: text.into(),
            background: palette.tooltip_bg,
            text_color: palette.text_primary,
            border_radius: BorderRadius::all(radius.tooltip),
            padding: Edges {
                top: spacing.s6,
                right: spacing.md,
                bottom: spacing.s6,
                left: spacing.md,
            },
            width: Length::Auto,
            height: Length::Auto,
        }
    }
}

impl LayoutSource for Tooltip {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
            width: self.width,
            height: self.height,
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

/// Drawable area inside the dedicated tooltip HWND.
pub fn tooltip_pill_rect(viewport: Size) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width: viewport.width.max(1.0),
        height: viewport.height.max(1.0),
    }
}

/// Text run area after applying the descriptor padding.
pub fn tooltip_text_rect(viewport: Size, tooltip: &Tooltip) -> Rect {
    let pill = tooltip_pill_rect(viewport);
    Rect {
        x: pill.x + tooltip.padding.left,
        y: pill.y + tooltip.padding.top,
        width: (pill.width - tooltip.padding.left - tooltip.padding.right).max(1.0),
        height: (pill.height - tooltip.padding.top - tooltip.padding.bottom).max(1.0),
    }
}

// -----------------------------------------------------------------------------
// TooltipController — the show-delay state machine. One per anchor.
// -----------------------------------------------------------------------------

/// Show-delay state machine. Owns the wall-clock accumulator that fires
/// `Show` after the configured `delay_ms` of continuous hover, and the
/// `Hide` request that cancels a pending show or retracts a visible tip.
///
/// Construction-time invariants:
///   * Starts in the `Idle` (no hover) state.
///   * `delay_ms` defaults to [`TOOLTIP_DEFAULT_DELAY_MS`] (400 ms) per the
///     1.x `DEFAULT_DELAY` constant.
#[derive(Debug)]
pub struct TooltipController {
    delay_ms: u32,
    /// Accumulated hover time since the last `on_anchor_enter` call,
    /// in milliseconds. `None` when not hovering.
    hover_elapsed_ms: Option<u32>,
    /// Whether `Show` has already been emitted for the current hover
    /// session. Prevents a second `Show` from `tick()` after the first.
    visible: bool,
}

impl Default for TooltipController {
    fn default() -> Self {
        Self::new()
    }
}

impl TooltipController {
    /// New controller with the 1.x default 400 ms delay.
    pub fn new() -> Self {
        Self {
            delay_ms: TOOLTIP_DEFAULT_DELAY_MS,
            hover_elapsed_ms: None,
            visible: false,
        }
    }

    /// Override the show delay. Called for tooltips with explicit `delay`
    /// overrides on the 1.x prop surface.
    pub fn with_delay_ms(mut self, delay_ms: u32) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// User entered the anchor. Returns [`TooltipPhase::Idle`] always —
    /// the matching `Show` arrives later from [`tick`] once the delay
    /// elapses. Calling `on_anchor_enter` while already hovering rewinds
    /// the timer (matches the 1.x behaviour where a second `mouseenter`
    /// without a `mouseleave` between them would re-arm the `setTimeout`).
    ///
    /// [`tick`]: TooltipController::tick
    pub fn on_anchor_enter(&mut self) -> TooltipPhase {
        self.hover_elapsed_ms = Some(0);
        // Don't reset `visible` — a re-enter while already shown should
        // keep the tip on screen.
        TooltipPhase::Idle
    }

    /// User left the anchor. Returns [`TooltipPhase::Hide`] iff the tip
    /// was actually visible (so the caller dispatches `Command::HideTooltip`),
    /// or `Idle` when the leave happened during the show delay (no tip
    /// was ever rendered, so nothing to retract).
    pub fn on_anchor_leave(&mut self) -> TooltipPhase {
        self.hover_elapsed_ms = None;
        if self.visible {
            self.visible = false;
            TooltipPhase::Hide
        } else {
            TooltipPhase::Idle
        }
    }

    /// Advance the hover wall clock by `dt_ms` milliseconds. Returns
    /// [`TooltipPhase::Show`] exactly once per hover session — the frame
    /// in which the cumulative hover time first crosses `delay_ms`. All
    /// subsequent ticks within the same session return `Idle`.
    pub fn tick(&mut self, dt_ms: u32) -> TooltipPhase {
        let Some(elapsed) = self.hover_elapsed_ms else {
            return TooltipPhase::Idle;
        };
        let next = elapsed.saturating_add(dt_ms);
        self.hover_elapsed_ms = Some(next);
        if !self.visible && next >= self.delay_ms {
            self.visible = true;
            return TooltipPhase::Show;
        }
        TooltipPhase::Idle
    }

    /// Whether the tooltip is currently on screen (per the controller's
    /// view of the world — diverges from the OS state only between an
    /// emitted `Show` and the wndproc's `Command::ShowTooltip` handler).
    #[inline]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Whether the controller is mid-hover (the user has entered the
    /// anchor but the show delay hasn't elapsed yet).
    #[inline]
    pub fn is_pending(&self) -> bool {
        matches!(self.hover_elapsed_ms, Some(t) if t < self.delay_ms)
    }

    /// Configured show delay.
    #[inline]
    pub fn delay_ms(&self) -> u32 {
        self.delay_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_descriptor_uses_palette_surface_alt() {
        let t = Tooltip::new("hi");
        let palette = theme::current().palette;
        assert_eq!(t.background, palette.surface_alt);
        assert_eq!(t.text_color, palette.text);
        assert_eq!(t.border_radius.top_left, 4.0);
    }

    #[test]
    fn tooltip_descriptor_accepts_explicit_active_tokens() {
        let tokens = theme::current();
        let mut palette = tokens.palette;
        let mut radius = tokens.radius;
        let mut spacing = tokens.spacing;
        palette.surface_alt = Color::from_u8(0x12, 0x23, 0x34, 0xCC);
        radius.sm = BorderRadius::all(9.0);
        spacing.sm = 5.0;
        spacing.md = 13.0;

        let t = Tooltip::from_tokens("hi", palette, radius, spacing);

        assert_eq!(t.background, Color::from_u8(0x12, 0x23, 0x34, 0xCC));
        assert_eq!(t.border_radius, BorderRadius::all(9.0));
        assert_eq!(t.padding.top, 5.0);
        assert_eq!(t.padding.left, 13.0);
    }

    #[test]
    fn tooltip_geometry_applies_descriptor_padding_inside_host() {
        let t = Tooltip::new("hover label");
        let viewport = Size {
            width: 200.0,
            height: 40.0,
        };

        let pill = tooltip_pill_rect(viewport);
        assert_eq!(pill.width, 200.0);
        assert_eq!(pill.height, 40.0);

        let text = tooltip_text_rect(viewport, &t);
        assert_eq!(text.x, 8.0);
        assert_eq!(text.y, 4.0);
        assert_eq!(text.width, 184.0);
        assert_eq!(text.height, 32.0);
    }

    #[test]
    fn tooltip_controller_emits_show_after_default_delay() {
        let mut c = TooltipController::new();
        assert_eq!(c.on_anchor_enter(), TooltipPhase::Idle);
        // Tick under the threshold — no Show yet.
        assert_eq!(c.tick(200), TooltipPhase::Idle);
        assert!(c.is_pending());
        assert!(!c.is_visible());
        // Tick across the threshold — Show fires exactly once.
        assert_eq!(c.tick(300), TooltipPhase::Show);
        assert!(c.is_visible());
        // Subsequent ticks within the same session → Idle.
        assert_eq!(c.tick(100), TooltipPhase::Idle);
    }

    #[test]
    fn tooltip_controller_respects_custom_delay() {
        let mut c = TooltipController::new().with_delay_ms(50);
        c.on_anchor_enter();
        assert_eq!(c.tick(60), TooltipPhase::Show);
    }

    #[test]
    fn tooltip_controller_on_anchor_leave_emits_hide_when_visible() {
        let mut c = TooltipController::new().with_delay_ms(10);
        c.on_anchor_enter();
        let _ = c.tick(20);
        assert!(c.is_visible());
        assert_eq!(c.on_anchor_leave(), TooltipPhase::Hide);
        assert!(!c.is_visible());
    }

    #[test]
    fn tooltip_controller_on_anchor_leave_idle_when_not_yet_shown() {
        let mut c = TooltipController::new();
        c.on_anchor_enter();
        // Leave during delay — no tip ever rendered, nothing to retract.
        assert_eq!(c.on_anchor_leave(), TooltipPhase::Idle);
    }

    #[test]
    fn tooltip_controller_re_enter_rewinds_timer() {
        let mut c = TooltipController::new();
        c.on_anchor_enter();
        let _ = c.tick(100);
        // Re-enter mid-session — accumulator resets so the next 350 ms
        // do NOT cross the 400 ms threshold.
        c.on_anchor_enter();
        assert_eq!(c.tick(350), TooltipPhase::Idle);
        assert!(!c.is_visible());
        assert_eq!(c.tick(60), TooltipPhase::Show);
    }

    #[test]
    fn tooltip_from_tauri_tokens_consumes_wave_b_ssot() {
        use bento_nano_style::tokens as style_tokens;
        let t = Tooltip::from_tauri_tokens(
            "hi",
            style_tokens::PALETTE_DARK,
            style_tokens::RADIUS,
            style_tokens::SPACING,
        );
        assert_eq!(t.background, style_tokens::PALETTE_DARK.tooltip_bg);
        assert_eq!(t.text_color, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(
            t.border_radius,
            BorderRadius::all(style_tokens::RADIUS.tooltip)
        );
        // Wave A: 6 px vertical, 12 px horizontal.
        assert_eq!(t.padding.top, style_tokens::SPACING.s6);
        assert_eq!(t.padding.bottom, style_tokens::SPACING.s6);
        assert_eq!(t.padding.left, style_tokens::SPACING.md);
        assert_eq!(t.padding.right, style_tokens::SPACING.md);
    }
}
