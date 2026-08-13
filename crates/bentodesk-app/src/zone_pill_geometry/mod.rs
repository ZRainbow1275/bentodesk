//! Wave C (05-20 visual parity) — collapsed zone pill geometry.
//!
//! Tauri 1.2.4 renders each zone in the Main HWND as a capsule "pill"
//! (icon glyph + name + count badge with rounded-rect shadow) by default;
//! hover or click reveals the item grid via the existing expanded path in
//! `render::draw_zones`. Geometry constants live here so the renderer +
//! hit-test + unit tests share one source of truth — Wave A baseline
//! `research/baseline/zone-collapsed-pill.md` and Wave B SSoT
//! `bentodesk_style::tokens::{RADIUS, SPACING, TYPOGRAPHY}`.
//!
//! Spec §3.2 100% self-rolled / spec §8 no new crate deps / spec §10 zero
//! allocation hot-path: every helper here returns `Copy` rects, no `Vec`,
//! no `String`.

use crate::business::zen_capsule::{CapsuleShape, CapsuleSize};
use bentodesk_style::tokens::{RADIUS, SPACING, TYPOGRAPHY};
use bentodesk_style::{BorderRadius, Rect, Size};
use bentodesk_zone::{Zone, ZoneId};

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
    /// Pill corner radius — Wave B `RADIUS.capsule` (24 DIPs).
    pub radius: BorderRadius,
    /// Badge corner radius — Wave B `RADIUS.badge` (10 DIPs).
    pub badge_radius: BorderRadius,
}

/// Return whether a persisted zone icon name should paint a visible glyph.
///
/// The 2026-06-02 reference scene contains at least one capsule with no
/// visible icon. Preserve that as an explicit wire value instead of treating
/// it as an unknown icon and substituting a document glyph.
#[inline]
pub fn icon_name_has_visible_glyph(icon: &str) -> bool {
    !matches!(icon, "" | "none")
}

/// Stack capsule layout slot map, matching the Tauri `StackCapsule` CSS grid:
/// peek icons, main icon bubble, title, and member-count badge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackCapsuleLayout {
    /// The stack capsule outer rectangle in logical DIPs. Hit-test region.
    pub rect: Rect,
    /// Drop-shadow band painted under the main capsule rect.
    pub shadow_outer: Rect,
    /// Soft surface lift painted under the capsule but above `shadow_outer`.
    pub shadow_inner: Rect,
    /// Up to three overlapped member peek icons (`StackCapsule.tsx slice(-3)`).
    pub peek_icons: [Rect; STACK_CAPSULE_MAX_PEEK_ICONS],
    /// Number of `peek_icons` that should be painted.
    pub peek_visible_count: usize,
    /// Main top-zone icon bubble.
    pub icon_bubble: Rect,
    /// Fixed `ZoneIcon size={18}` glyph slot inside `icon_bubble`.
    pub icon_glyph: Rect,
    /// Stack title band.
    pub label: Rect,
    /// Stack member-count badge.
    pub badge: Rect,
    /// Capsule corner radius.
    pub radius: BorderRadius,
    /// Peek icon radius.
    pub peek_radius: BorderRadius,
    /// Main icon bubble radius.
    pub icon_radius: BorderRadius,
    /// Badge corner radius.
    pub badge_radius: BorderRadius,
}

/// Default pill height in DIPs — the **Medium** size tier.
///
/// M2② (2026-05-29) — re-centred on Tauri v1.3.0's `getCapsuleBoxPx` medium
/// box height of **48** (`bentodesk/src/services/hitTest.ts:96`), the
/// authoritative pixel source for the collapsed capsule. The pre-M2② value
/// (36) was the Wave A baseline and is now the Small tier. This constant is
/// the fallback used wherever a `Zone`'s `capsule_size` is not available; the
/// live pill resolves height per-zone via [`CapsuleSize::height_px`] inside
/// [`pill_layout_for_zone`]. Kept in sync with
/// `CapsuleSize::Medium.height_px()` (= 48).
pub const PILL_HEIGHT: f32 = 48.0;

/// Minimum total width before the label is clipped.
pub const PILL_MIN_WIDTH: f32 = 96.0;

/// Legacy label width budget kept for API/back-compat callers. The live Tauri
/// pill no longer expands to this width; [`pill_layout_for_zone`] clamps the
/// outer box to [`CapsuleSize::width_px`] and gives the label only the remaining
/// flex space.
pub const PILL_LABEL_DEFAULT_WIDTH: f32 = 108.0;

/// Icon chip side length in DIPs — legacy Large-tier fallback constant.
///
/// M2② (2026-05-29) — the live pill resolves the icon size per-zone from
/// `zone.capsule_size` via [`CapsuleSize::icon_px`] inside
/// [`pill_layout_for_zone`]; V21-C4 reconciles the live icon box to Tauri's
/// actual fixed `ZoneIcon size={18}` wrapper. This legacy fallback constant is
/// retained for API/back-compat callers and keeps its historical value.
pub const PILL_ICON_SIZE: f32 = 22.0;

/// Video-observed residual width for an explicit no-glyph capsule icon.
///
/// V21-C19 introduced this residual slot for persisted `""` / `"none"` icon
/// values. N187 later proved that the reference `Compiler` uses a real `code`
/// glyph, so the live Compiler path no longer consumes this fallback.
pub const PILL_NO_GLYPH_ICON_SLOT_PX: f32 = 6.0;

/// Video-observed left padding for a large capsule with a visible icon glyph.
///
/// V21-C21: the post-C20 Browser component crop still showed the visible icon
/// and title run about 10 px too far right at 1.5x proof scale. Keep the source
/// CSS padding table intact, but resolve visible-glyph large capsules to this
/// 21-DIP left slot so the Browser icon/title band aligns without changing the
/// explicit no-glyph fallback.
pub const PILL_LARGE_VISIBLE_GLYPH_PAD_LEFT_PX: f32 = 21.0;

/// Video-observed inner gap for a large capsule with a visible icon glyph.
///
/// V21-C22: after C21 aligned the Browser icon's left edge, the component crop
/// still showed the title start 7 px to the right of the reference at 1.5x
/// proof scale. The source CSS `gap` table stays 16 DIPs for large capsules,
/// but visible-glyph large capsules use this 11-DIP runtime gap so Browser's
/// icon/title spacing matches the 2026-06-02 recording. Explicit no-glyph
/// capsules keep the 16-DIP source gap.
pub const PILL_LARGE_VISIBLE_GLYPH_INNER_GAP_PX: f32 = 11.0;

/// Source title letter spacing for ordinary collapsed-pill labels.
pub const PILL_TITLE_TRACKING_PX: f32 = 0.3;
/// Default title alpha for collapsed-pill labels.
pub const PILL_TITLE_ALPHA: f32 = 1.0;

/// Video-observed right inset for Large count badges with visible glyphs.
///
/// V21-C27: after C26, Browser and source-tier `Compiler` large count badges were
/// still right of the 2026-06-02 component crop, but by different amounts.
/// Keep the source CSS large right padding table at 20 DIPs, then use this
/// smaller inset for Browser-style visible-glyph capsules so the blue badge
/// moves left without overshooting the reference.
pub const PILL_LARGE_VISIBLE_GLYPH_BADGE_RIGHT_INSET_PX: f32 = 20.5;

/// Video-observed right inset for Large count badges without visible glyphs.
///
/// The source-tier right-rail `Compiler` chip needs a larger left move than the
/// Browser visible-glyph chip after C26. Keep this separate from the source
/// padding table and the Browser profile.
pub const PILL_LARGE_NO_GLYPH_BADGE_RIGHT_INSET_PX: f32 = 21.5;
/// Width expansion for Large count badges without visible glyphs.
///
/// V21-C30: after C28/C29, the source-tier `Compiler` badge right edge and y span
/// are correct, but the green chip is one device pixel too narrow and starts one
/// pixel too far right. Expanding the width by 0.67 DIP at the fixed right anchor
/// lands the left edge on the 1.5x component crop without moving Browser badges.
pub const PILL_LARGE_NO_GLYPH_BADGE_WIDTH_EXTRA_PX: f32 = 0.67;

/// Video-observed badge height for Large count badges with visible glyphs.
///
/// V21-C28: after C27 aligned Large badge x positions, the Browser visible-glyph
/// chip still measured two device pixels taller than the 2026-06-02 crop while
/// the source-tier `Compiler` chip already matched the 26 px reference span.
/// Keep this profile separate so Browser can use the 16-DIP visual span.
pub const PILL_LARGE_VISIBLE_GLYPH_BADGE_HEIGHT_PX: f32 = 16.0;

/// Video-observed badge height for Large count badges without visible glyphs.
///
/// The right-rail source-tier `Compiler` badge remains on the C23/C26 17-DIP
/// span because its post-C27 bbox already matches the reference height.
pub const PILL_LARGE_NO_GLYPH_BADGE_HEIGHT_PX: f32 = 17.0;

/// Count badge minimum width (fits 3-digit count without truncation).
pub const PILL_BADGE_MIN_WIDTH: f32 = 28.0;

/// Count badge height in DIPs.
pub const PILL_BADGE_HEIGHT: f32 = 20.0;

/// Drop-shadow outer offset matching `bentodesk_style::tokens::SHADOW.zen`
/// (y=8, blur=32). Renderer maps this to a translated rect since D2D's
/// shadow effect isn't always available.
pub const PILL_SHADOW_OUTER_DY: f32 = 8.0;

/// Drop-shadow inner lift matching `SHADOW.zen_inner` (y=2, blur=8).
pub const PILL_SHADOW_INNER_DY: f32 = 2.0;

/// Tauri `StackCapsule.css` visible capsule width (`min-width: 220px`).
pub const STACK_CAPSULE_WIDTH_PX: f32 = 220.0;
/// Tauri `StackCapsule.css` visible capsule height (`min-height: 52px`).
pub const STACK_CAPSULE_HEIGHT_PX: f32 = 52.0;
/// Stack capsule radius (`border-radius: 24px`).
pub const STACK_CAPSULE_RADIUS_PX: f32 = 24.0;
/// Stack capsule horizontal padding (`padding: 10px 12px`).
pub const STACK_CAPSULE_PAD_X_PX: f32 = 12.0;
/// Grid gap between stack capsule columns.
pub const STACK_CAPSULE_GAP_PX: f32 = 10.0;
/// Number of member peek icons visible inside the capsule.
pub const STACK_CAPSULE_MAX_PEEK_ICONS: usize = 3;
/// Member peek icon diameter.
pub const STACK_CAPSULE_PEEK_ICON_SIZE_PX: f32 = 20.0;
/// Negative left margin between adjacent peek icons.
pub const STACK_CAPSULE_PEEK_OVERLAP_PX: f32 = 6.0;
/// Tauri `.stack-capsule__peek { padding-right: 4px }`.
pub const STACK_CAPSULE_PEEK_PAD_RIGHT_PX: f32 = 4.0;
/// Main icon bubble diameter.
pub const STACK_CAPSULE_MAIN_ICON_BUBBLE_PX: f32 = 28.0;
/// Fixed Tauri `ZoneIcon size={18}` glyph inside the main bubble.
pub const STACK_CAPSULE_MAIN_ICON_GLYPH_PX: f32 = 18.0;
/// Badge minimum width.
pub const STACK_CAPSULE_BADGE_MIN_WIDTH_PX: f32 = 24.0;
/// Badge height.
pub const STACK_CAPSULE_BADGE_HEIGHT_PX: f32 = 24.0;
/// Badge horizontal padding (`padding: 0 8px`).
pub const STACK_CAPSULE_BADGE_PAD_X_PX: f32 = 8.0;
/// Stack capsule title font size.
pub const STACK_CAPSULE_TITLE_FONT_PX: f32 = 13.0;
/// Stack capsule title font weight.
pub const STACK_CAPSULE_TITLE_FONT_WEIGHT: u16 = 600;
/// Stack capsule badge font size.
pub const STACK_CAPSULE_BADGE_FONT_PX: f32 = 12.0;
/// Stack capsule badge font weight.
pub const STACK_CAPSULE_BADGE_FONT_WEIGHT: u16 = 700;

/// Release motion envelope for the one capsule-to-panel surface.
///
/// The 500 ms reference cadence was visually continuous but felt heavy once
/// the shell/content split had been removed. A 240 ms envelope keeps roughly
/// fourteen 60 Hz frames for a full travel, reaches the settled product state
/// quickly, and still leaves enough samples for a mid-flight reversal.
pub const ZONE_PILL_ANIM_DURATION_MS: u32 = 240;
/// Geometry, identity and expanded content deliberately share the same clock.
pub const ZONE_PILL_GEOMETRY_DURATION_MS: u32 = ZONE_PILL_ANIM_DURATION_MS;
/// Shortest interrupted/reversed segment. A partial reversal scales with the
/// remaining visual distance but retains several 60 Hz frames.
pub const ZONE_PILL_MIN_SEGMENT_DURATION_MS: u32 = 50;
/// Map the shared animation progress onto the outer-shell timeline.
/// Kept as a named helper because paint and hit-test must consume identical
/// geometry even if the content envelope is retuned in a future reference run.
#[inline]
pub fn pill_geometry_progress(animation_progress: f32) -> f32 {
    let elapsed_ms = animation_progress.clamp(0.0, 1.0) * ZONE_PILL_ANIM_DURATION_MS as f32;
    (elapsed_ms / ZONE_PILL_GEOMETRY_DURATION_MS as f32).clamp(0.0, 1.0)
}

/// Duration for a transition segment that starts from an already-visible
/// morph. CSS shortens interrupted reverse transitions; scaling by remaining
/// visual distance reproduces that responsive feel while keeping a 50 ms floor
/// for input legibility. Pure and allocation-free.
#[inline]
pub fn pill_segment_duration_ms(from_morph: f32, to_morph: f32) -> u32 {
    let distance = (to_morph - from_morph).abs().clamp(0.0, 1.0);
    let scaled = (distance * ZONE_PILL_ANIM_DURATION_MS as f32).round() as u32;
    scaled.clamp(
        ZONE_PILL_MIN_SEGMENT_DURATION_MS,
        ZONE_PILL_ANIM_DURATION_MS,
    )
}

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

/// CSS `ease-out` (`cubic-bezier(0, 0, 0.58, 1)`) used by the Zen layer.
const EASE_OUT_P1X: f32 = 0.0;
const EASE_OUT_P1Y: f32 = 0.0;
const EASE_OUT_P2X: f32 = 0.58;
const EASE_OUT_P2Y: f32 = 1.0;

/// Stack bloom petal exit easing from Tauri `StackWrapper.css`:
/// `cubic-bezier(0.4, 0, 0.7, 0.2)`.
const STACK_BLOOM_EXIT_P1X: f32 = 0.40;
const STACK_BLOOM_EXIT_P1Y: f32 = 0.0;
const STACK_BLOOM_EXIT_P2X: f32 = 0.70;
const STACK_BLOOM_EXIT_P2Y: f32 = 0.20;

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
/// Stack-only, total, no panics. Shared by the active Zen and stack animation
/// curves.
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

/// CSS `ease-out` progress used by the single capsule-to-panel visual morph.
#[inline]
pub fn ease_out_progress(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let u = bezier_solve_x(t, EASE_OUT_P1X, EASE_OUT_P2X);
    bezier_axis(u, EASE_OUT_P1Y, EASE_OUT_P2Y)
}

/// Tauri stack bloom petal exit curve.
///
/// This is intentionally separate from [`ease_out_back_progress`]: exit petals
/// do not overshoot, they accelerate back into the capsule origin while fading.
#[inline]
pub fn ease_stack_bloom_exit_progress(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let u = bezier_solve_x(t, STACK_BLOOM_EXIT_P1X, STACK_BLOOM_EXIT_P2X);
    bezier_axis(u, STACK_BLOOM_EXIT_P1Y, STACK_BLOOM_EXIT_P2Y)
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

/// Resolve the current visual morph for one interruptible transition segment.
///
/// Paint, hit/chrome geometry, identity placement, and expanded-content alpha
/// all consume this same segment start, eased progress, and target. Recording
/// `from_morph` keeps a mid-flight reversal continuous. The monotonic ease-out
/// deliberately replaces the old geometry-only `easeOutBack` overshoot: that
/// curve reached and exceeded the final panel while content was still arriving,
/// which made the animation look like a detached plate followed by a second
/// steady-state render.
pub fn current_morph_progress(from_morph: f32, raw: f32, expanding: bool) -> f32 {
    let eased = ease_out_progress(pill_geometry_progress(raw));
    let target = if expanding { 1.0 } else { 0.0 };
    from_morph + (target - from_morph) * eased
}

/// Resolve `(morph, rect)` from the same segment state used by paint, hit-test,
/// and click-through region generation. Pure and allocation-free.
pub fn current_morph_rect(
    pill: Rect,
    expanded: Rect,
    from_morph: f32,
    raw: f32,
    expanding: bool,
) -> (f32, Rect) {
    let morph = current_morph_progress(from_morph, raw, expanding);
    (morph, morph_pill_to_rect(pill, expanded, morph))
}

/// Directional expanded-panel placement resolved from a collapsed capsule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpandedZonePlacement {
    /// Visible expanded panel rectangle in Main-client logical DIPs.
    pub panel: Rect,
    /// `true` when the capsule's right edge is the fixed horizontal anchor.
    pub anchor_right: bool,
    /// `true` when the capsule's bottom edge is the fixed vertical anchor.
    pub anchor_bottom: bool,
}

/// Resolve the expanded panel from the capsule's desktop quadrant.
///
/// The selected capsule edge stays fixed; insufficient directional space
/// shrinks the visible panel instead of translating it away from that edge.
/// Center-line ties deterministically use the left/top expansion rules.
pub fn expanded_zone_placement(
    capsule: Rect,
    stored_width: f32,
    stored_height: f32,
    viewport: Size,
) -> ExpandedZonePlacement {
    let viewport_width = finite_non_negative(viewport.width).floor();
    let viewport_height = finite_non_negative(viewport.height).floor();
    let capsule_left = finite_non_negative(capsule.x).min(viewport_width);
    let capsule_top = finite_non_negative(capsule.y).min(viewport_height);
    let capsule_right = finite_non_negative(capsule.right()).clamp(capsule_left, viewport_width);
    let capsule_bottom = finite_non_negative(capsule.bottom()).clamp(capsule_top, viewport_height);
    let anchor_right = (capsule_left + capsule_right) * 0.5 > viewport_width * 0.5;
    let anchor_bottom = (capsule_top + capsule_bottom) * 0.5 > viewport_height * 0.5;
    let available_width = if anchor_right {
        capsule_right
    } else {
        viewport_width - capsule_left
    }
    .max(0.0)
    .floor();
    let available_height = if anchor_bottom {
        capsule_bottom
    } else {
        viewport_height - capsule_top
    }
    .max(0.0)
    .floor();
    let width = finite_non_negative(stored_width).min(available_width);
    let height = finite_non_negative(stored_height).min(available_height);

    ExpandedZonePlacement {
        panel: Rect {
            x: if anchor_right {
                capsule_right - width
            } else {
                capsule_left
            },
            y: if anchor_bottom {
                capsule_bottom - height
            } else {
                capsule_top
            },
            width,
            height,
        },
        anchor_right,
        anchor_bottom,
    }
}

/// Clamp a capsule home to a logical Main-client viewport.
///
/// Fractional viewport bounds are rounded down so the returned integer origin
/// can never place the visible capsule across the right or bottom edge.
pub fn clamp_capsule_origin_to_viewport(
    x: i32,
    y: i32,
    capsule_width: i32,
    capsule_height: i32,
    viewport: Size,
) -> (i32, i32) {
    let viewport_width = finite_non_negative(viewport.width).floor() as i32;
    let viewport_height = finite_non_negative(viewport.height).floor() as i32;
    let max_x = viewport_width.saturating_sub(capsule_width.max(0)).max(0);
    let max_y = viewport_height.saturating_sub(capsule_height.max(0)).max(0);
    (x.clamp(0, max_x), y.clamp(0, max_y))
}

#[inline]
fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

/// Title font size resolved for the live collapsed pill paint/layout path.
///
/// This keeps the geometry line box and the renderer's DWrite font size locked
/// to the same visible-glyph branch without moving the base
/// [`CapsuleSize::title_font_px`] contract used by no-glyph large capsules.
#[inline]
pub const fn pill_title_font_px_for(size: CapsuleSize, _has_visible_glyph: bool) -> f32 {
    size.title_font_px()
}

/// Select the visual metrics used by the icon/title/badge content band.
///
/// N188 separates this from actual glyph presence. The corrected Large
/// `Compiler` capsule paints a real `code` glyph, but the reference video keeps
/// its long-ASCII title and count chip on the previously proven source-tier
/// content profile. CJK `浏览器` and short-ASCII `ai` keep the visible-glyph
/// profile. Icon slot geometry and glyph paint continue to use the actual icon
/// name, so this selector cannot suppress a real glyph.
#[inline]
pub fn pill_uses_visible_glyph_content_metrics(size: CapsuleSize, icon: &str, title: &str) -> bool {
    icon_name_has_visible_glyph(icon)
        && !(size == CapsuleSize::Large && icon == "code" && title.len() > 2 && title.is_ascii())
}

/// Title font size resolved for a concrete collapsed pill title.
///
/// This mirrors Tauri's `useTextAbbr` intent more closely than the old
/// size/glyph-only helper: short Large visible-glyph labels keep the source
/// tier, while longer labels still enter the C25 video-observed cap before the
/// renderer's shrink-to-fit loop measures them.
#[inline]
pub fn pill_title_font_px_for_text(
    size: CapsuleSize,
    has_visible_glyph: bool,
    _title: &str,
) -> f32 {
    pill_title_font_px_for(size, has_visible_glyph)
}

/// Title letter spacing resolved for the live collapsed pill paint path.
#[inline]
pub const fn pill_title_tracking_px_for(_size: CapsuleSize, _has_visible_glyph: bool) -> f32 {
    PILL_TITLE_TRACKING_PX
}

/// Title alpha resolved for the live collapsed pill paint path.
#[inline]
pub const fn pill_title_alpha_for(_size: CapsuleSize, _has_visible_glyph: bool) -> f32 {
    PILL_TITLE_ALPHA
}

/// Vertical content offset for the capsule's icon/title/badge band.
///
/// The video-observed Large tier has separate visible-glyph and no-glyph
/// branches. Small and Medium stay centered on the Tauri CSS line box.
#[inline]
pub const fn pill_content_dy_for(_size: CapsuleSize, _has_visible_glyph: bool) -> f32 {
    0.0
}

/// Right inset used to anchor the count badge.
///
/// The Large tier keeps a video-observed badge inset separate from the source
/// CSS right padding token so title/icon source geometry stays independently
/// auditable.
#[inline]
pub const fn pill_badge_right_inset_for(
    size: CapsuleSize,
    has_visible_glyph: bool,
    source_pad_right: f32,
) -> f32 {
    match (size, has_visible_glyph) {
        (CapsuleSize::Large, true) => PILL_LARGE_VISIBLE_GLYPH_BADGE_RIGHT_INSET_PX,
        (CapsuleSize::Large, false) => PILL_LARGE_NO_GLYPH_BADGE_RIGHT_INSET_PX,
        _ => source_pad_right,
    }
}

/// Runtime badge height for the visible-glyph/no-glyph Large visual branches.
///
/// This keeps the source `CapsuleSize::badge_height_px` table intact while the
/// live same-scene renderer follows the component-local video crop for Large
/// count chips.
#[inline]
pub const fn pill_badge_height_for(size: CapsuleSize, has_visible_glyph: bool) -> f32 {
    match (size, has_visible_glyph) {
        (CapsuleSize::Large, true) => PILL_LARGE_VISIBLE_GLYPH_BADGE_HEIGHT_PX,
        (CapsuleSize::Large, false) => PILL_LARGE_NO_GLYPH_BADGE_HEIGHT_PX,
        _ => size.badge_height_px(),
    }
}

/// Runtime badge width for the visible-glyph/no-glyph Large visual branches.
#[inline]
pub fn pill_badge_width_for_size_count(
    size: CapsuleSize,
    has_visible_glyph: bool,
    count: usize,
) -> f32 {
    let width = badge_width_for_size_count(size, count);
    match (size, has_visible_glyph) {
        (CapsuleSize::Large, false) => width + PILL_LARGE_NO_GLYPH_BADGE_WIDTH_EXTRA_PX,
        _ => width,
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

mod hover;
mod layout;

pub use hover::*;
pub use layout::*;

#[cfg(test)]
mod tests;
