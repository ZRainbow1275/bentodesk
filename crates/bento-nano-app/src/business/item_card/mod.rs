//! Business surface — `ItemCard`, a single tile inside an `ItemGrid`.
//!
//! Visual spec: see `item_card.snap.md`. Two variants (`Standard`,
//! `Wide`) drive both layout direction and column span. The
//! `display_name` helper is the locked port of 1.x `displayName`
//! (strip `.lnk` / `.url` for shortcut files) and is exercised by tests.
//!
//! The native renderer consumes this geometry, hover/press state and naming
//! model directly. `build()` remains the compatibility widget-tree descriptor;
//! icon, label and missing-item painting live in `render::item_cards`.

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
/// First-mount item-card entrance duration. It is derived from the shared
/// morph envelope after reserving the complete visible stagger tail, so the
/// steady renderer takes over at the same terminal frame.
pub const CARD_ENTER_DURATION_MS: u32 =
    CARD_ENTER_MORPH_ENVELOPE_MS - CARD_ENTER_STAGGER_MS * CARD_ENTER_MAX_STAGGER_INDEX as u32;
/// First-mount item-card stagger. The reference uses 30 ms, but the native
/// morph reserves a 50 ms tail across the first six visible slots.
/// Distributing that tail across the first six slots keeps a visible cascade
/// while every card reaches its real terminal state with the panel.
pub const CARD_ENTER_STAGGER_MS: u32 = 10;
/// First-mount item-card base delay. Tauri `ItemCard.tsx` only applies
/// `index * 0.03s`; the parent bento/content layers own their own visibility
/// delays, so the item animation itself has no extra base delay.
pub const CARD_ENTER_START_DELAY_MS: u32 = 0;
/// Expanded-panel morph envelope — follows the single selected-stack
/// pill-to-panel wall-clock duration so item content cannot complete early while
/// the panel geometry is still catching up to the video reference.
pub const CARD_ENTER_MORPH_ENVELOPE_MS: u32 = crate::zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS;
/// Last stagger slot that can still finish inside the morph envelope.
pub const CARD_ENTER_MAX_STAGGER_INDEX: usize = 5;
/// Tauri `@keyframes itemEnter` starts at `translateY(6px)`.
pub const CARD_ENTER_OFFSET_Y: f32 = 6.0;

/// Staggered first-reveal progress for expanded item cards during the
/// pill-to-panel morph. The first six slots use the release's compact 10 ms
/// stagger and the morph-derived entry duration; later slots share the final slot so
/// they are fully settled by the end of the morph instead of popping when the
/// steady panel path takes over.
#[inline]
pub fn card_enter_progress_for_morph(morph: f32, item_index: usize) -> f32 {
    let morph_ms = morph.clamp(0.0, 1.0) * CARD_ENTER_MORPH_ENVELOPE_MS as f32;
    let stagger_index = item_index.min(CARD_ENTER_MAX_STAGGER_INDEX) as f32;
    let start_ms = CARD_ENTER_START_DELAY_MS as f32 + stagger_index * CARD_ENTER_STAGGER_MS as f32;
    let raw = ((morph_ms - start_ms) / CARD_ENTER_DURATION_MS as f32).clamp(0.0, 1.0);
    crate::animator::ease_out_cubic(raw)
}

/// Vertical offset for Tauri `itemEnter`: 6px at the first frame, 0px when
/// settled.
#[inline]
pub fn card_enter_translate_y(progress: f32) -> f32 {
    CARD_ENTER_OFFSET_Y * (1.0 - progress.clamp(0.0, 1.0))
}

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
    // #2 step 9 (2026-06-02) — ease-out cubic via the single
    // `animator::ease_out_cubic` SSoT (was an inlined `1 - (1 - t)^3` copy whose
    // own comment admitted it "Matches animator::ease_out_cubic"). One curve
    // definition shared by the pill + item-card so they can never drift.
    let eased = crate::animator::ease_out_cubic(raw);
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
    /// Secondary item label text. V21-C3 — Tauri `.item-card__name` uses
    /// `color: var(--text-secondary)` (#C0C0CC opaque).
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
            PALETTE_DARK.text_primary,
            PALETTE_DARK.surface_hover,
            PALETTE_DARK.border_hover,
        )
    }

    /// Build ItemCard chrome from explicit active theme token groups.
    ///
    /// M2 E-03 (2026-05-29) — corrected to Tauri `ItemCard.css` 1:1.
    /// Radius is `--radius-card` = 10 (was `radius.md` = 6); normal bg is
    /// `--surface-subtle` = `rgba(255,255,255,0.03)` (was the warm/opaque
    /// `surface_alt @0.46`); missing bg is softened toward Tauri's
    /// `rgba(239,68,68,0.08)` (was `danger @0.55`, far too strong).
    ///
    /// V21-C3 (2026-06-22) — the card name text is `--text-secondary` =
    /// `#c0c0cc` (Tauri `.item-card__name { color: var(--text-secondary) }`).
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
        icon_text: Color,
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
            icon_text,
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
mod tests;
