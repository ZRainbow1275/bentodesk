//! Wave C (05-20 visual parity) — collapsed zone pill geometry.
//!
//! Tauri 1.2.4 renders each zone in the Main HWND as a capsule "pill"
//! (icon glyph + name + count badge with rounded-rect shadow) by default;
//! hover or click reveals the item grid via the existing expanded path in
//! `render::draw_zones`. Geometry constants live here so the renderer +
//! hit-test + unit tests share one source of truth — Wave A baseline
//! `research/baseline/zone-collapsed-pill.md` and Wave B SSoT
//! `bento_nano_style::tokens::{RADIUS, SPACING, TYPOGRAPHY}`.
//!
//! Spec §3.2 100% self-rolled / spec §8 no new crate deps / spec §10 zero
//! allocation hot-path: every helper here returns `Copy` rects, no `Vec`,
//! no `String`.

use bento_nano_style::tokens::{RADIUS, SPACING, TYPOGRAPHY};
use bento_nano_style::{BorderRadius, Rect};
use bento_nano_zone::{Zone, ZoneId};

/// Layout slot inside the collapsed pill (icon chip, label band, count
/// badge). Caller paints whatever fill + text suits the accent / palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZonePillLayout {
    /// The pill outer rectangle in logical DIPs. Hit-test region.
    pub rect: Rect,
    /// Drop-shadow band (Wave B `SHADOW.zen` outer offset). Painted under
    /// the main pill rect.
    pub shadow_outer: Rect,
    /// Soft surface lift (Wave B `SHADOW.zen_inner`). Painted under the
    /// pill but above `shadow_outer`.
    pub shadow_inner: Rect,
    /// Icon chip rectangle (left-aligned circle / square).
    pub icon: Rect,
    /// Label band (one line of zone title).
    pub label: Rect,
    /// Count badge (item count or stack member count).
    pub badge: Rect,
    /// Wave H2 — status dot at the top-right corner of the pill. Renderer
    /// paints it only when the zone has items (`count > 0`); empty pills
    /// suppress it. Mirrors the Tauri 1.2.4 "filled" indicator.
    pub status_dot: Rect,
    /// Pill corner radius — Wave B `RADIUS.capsule` (24 DIPs).
    pub radius: BorderRadius,
    /// Badge corner radius — Wave B `RADIUS.badge` (10 DIPs).
    pub badge_radius: BorderRadius,
}

/// Default pill height in DIPs. Tauri reference: 36 DIPs (Wave A
/// `zone-collapsed-pill.md`). Pinned constant — Wave A baseline.
pub const PILL_HEIGHT: f32 = 36.0;

/// Minimum total width before the label is clipped.
pub const PILL_MIN_WIDTH: f32 = 96.0;

/// Default visible label width before truncation (~12 ASCII chars at
/// TYPOGRAPHY.md). Keeps the pill horizontally compact next to the badge.
pub const PILL_LABEL_DEFAULT_WIDTH: f32 = 108.0;

/// Icon chip side length in DIPs.
pub const PILL_ICON_SIZE: f32 = 22.0;

/// Count badge minimum width (fits 3-digit count without truncation).
pub const PILL_BADGE_MIN_WIDTH: f32 = 28.0;

/// Count badge height in DIPs.
pub const PILL_BADGE_HEIGHT: f32 = 20.0;

/// Drop-shadow outer offset matching `bento_nano_style::tokens::SHADOW.zen`
/// (y=8, blur=32). Renderer maps this to a translated rect since D2D's
/// shadow effect isn't always available.
pub const PILL_SHADOW_OUTER_DY: f32 = 8.0;

/// Drop-shadow inner lift matching `SHADOW.zen_inner` (y=2, blur=8).
pub const PILL_SHADOW_INNER_DY: f32 = 2.0;

/// Wave H2 — diameter of the top-right "has items" status dot in DIPs.
/// Sized to read at 100 % DPI without crowding the pill chrome (six DIPs
/// is the smallest legible filled disc against PALETTE_DARK.surface_zen).
pub const PILL_STATUS_DOT_SIZE: f32 = 6.0;

/// Wave H2 — inset of the status dot from the pill's top-right corner.
/// Keeps the disc clear of the badge and the capsule curvature.
pub const PILL_STATUS_DOT_INSET: f32 = 6.0;

/// M3 (2026-05-29) — total wall-clock duration of the capsule expand/shrink
/// morph. Tauri ground truth (`animations.css:41-43`) animates `width`,
/// `height`, and `--rad` over **0.5s** with the `cubic-bezier(0.34,1.56,0.64,1)`
/// "spring" overshoot curve, SYMMETRIC for both expand and collapse (it is a
/// CSS `transition`, not a directional keyframe). The pre-M3 160ms value was a
/// stand-in; this brings the live morph to 1:1 pixel parity. The §12 arch spec
/// (mass=1/stiffness=170/damping=26 physics spring + cubic-bezier(0.16,1,0.3,1))
/// is REFUTED by the Tauri source (no physics spring anywhere) — see the M3
/// research note `research/m3-animation-tauri.md`.
pub const ZONE_PILL_ANIM_DURATION_MS: u32 = 500;

// --- M3 (2026-05-29) easeOutBack cubic-bezier solver ----------------------
//
// Tauri's `.spring-expand` size morph uses the CSS easing
// `cubic-bezier(0.34, 1.56, 0.64, 1)`. With control-point P1.y = 1.56 (> 1)
// the curve overshoots its target by ~10% near the parametric x ≈ 0.7 region
// then settles to EXACTLY 1.0 at the endpoint — the "bounce" that makes the
// expanding rect+radius briefly grow past the token target before snapping
// back. We reproduce it 1:1 with the standard parametric-x Newton-Raphson
// inversion: a CSS cubic-bezier maps an input *x* (time fraction) to an
// output *y* (progress) via an intermediate bezier parameter `u`, where the
// fixed endpoints are P0 = (0,0), P3 = (1,1). We solve `bezier_x(u) = x` for
// `u`, then evaluate `bezier_y(u)`.
//
// Stack-only / zero-alloc (spec §10): all scalars, no `Vec`/`String`/`Box`,
// no panic forms (spec §11). The solver runs a handful of Newton iterations
// with a bisection fallback so it is total for any finite input.

/// X coordinate of the first cubic-bezier control point (P1.x) for the Tauri
/// `.spring-expand` SIZE easing `cubic-bezier(0.34, 1.56, 0.64, 1)`
/// (`animations.css:41-43`).
const BEZIER_P1X: f32 = 0.34;
/// Y coordinate of the first control point (P1.y) — the `1.56` overshoot.
const BEZIER_P1Y: f32 = 1.56;
/// X coordinate of the second control point (P2.x).
const BEZIER_P2X: f32 = 0.64;
/// Y coordinate of the second control point (P2.y).
const BEZIER_P2Y: f32 = 1.0;

/// B2 (2026-05-29) — control points for the CSS-standard `ease` keyword,
/// `cubic-bezier(0.25, 0.1, 0.25, 1)`. Tauri's `.spring-expand` drives
/// `background`/`border-color` on a SEPARATE 300ms `ease` timeline
/// (`animations.css:44-45`), distinct from the 500ms back-curve size morph.
const EASE_STD_P1X: f32 = 0.25;
const EASE_STD_P1Y: f32 = 0.10;
const EASE_STD_P2X: f32 = 0.25;
const EASE_STD_P2Y: f32 = 1.0;

/// Evaluate one axis of a cubic Bézier with fixed endpoints 0 and 1 at
/// parameter `u ∈ [0,1]`. `c1`/`c2` are that axis' two control values.
/// `B(u) = 3(1-u)²u·c1 + 3(1-u)u²·c2 + u³` (the `(1-u)³·0` term drops out).
#[inline]
fn bezier_axis(u: f32, c1: f32, c2: f32) -> f32 {
    let inv = 1.0 - u;
    3.0 * inv * inv * u * c1 + 3.0 * inv * u * u * c2 + u * u * u
}

/// Derivative of [`bezier_axis`] w.r.t. `u` — used by Newton-Raphson to invert
/// the x-axis. `B'(u) = 3(1-u)²·c1 + 6(1-u)u·(c2-c1) + 3u²·(1-c2)`.
#[inline]
fn bezier_axis_derivative(u: f32, c1: f32, c2: f32) -> f32 {
    let inv = 1.0 - u;
    3.0 * inv * inv * c1 + 6.0 * inv * u * (c2 - c1) + 3.0 * u * u * (1.0 - c2)
}

/// Invert the bezier x-axis: find `u` such that `bezier_x(u) == x` for a given
/// time fraction `x ∈ [0,1]`, where `c1x`/`c2x` are the two x control points.
/// Newton-Raphson seeded at `u = x` (x grows monotonically so the seed is
/// always close), with a bisection fallback when the derivative is near zero.
/// Stack-only, total, no panics. Shared by [`ease_out_back_progress`] (the
/// 500ms size curve) and [`ease_standard_progress`] (the 300ms color curve).
#[inline]
fn bezier_solve_x(x: f32, c1x: f32, c2x: f32) -> f32 {
    let target = x.clamp(0.0, 1.0);
    let mut u = target;
    // Newton-Raphson — a handful of iterations converges to f32 precision.
    let mut i = 0;
    while i < 8 {
        let fx = bezier_axis(u, c1x, c2x) - target;
        if fx.abs() < 1e-6 {
            return u.clamp(0.0, 1.0);
        }
        let dfx = bezier_axis_derivative(u, c1x, c2x);
        if dfx.abs() < 1e-6 {
            break;
        }
        u -= fx / dfx;
        i += 1;
    }
    // Bisection fallback — guaranteed to bracket because bezier_x is monotone
    // on [0,1] for these control points (0 < c1x,c2x < 1).
    let mut lo = 0.0_f32;
    let mut hi = 1.0_f32;
    let mut mid = target;
    let mut j = 0;
    while j < 24 {
        mid = (lo + hi) * 0.5;
        let fx = bezier_axis(mid, c1x, c2x) - target;
        if fx.abs() < 1e-6 {
            break;
        }
        if fx > 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
        j += 1;
    }
    mid.clamp(0.0, 1.0)
}

/// M3 (2026-05-29) — Tauri `.spring-expand` easeOutBack progress curve.
///
/// Reproduces CSS `cubic-bezier(0.34, 1.56, 0.64, 1)` 1:1. Input `progress`
/// is the linear time fraction (0..1); the return value is the eased
/// 0..1 morph factor and **overshoots ~10% past 1.0 mid-animation** (around
/// the input region 0.6..0.85) before settling to **EXACTLY 1.0 at t=1.0**.
/// Fed to [`morph_pill_to_rect`] / [`morph_pill_radius`] so the expanding
/// rect+radius briefly bulge past the token target then snap back — matching
/// the Tauri capsule<->panel "spring" feel. Symmetric: the same curve drives
/// expand and collapse (Tauri applies it as a `transition`, not a keyframe).
#[inline]
pub fn ease_out_back_progress(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    // Endpoints are exact by construction — short-circuit so t=1 lands on
    // precisely 1.0 with no Newton residual.
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let u = bezier_solve_x(t, BEZIER_P1X, BEZIER_P2X);
    bezier_axis(u, BEZIER_P1Y, BEZIER_P2Y)
}

/// B2 (2026-05-29) — CSS-standard `ease` progress curve,
/// `cubic-bezier(0.25, 0.1, 0.25, 1)`.
///
/// Tauri's `.spring-expand` drives `background 0.3s ease, border-color 0.3s
/// ease` (`animations.css:44-45`) — a SEPARATE 300ms color timeline on the
/// plain CSS `ease` keyword, NOT the 500ms easeOutBack size curve. This fn
/// reproduces that `ease` keyword 1:1 via the shared bezier solver. Input
/// `progress` is the linear time fraction (0..1) ALONG THE COLOR TIMELINE
/// (the caller already remaps the 500ms morph fraction into the 300ms span);
/// the return value is the eased 0..1 color factor. No overshoot — monotone,
/// endpoints EXACTLY 0.0 / 1.0. Stack-only / zero-alloc (spec §10), no panics
/// (spec §11).
#[inline]
pub fn ease_standard_progress(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let u = bezier_solve_x(t, EASE_STD_P1X, EASE_STD_P2X);
    bezier_axis(u, EASE_STD_P1Y, EASE_STD_P2Y)
}

/// Linearly interpolate between the collapsed pill rect and the expanded
/// zone rect using a morph factor. `morph = 0` → pill, `morph = 1` →
/// expanded body. Pure / allocation-free.
///
/// M3 (2026-05-29) — the lower bound is clamped at 0.0 but the **upper bound
/// is NOT clamped** so [`ease_out_back_progress`]'s ~10% overshoot (morph
/// transiently > 1.0) flows through and the rect briefly grows past the
/// expanded target before the curve settles back to exactly 1.0. A negative
/// `morph` still pins to the pill.
pub fn morph_pill_to_rect(pill: Rect, expanded: Rect, morph: f32) -> Rect {
    let t = morph.max(0.0);
    let inv = 1.0 - t;
    Rect {
        x: pill.x * inv + expanded.x * t,
        y: pill.y * inv + expanded.y * t,
        width: pill.width * inv + expanded.width * t,
        height: pill.height * inv + expanded.height * t,
    }
}

/// Morph the pill corner radius (capsule, 24px) toward the expanded surface
/// radius supplied by `expanded_radius`. Used by the renderer so the chrome
/// "uncurls" smoothly during the expand transition.
///
/// M3 — like [`morph_pill_to_rect`], the upper bound is left un-clamped so the
/// easeOutBack overshoot perturbs the radius in lockstep with the rect; only
/// the lower bound is pinned at 0.0.
pub fn morph_pill_radius(pill_radius: f32, expanded_radius: f32, morph: f32) -> f32 {
    let t = morph.max(0.0);
    pill_radius * (1.0 - t) + expanded_radius * t
}

/// Build a pill layout anchored at `(zone.x, zone.y)`. `count` is the badge
/// number (item count for a regular zone or stack-member count for an
/// anchor). The returned `rect` is the pill's outer hit-test region.
///
/// Pure / allocation-free / `Copy` output. Safe to call every frame.
pub fn pill_layout_for_zone(zone: &Zone, count: usize) -> ZonePillLayout {
    let pad_horizontal = SPACING.md; // 12 DIPs left/right inset
    let pad_inner = SPACING.s6; // 6 DIPs between icon/label/badge
    let badge_width = badge_width_for_count(count);
    let label_width = PILL_LABEL_DEFAULT_WIDTH;
    let total_width = (pad_horizontal * 2.0)
        + PILL_ICON_SIZE
        + pad_inner
        + label_width
        + pad_inner
        + badge_width;
    let width = total_width.max(PILL_MIN_WIDTH);
    let x = zone.x as f32;
    let y = zone.y as f32;
    let rect = Rect {
        x,
        y,
        width,
        height: PILL_HEIGHT,
    };
    let shadow_outer = Rect {
        x: rect.x,
        y: rect.y + PILL_SHADOW_OUTER_DY,
        width: rect.width,
        height: rect.height,
    };
    let shadow_inner = Rect {
        x: rect.x,
        y: rect.y + PILL_SHADOW_INNER_DY,
        width: rect.width,
        height: rect.height,
    };
    let icon_y = rect.y + (PILL_HEIGHT - PILL_ICON_SIZE) * 0.5;
    let icon = Rect {
        x: rect.x + pad_horizontal,
        y: icon_y,
        width: PILL_ICON_SIZE,
        height: PILL_ICON_SIZE,
    };
    let label_x = icon.x + icon.width + pad_inner;
    let label_h = TYPOGRAPHY.md.size_px * TYPOGRAPHY.md.line_height;
    let label = Rect {
        x: label_x,
        y: rect.y + (PILL_HEIGHT - label_h) * 0.5,
        width: label_width,
        height: label_h,
    };
    let badge_y = rect.y + (PILL_HEIGHT - PILL_BADGE_HEIGHT) * 0.5;
    let badge = Rect {
        x: label.x + label.width + pad_inner,
        y: badge_y,
        width: badge_width,
        height: PILL_BADGE_HEIGHT,
    };
    // Wave H2 — status dot inset from the pill's top-right corner. The
    // renderer paints it only when `count > 0` so empty zones stay clean.
    let status_dot = Rect {
        x: rect.right() - PILL_STATUS_DOT_INSET - PILL_STATUS_DOT_SIZE,
        y: rect.y + PILL_STATUS_DOT_INSET,
        width: PILL_STATUS_DOT_SIZE,
        height: PILL_STATUS_DOT_SIZE,
    };
    ZonePillLayout {
        rect,
        shadow_outer,
        shadow_inner,
        icon,
        label,
        badge,
        status_dot,
        radius: BorderRadius::all(RADIUS.capsule),
        badge_radius: BorderRadius::all(RADIUS.badge),
    }
}

/// Smallest badge width that fits `count` digits (plus default-min padding).
pub fn badge_width_for_count(count: usize) -> f32 {
    let digits = digit_count(count);
    let per_digit = TYPOGRAPHY.xs.size_px * 0.62;
    let raw = (digits as f32) * per_digit + SPACING.md;
    raw.max(PILL_BADGE_MIN_WIDTH)
}

/// True when `(x, y)` falls within the pill's hit-test region (the outer
/// `rect`, not the shadow extents).
pub fn pill_hit(layout: &ZonePillLayout, x: f32, y: f32) -> bool {
    rect_contains(layout.rect, x, y)
}

fn digit_count(value: usize) -> u32 {
    if value < 10 {
        1
    } else if value < 100 {
        2
    } else if value < 1000 {
        3
    } else {
        4
    }
}

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom()
}

// --- A3 (2026-05-29) auto-return grace state machine ----------------------
//
// Tauri (`BentoZone.tsx`) does NOT expand/collapse the instant the cursor
// crosses a zone edge — it runs hover-intent + grace timers so transient
// pointer twitches and the 500ms overshoot can't race the open/close:
//
//   * HOVER-INTENT: entering a collapsed zone schedules an expand `now +
//     expand_delay_ms` (Tauri default 150). Leaving before it fires cancels.
//   * EXPAND-LOCK: when an expand fires it sets `expand_lock_until = now +
//     EXPAND_LOCK_MS` (Tauri 550 "normal" path) so a transient leave during
//     the overshoot can't race-collapse.
//   * GRACE COLLAPSE: leaving an expanded zone schedules a collapse at
//     `max(now + collapse_delay_ms (Tauri 300), expand_lock_until)`.
//     Re-entering before it fires cancels.
//
// This struct is the PURE, allocation-free, unit-testable core (spec §10/§11)
// driven by frame-tick `GetTickCount` timestamps from the shell — NO
// `WM_TIMER`, no thread, no clock access inside the struct. The shell feeds
// it `on_enter` / `on_leave` events and polls `poll(now)` once per frame.

/// Tauri-parity expand-lock window (ms) applied when an expand fires, so a
/// transient cursor leave during the 500ms easeOutBack overshoot cannot
/// race-collapse the zone. Tauri's "normal" (non-velocity) path uses 550ms
/// (`BentoZone.tsx:415`, `fastPath ? 300 : 550`); we take the 550 normal path.
pub const EXPAND_LOCK_MS: u32 = 550;

/// Action the [`HoverScheduler`] asks the shell to perform on a `poll`. The
/// shell maps `Expand`/`Collapse` onto the existing `update_zone_pill_hover`
/// morph triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverAction {
    /// Nothing due this frame.
    None,
    /// The hover-intent delay elapsed while the cursor stayed inside — expand
    /// the carried zone.
    Expand(ZoneId),
    /// The grace delay (and any expand-lock) elapsed while the cursor stayed
    /// outside — collapse the carried zone back to its pill.
    Collapse(ZoneId),
}

/// Pure hover/grace scheduler. One instance per process tracks the single
/// zone the pointer is currently interacting with (nano only expands one
/// zone at a time). All timestamps are raw `GetTickCount` ms supplied by the
/// caller; the struct never reads a clock itself, which makes every
/// transition deterministically testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoverScheduler {
    /// Zone with an armed expand-intent timer (cursor inside a collapsed
    /// zone), and the tick at which the expand should fire.
    expand_zone: Option<ZoneId>,
    expand_pending_at_ms: u32,
    /// Zone currently expanded (or expanding) under this scheduler.
    expanded_zone: Option<ZoneId>,
    /// Tick before which a collapse must not fire (set when an expand fires).
    expand_lock_until_ms: u32,
    /// Zone with an armed collapse-grace timer (cursor left an expanded
    /// zone), and the tick at which the collapse should fire.
    collapse_zone: Option<ZoneId>,
    collapse_pending_at_ms: u32,
}

impl Default for HoverScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl HoverScheduler {
    pub const fn new() -> Self {
        Self {
            expand_zone: None,
            expand_pending_at_ms: 0,
            expanded_zone: None,
            expand_lock_until_ms: 0,
            collapse_zone: None,
            collapse_pending_at_ms: 0,
        }
    }

    /// True while any expand-intent or collapse-grace timer is armed — the
    /// shell uses this to keep the frame pump alive until the timer resolves.
    #[inline]
    pub fn is_pending(&self) -> bool {
        self.expand_zone.is_some() || self.collapse_zone.is_some()
    }

    /// The zone the scheduler currently considers expanded (if any).
    #[inline]
    pub fn expanded_zone(&self) -> Option<ZoneId> {
        self.expanded_zone
    }

    /// Cursor entered collapsed `zone`. Arms the hover-intent expand at
    /// `now + expand_delay_ms`. Cancels any pending collapse for that zone
    /// (re-enter aborts the grace). No-op if the zone is already expanded.
    pub fn on_enter(&mut self, zone: ZoneId, now_ms: u32, expand_delay_ms: u32) {
        // Re-entering the zone whose collapse is pending cancels the collapse.
        if self.collapse_zone == Some(zone) {
            self.collapse_zone = None;
        }
        // Already expanded — nothing to schedule.
        if self.expanded_zone == Some(zone) {
            self.expand_zone = None;
            return;
        }
        self.expand_zone = Some(zone);
        self.expand_pending_at_ms = now_ms.wrapping_add(expand_delay_ms);
    }

    /// Cursor left whatever it was over. Clears a pending expand-intent and,
    /// if the carried zone is expanded, arms a collapse at
    /// `max(now + collapse_delay_ms, expand_lock_until)`. `auto_collapse`
    /// gates display-mode: only HOVER mode auto-collapses (Tauri
    /// `BentoZone.tsx:589` — ALWAYS mode is a no-op).
    pub fn on_leave(&mut self, now_ms: u32, collapse_delay_ms: u32, auto_collapse: bool) {
        // A leave always cancels a not-yet-fired expand intent.
        self.expand_zone = None;
        let Some(expanded) = self.expanded_zone else {
            return;
        };
        if !auto_collapse {
            // ALWAYS / pinned mode never auto-collapses on leave.
            return;
        }
        let base = now_ms.wrapping_add(collapse_delay_ms);
        // Defer past the expand-lock window so the overshoot can't be raced:
        // pending = max(base, expand_lock_until). `!reached(base, lock)` means
        // `base` has not yet caught up to the lock deadline (lock is later).
        let pending = if !reached(base, self.expand_lock_until_ms) {
            self.expand_lock_until_ms
        } else {
            base
        };
        self.collapse_zone = Some(expanded);
        self.collapse_pending_at_ms = pending;
    }

    /// Force the scheduler back to a fully idle state (e.g. the pointer left
    /// the whole overlay and the fallback path collapsed everything). Drops
    /// all pending timers and the expanded marker.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Advance one frame at `now_ms`. Returns the action that just became due
    /// (at most one per call) and updates internal state so the action is not
    /// re-emitted. Expand wins over collapse if both somehow resolve on the
    /// same tick (they target different lifecycle states so this is defensive).
    pub fn poll(&mut self, now_ms: u32) -> HoverAction {
        if let Some(zone) = self.expand_zone {
            if reached(now_ms, self.expand_pending_at_ms) {
                self.expand_zone = None;
                self.mark_expanded(zone, now_ms);
                return HoverAction::Expand(zone);
            }
        }
        if let Some(zone) = self.collapse_zone {
            if reached(now_ms, self.collapse_pending_at_ms) {
                self.collapse_zone = None;
                if self.expanded_zone == Some(zone) {
                    self.expanded_zone = None;
                }
                return HoverAction::Collapse(zone);
            }
        }
        HoverAction::None
    }

    /// Record that `zone` is now expanded and arm the expand-lock window so a
    /// transient leave during the overshoot defers the collapse. Called from
    /// `poll` when a hover-intent fires; also exposed for the shell when an
    /// expand is forced through a path other than the intent timer (e.g. a
    /// click or a direct zone-to-zone hand-off).
    pub fn mark_expanded(&mut self, zone: ZoneId, now_ms: u32) {
        self.expanded_zone = Some(zone);
        self.expand_lock_until_ms = now_ms.wrapping_add(EXPAND_LOCK_MS);
        self.expand_zone = None;
        // A fresh expand cancels any stale collapse for the same zone.
        if self.collapse_zone == Some(zone) {
            self.collapse_zone = None;
        }
    }
}

/// Monotone-ish "now has reached deadline" test tolerant of `GetTickCount`
/// wraparound (every ~49.7 days). Treats the unsigned wrap distance: if
/// `now - deadline` is small-positive (< half the u32 range) the deadline
/// has passed.
#[inline]
fn reached(now_ms: u32, deadline_ms: u32) -> bool {
    now_ms.wrapping_sub(deadline_ms) < (u32::MAX / 2)
}

// Unit + state-machine tests live in the sibling `tests.rs` to keep this
// production module within the §15 800-line budget.
#[cfg(test)]
mod tests;
