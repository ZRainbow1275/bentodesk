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
use bento_nano_zone::{ZoneId, ZoneItemId};
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

// --- A2 (M3 2026-05-29) item-card hover / press scale ---------------------
//
// Tauri ground truth `ItemCard/ItemCard.css:20-31`:
//   .item-card:hover  { transform: translateY(-1px) scale(1.02); }
//   .item-card:active { transform: scale(0.97); transition: ...80ms; }
// (Resolution #3: unlike the collapsed PILL — nano V-12, no scale — the item
// card DOES scale on hover/press.) These constants + `card_scale_for` are the
// single source of truth for the multiplier; the renderer applies it at paint
// time via a centred scale, leaving the persisted card geometry untouched
// (mirrors the pill animator's V-8 contract). Pure / stack-only (spec §10).

/// Hover scale multiplier — Tauri `scale(1.02)` (+2%).
pub const CARD_HOVER_SCALE: f32 = 1.02;
/// Press/active scale multiplier — Tauri `scale(0.97)` (-3%).
pub const CARD_PRESS_SCALE: f32 = 0.97;
/// Press transition duration — Tauri `:active { transition-duration: 80ms }`
/// (`ItemCard.css:30`).
pub const CARD_PRESS_DURATION_MS: u32 = 80;
/// Hover transition duration — Tauri `.item-card { transition: all
/// var(--transition-fast) }` where `--transition-fast: 150ms ease-out`
/// (`variables.css:68`). The `:hover` `scale(1.02)` rides this 150ms ease-out
/// timeline; the SSoT's "80ms" applies only to the `:active` press override.
pub const CARD_HOVER_DURATION_MS: u32 = 150;
/// Hover-lift vertical offset — Tauri `translateY(-1px)`.
pub const CARD_HOVER_LIFT_DY: f32 = -1.0;

/// Compose hover + press into the item-card scale multiplier. `hover_t` and
/// `press_t` are 0..1 animator progress values (e.g. from a future per-item
/// hover channel). Press takes precedence when both are active — Tauri's
/// `:active` overrides `:hover` (the card shrinks under the pointer-down even
/// while hovered). Returns 1.0 when idle.
#[inline]
pub fn card_scale_for(hover_t: f32, press_t: f32) -> f32 {
    let h = hover_t.clamp(0.0, 1.0);
    let p = press_t.clamp(0.0, 1.0);
    // Hover inflates toward 1.02; press then deflates toward 0.97. Compose
    // multiplicatively so a mid-hover press still reads as a relative shrink.
    let hover_scale = 1.0 + h * (CARD_HOVER_SCALE - 1.0);
    let press_scale = 1.0 + p * (CARD_PRESS_SCALE - 1.0);
    hover_scale * press_scale
}

/// Ease-out-cubic ramp progress over `duration_ms`, mirroring Tauri's
/// `transition: ... ease-out`. `rising` true ramps 0→1 (hover/press enter);
/// false ramps 1→0 (leave/release). `elapsed_ms` is `now - started` from the
/// shell's `GetTickCount` cadence (wrap-safe at the caller). Pure / stack-only
/// (spec §10) so it unit-tests without a clock or animator instance.
#[inline]
pub fn card_ramp_t(elapsed_ms: u32, duration_ms: u32, rising: bool) -> f32 {
    let d = duration_ms.max(1) as f32;
    let raw = (elapsed_ms as f32 / d).clamp(0.0, 1.0);
    // Ease-out cubic — `1 - (1 - t)^3`, the decelerating curve CSS `ease-out`
    // approximates. Matches `animator::ease_out_cubic` so item + pill feel
    // identical.
    let inv = 1.0 - raw;
    let eased = 1.0 - inv * inv * inv;
    if rising { eased } else { 1.0 - eased }
}

/// Per-item hover / press animation state — the live wiring of the
/// `card_scale_for` SSoT (M3-A2, 2026-05-29). The CSS `.item-card:hover`
/// transform is bidirectional: the entering card ramps up while the just-left
/// card ramps down. nano's pointer is over at most one card at a time, so a
/// fixed three-slot record (entering / leaving / pressed) covers every visible
/// transition without a per-item map — O(1), `Copy`, zero-alloc (spec §10), and
/// stored in a `Cell` on `AppState` exactly like `HoverScheduler`.
///
/// Timestamps are raw `GetTickCount` ms supplied by the shell's frame/move
/// cadence; the struct never reads a clock itself, which keeps every ramp
/// deterministically testable. The persisted card geometry is NEVER mutated —
/// the renderer applies `card_scale_for` as a centred draw-time scale (V-8
/// contract) and the V-13 hit-rect stays on the base, unscaled rect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemHoverState {
    /// Card the pointer is currently over (hover ramping toward 1.0).
    hovered: Option<(ZoneId, ZoneItemId)>,
    hover_started_ms: u32,
    /// Card the pointer just left (hover ramping back toward 0.0). Distinct
    /// from `hovered` so the CSS leave-transition still animates the prior
    /// card down while the new one animates up.
    leaving: Option<(ZoneId, ZoneItemId)>,
    leave_started_ms: u32,
    /// Card under an active pointer-down (press ramping toward 1.0), or the
    /// card whose press is releasing (ramping back toward 0.0) when
    /// `press_down` is false.
    pressed: Option<(ZoneId, ZoneItemId)>,
    press_started_ms: u32,
    press_down: bool,
}

impl ItemHoverState {
    pub const fn new() -> Self {
        Self {
            hovered: None,
            hover_started_ms: 0,
            leaving: None,
            leave_started_ms: 0,
            pressed: None,
            press_started_ms: 0,
            press_down: false,
        }
    }

    /// Pointer moved onto `card` (or off everything when `card` is `None`).
    /// Returns `true` when the hovered target actually changed, so the shell
    /// can request a redraw and keep the frame pump alive for the ramp.
    pub fn on_hover(&mut self, card: Option<(ZoneId, ZoneItemId)>, now_ms: u32) -> bool {
        if self.hovered == card {
            return false;
        }
        // The previously-hovered card becomes the leaving (ramp-down) card so
        // its hover-out still animates. A brand-new hover with no prior card
        // simply clears the leaving slot.
        if let Some(prev) = self.hovered {
            self.leaving = Some(prev);
            self.leave_started_ms = now_ms;
        } else {
            self.leaving = None;
        }
        self.hovered = card;
        self.hover_started_ms = now_ms;
        true
    }

    /// Pointer-down landed on `card`. Starts the press ramp toward 0.97.
    pub fn on_press(&mut self, card: (ZoneId, ZoneItemId), now_ms: u32) {
        self.pressed = Some(card);
        self.press_started_ms = now_ms;
        self.press_down = true;
    }

    /// Pointer-up anywhere. Starts the press release ramp back toward 1.0 for
    /// whatever card was held. No-op when nothing was pressed.
    pub fn on_release(&mut self, now_ms: u32) -> bool {
        if self.pressed.is_none() || !self.press_down {
            return false;
        }
        self.press_started_ms = now_ms;
        self.press_down = false;
        true
    }

    /// Per-frame retire pass. Drops the leaving card once its hover-out ramp
    /// completes and drops a fully-released press, so a never-ending entry
    /// can't pin the frame pump. Returns `true` while any ramp is still in
    /// flight (the shell keeps requesting redraws until then).
    pub fn tick(&mut self, now_ms: u32) -> bool {
        let mut active = false;
        if let Some(_card) = self.leaving {
            if now_ms.wrapping_sub(self.leave_started_ms) >= CARD_HOVER_DURATION_MS {
                self.leaving = None;
            } else {
                active = true;
            }
        }
        if self.pressed.is_some() {
            let elapsed = now_ms.wrapping_sub(self.press_started_ms);
            if !self.press_down && elapsed >= CARD_PRESS_DURATION_MS {
                self.pressed = None;
            } else {
                active = true;
            }
        }
        // The held hover card keeps ramping until it reaches 1.0; after that it
        // stays pinned (no redraw needed) but does not count as "active".
        if self.hovered.is_some()
            && now_ms.wrapping_sub(self.hover_started_ms) < CARD_HOVER_DURATION_MS
        {
            active = true;
        }
        active
    }

    /// Sample the `(hover_t, press_t)` 0..1 ramp pair for `card` at `now_ms`.
    /// Returns `(0.0, 0.0)` for any card not currently animating, so the
    /// renderer can call it per item and apply `card_scale_for` — identity
    /// scale for idle cards.
    #[inline]
    pub fn sample(&self, card: (ZoneId, ZoneItemId), now_ms: u32) -> (f32, f32) {
        let hover_t = if self.hovered == Some(card) {
            card_ramp_t(
                now_ms.wrapping_sub(self.hover_started_ms),
                CARD_HOVER_DURATION_MS,
                true,
            )
        } else if self.leaving == Some(card) {
            card_ramp_t(
                now_ms.wrapping_sub(self.leave_started_ms),
                CARD_HOVER_DURATION_MS,
                false,
            )
        } else {
            0.0
        };
        let press_t = if self.pressed == Some(card) {
            card_ramp_t(
                now_ms.wrapping_sub(self.press_started_ms),
                CARD_PRESS_DURATION_MS,
                self.press_down,
            )
        } else {
            0.0
        };
        (hover_t, press_t)
    }

    /// True while `card` is the card under an actively-held pointer-down
    /// (press entered, not yet released). FIX 1 (M3-A3) uses this to DROP the
    /// hover translateY(-1px) lift while pressed — Tauri's `.item-card:active`
    /// respecifies `transform: scale(0.97)` (scale-only), so per CSS
    /// specificity the inherited `:hover` `translateY(-1px)` is overwritten
    /// while the pointer is held. On release (`press_down` false) the lift
    /// returns even as the press scale ramps back out.
    #[inline]
    pub fn press_held(&self, card: (ZoneId, ZoneItemId)) -> bool {
        self.press_down && self.pressed == Some(card)
    }

    /// True when any ramp is in flight — lets the shell keep the frame pump
    /// alive without mutating state (read-only companion to `tick`).
    #[inline]
    pub fn is_active(&self, now_ms: u32) -> bool {
        if self.leaving.is_some()
            && now_ms.wrapping_sub(self.leave_started_ms) < CARD_HOVER_DURATION_MS
        {
            return true;
        }
        if self.hovered.is_some()
            && now_ms.wrapping_sub(self.hover_started_ms) < CARD_HOVER_DURATION_MS
        {
            return true;
        }
        if self.pressed.is_some() {
            let elapsed = now_ms.wrapping_sub(self.press_started_ms);
            if self.press_down || elapsed < CARD_PRESS_DURATION_MS {
                return true;
            }
        }
        false
    }
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
    /// M3-A3 hover background — Tauri `:hover { background: var(--surface-hover) }`.
    /// The renderer lerps `normal_background` → this by `hover_t`.
    pub hover_background: Color,
    /// M3-A3 hover border — Tauri `:hover { border-color: var(--border-hover) }`.
    /// A 1px stroke whose alpha lerps transparent → this by `hover_t`.
    pub hover_border: Color,
    /// M3-A3 hover drop shadow — Tauri `--shadow-item-hover` outer layer
    /// `0 2px 8px rgba(0,0,0,0.12)` (offset_y 2, blur 8, alpha 0.12). Painted
    /// behind the card at alpha scaled by `hover_t`.
    pub hover_shadow_outer: Color,
    /// M3-A3 hover drop shadow — Tauri `--shadow-item-hover` ambient layer
    /// `0 8px 24px rgba(0,0,0,0.08)` (offset_y 8, blur 24, alpha 0.08).
    pub hover_shadow_inner: Color,
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
            PALETTE_DARK.surface_hover,
            PALETTE_DARK.border_hover,
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
        surface_hover: Color,
        border_hover: Color,
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
            // M3-A3 — Tauri `--shadow-item-hover` is theme-independent
            // (black at fixed alphas in BOTH dark/light variables.css; the
            // dark stack is the parity target). `--surface-hover` /
            // `--border-hover` arrive from the live `PaletteTauri` so the
            // hover chrome re-skins with the active theme.
            hover_background: surface_hover,
            hover_border: border_hover,
            // 0 2px 8px rgba(0,0,0,0.12)
            hover_shadow_outer: Color::from_u8(0x00, 0x00, 0x00, 0x1F),
            // 0 8px 24px rgba(0,0,0,0.08)
            hover_shadow_inner: Color::from_u8(0x00, 0x00, 0x00, 0x14),
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
        // M3-A3 (2026-05-29) — corrected to Tauri `ItemCard.css` 1:1.
        // Standard `.item-card { padding: 8px 4px }` (vert 8 / horiz 4);
        // Wide `.item-card--wide { padding: 10px 12px }` (vert 10 / horiz 12).
        // (Was a symmetric 6 / 6+8 placeholder.)
        padding: match variant {
            CardVariant::Standard => Edges {
                top: 8.0,
                right: 4.0,
                bottom: 8.0,
                left: 4.0,
            },
            CardVariant::Wide => Edges {
                top: 10.0,
                right: 12.0,
                bottom: 10.0,
                left: 12.0,
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
    fn card_scale_idle_is_identity() {
        assert!((card_scale_for(0.0, 0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn card_scale_hover_inflates_to_tauri_1_02() {
        // Full hover, no press → scale(1.02).
        assert!((card_scale_for(1.0, 0.0) - CARD_HOVER_SCALE).abs() < 1e-5);
        // Half hover sits between 1.0 and 1.02.
        let half = card_scale_for(0.5, 0.0);
        assert!(half > 1.0 && half < CARD_HOVER_SCALE);
    }

    #[test]
    fn card_scale_press_deflates_to_tauri_0_97() {
        // Full press, no hover → scale(0.97).
        assert!((card_scale_for(0.0, 1.0) - CARD_PRESS_SCALE).abs() < 1e-5);
    }

    #[test]
    fn card_scale_press_overrides_hover_to_a_net_shrink() {
        // Pressing while hovered must read as a relative shrink (< the hover
        // peak) — Tauri `:active` overrides `:hover`.
        let pressed_while_hovered = card_scale_for(1.0, 1.0);
        assert!(pressed_while_hovered < CARD_HOVER_SCALE);
        // 1.02 * 0.97 = 0.9894 — below 1.0, a visible shrink.
        assert!((pressed_while_hovered - (CARD_HOVER_SCALE * CARD_PRESS_SCALE)).abs() < 1e-5);
        assert!(pressed_while_hovered < 1.0);
    }

    #[test]
    fn card_press_duration_matches_tauri_80ms() {
        assert_eq!(CARD_PRESS_DURATION_MS, 80);
    }

    #[test]
    fn card_hover_duration_matches_tauri_transition_fast_150ms() {
        // Tauri `.item-card { transition: all var(--transition-fast) }`,
        // `--transition-fast: 150ms ease-out`.
        assert_eq!(CARD_HOVER_DURATION_MS, 150);
    }

    #[test]
    fn card_ramp_endpoints_and_easeout_shape() {
        // Rising: 0 at start, 1 at/after the full duration, decelerating
        // (past linear at the midpoint).
        assert!(card_ramp_t(0, 150, true).abs() < 1e-5);
        assert!((card_ramp_t(150, 150, true) - 1.0).abs() < 1e-5);
        assert!((card_ramp_t(300, 150, true) - 1.0).abs() < 1e-5); // clamps
        let mid = card_ramp_t(75, 150, true);
        assert!(mid > 0.5 && mid < 1.0); // ease-out is ahead of linear 0.5
        // Falling is the mirror: 1 at start, 0 at the end.
        assert!((card_ramp_t(0, 150, false) - 1.0).abs() < 1e-5);
        assert!(card_ramp_t(150, 150, false).abs() < 1e-5);
        // Rising + falling at the same elapsed sum to 1 (continuous reversal).
        assert!((card_ramp_t(75, 150, true) + card_ramp_t(75, 150, false) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn card_ramp_zero_duration_does_not_div_by_zero() {
        // `duration_ms.max(1)` guards the divide. With a 0ms duration the
        // 1ms floor means any elapsed >= 1 reads as fully complete (rising →
        // 1.0, falling → 0.0); the start sample at elapsed 0 is still the
        // ramp origin. The key invariant is "no panic / no NaN".
        assert!(card_ramp_t(0, 0, true).abs() < 1e-5);
        assert!((card_ramp_t(1, 0, true) - 1.0).abs() < 1e-5);
        assert!((card_ramp_t(1, 0, false)).abs() < 1e-5);
        assert!(card_ramp_t(10, 0, true).is_finite());
    }

    fn card(z: u64, i: u64) -> (ZoneId, ZoneItemId) {
        (ZoneId(z), ZoneItemId(i))
    }

    #[test]
    fn item_hover_state_idle_samples_identity() {
        let st = ItemHoverState::new();
        let (h, p) = st.sample(card(1, 1), 1_000);
        assert!(h.abs() < 1e-6 && p.abs() < 1e-6);
        assert!((card_scale_for(h, p) - 1.0).abs() < 1e-6);
        assert!(!st.is_active(1_000));
    }

    #[test]
    fn item_hover_enter_ramps_up_and_changes_target() {
        let mut st = ItemHoverState::new();
        assert!(st.on_hover(Some(card(1, 7)), 1_000));
        // Same target again is a no-op (no spurious redraw).
        assert!(!st.on_hover(Some(card(1, 7)), 1_050));
        // Mid-ramp the hovered card is between 0 and 1.
        let (h, _) = st.sample(card(1, 7), 1_000 + 75);
        assert!(h > 0.0 && h < 1.0);
        // Fully ramped after the 150ms window.
        let (h_full, _) = st.sample(card(1, 7), 1_000 + 150);
        assert!((h_full - 1.0).abs() < 1e-5);
        assert!(st.is_active(1_000 + 75));
    }

    #[test]
    fn item_hover_handoff_ramps_prev_down_and_next_up() {
        let mut st = ItemHoverState::new();
        st.on_hover(Some(card(1, 1)), 0); // settle card A up
        let _ = st.sample(card(1, 1), 200);
        st.on_hover(Some(card(1, 2)), 200); // hand off to card B
        // Card A (leaving) ramps down from 1.0; card B (entering) ramps up.
        let (a_h, _) = st.sample(card(1, 1), 200 + 75);
        let (b_h, _) = st.sample(card(1, 2), 200 + 75);
        assert!(a_h > 0.0 && a_h < 1.0);
        assert!(b_h > 0.0 && b_h < 1.0);
        // After the leave window the prior card retires to identity.
        let _ = st.tick(200 + CARD_HOVER_DURATION_MS);
        let (a_done, _) = st.sample(card(1, 1), 200 + CARD_HOVER_DURATION_MS);
        assert!(a_done.abs() < 1e-5);
    }

    #[test]
    fn item_press_ramps_to_tauri_shrink_then_releases() {
        let mut st = ItemHoverState::new();
        st.on_hover(Some(card(2, 5)), 0);
        st.on_press(card(2, 5), 0);
        // Full press → press_t 1.0 → composed scale is the 1.02*0.97 shrink.
        let (h, p) = st.sample(card(2, 5), CARD_HOVER_DURATION_MS.max(CARD_PRESS_DURATION_MS));
        assert!((p - 1.0).abs() < 1e-5);
        let scale = card_scale_for(h, p);
        assert!(scale < 1.0);
        assert!((scale - CARD_HOVER_SCALE * CARD_PRESS_SCALE).abs() < 1e-4);
        // Release ramps press back toward 0; tick retires it after 80ms.
        assert!(st.on_release(200));
        assert!(st.is_active(200 + 10));
        let _ = st.tick(200 + CARD_PRESS_DURATION_MS);
        let (_h2, p2) = st.sample(card(2, 5), 200 + CARD_PRESS_DURATION_MS);
        assert!(p2.abs() < 1e-5);
    }

    #[test]
    fn item_press_only_on_pressed_card() {
        let mut st = ItemHoverState::new();
        st.on_press(card(3, 1), 0);
        // A different card sees no press.
        let (_h, p) = st.sample(card(3, 2), CARD_PRESS_DURATION_MS);
        assert!(p.abs() < 1e-5);
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
            bento_nano_style::tokens::PALETTE_DARK.surface_hover,
            bento_nano_style::tokens::PALETTE_DARK.border_hover,
        );

        assert_eq!(
            chrome.card_radius,
            BorderRadius::all(bento_nano_style::tokens::RADIUS.card)
        );
    }

    #[test]
    fn card_hover_lift_dy_zero_at_idle_minus_one_at_full_hover() {
        // FIX 1 — the renderer offsets `card_rect.y` by `CARD_HOVER_LIFT_DY *
        // hover_t`, mirroring Tauri `:hover { transform: translateY(-1px) }`.
        // At idle (hover_t 0) the lift is 0; at full hover (hover_t 1) it is
        // exactly -1 px; mid-hover lerps linearly. The renderer applies the
        // const directly so this pure check pins the contract the const must
        // satisfy.
        let lift_at = |hover_t: f32| CARD_HOVER_LIFT_DY * hover_t;
        assert!(lift_at(0.0).abs() < 1e-6, "idle card must not lift");
        assert!(
            (lift_at(1.0) - (-1.0)).abs() < 1e-6,
            "full hover must lift exactly -1px"
        );
        // Monotone upward (more negative) as hover ramps in.
        assert!(lift_at(0.5) < 0.0 && lift_at(0.5) > lift_at(1.0));
        assert!((lift_at(0.5) - (-0.5)).abs() < 1e-6);
    }

    #[test]
    fn item_card_chrome_exposes_tauri_hover_chrome_tokens() {
        // FIX 2 — hover background/border come from the live palette; the
        // two-layer `--shadow-item-hover` is theme-independent black at the
        // Tauri alphas (0.12 outer / 0.08 inner).
        let palette = bento_nano_theme::current().palette;
        let chrome = ItemCardChrome::from_tokens(
            palette,
            radius::DEFAULT,
            bento_nano_style::tokens::PALETTE_DARK.surface_subtle,
            bento_nano_style::tokens::PALETTE_DARK.text_secondary,
            bento_nano_style::tokens::PALETTE_DARK.surface_hover,
            bento_nano_style::tokens::PALETTE_DARK.border_hover,
        );
        assert_eq!(
            chrome.hover_background,
            bento_nano_style::tokens::PALETTE_DARK.surface_hover
        );
        assert_eq!(
            chrome.hover_border,
            bento_nano_style::tokens::PALETTE_DARK.border_hover
        );
        // 0.12 * 255 ≈ 31 (0x1F); 0.08 * 255 ≈ 20 (0x14).
        assert_eq!(chrome.hover_shadow_outer, Color::from_u8(0, 0, 0, 0x1F));
        assert_eq!(chrome.hover_shadow_inner, Color::from_u8(0, 0, 0, 0x14));
    }
}
