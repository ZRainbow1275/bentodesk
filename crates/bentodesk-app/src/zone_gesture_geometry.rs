//! Pure geometry helpers for M4 functional zone gestures (drag threshold +
//! drop-overlap → stack). Behaviourally 1:1 with the Tauri reference
//! (`BentoZone.tsx`, v1.3.0); see the locked native-migration gesture contract.
//!
//! Everything here is window-free, allocation-free, panic-free (§11), and
//! safe for the per-`WM_MOUSEMOVE` hot path (§10) — the threshold check is
//! integer-only with an `i64` widen so the squared sum cannot overflow, and
//! the O(n) stack-target scan runs only once on mouse-up, off the hot path.

use crate::{
    state::ZoneResizeSession,
    zone_pill_geometry::{
        expanded_zone_placement, pill_layout_for_zone, stack_capsule_layout_for_zone,
    },
};
use bentodesk_style::Size;
use bentodesk_zone::{Zone, ZoneId, ZoneList};

/// Tauri parity: `ZONE_DRAG_THRESHOLD_PX = 4` (`BentoZone.tsx:72`). Logical
/// DIP; native's mouse handlers already operate in logical DIP, so this is
/// apples-to-apples with Tauri's 4 CSS px (no DPI scaling needed).
pub const ZONE_DRAG_THRESHOLD_DIP: i32 = 4;

/// Tauri parity: `OVERLAP_THRESHOLD = 0.3` (`BentoZone.tsx:787`). A stack
/// fires when the AABB intersection area is ≥ 30 % of the smaller capsule's
/// area.
pub const STACK_OVERLAP_THRESHOLD: f32 = 0.30;

/// Tauri parity: `PROXIMITY_FACTOR = 0.8` (`BentoZone.tsx:786`). A stack also
/// fires when the centre-to-centre distance is ≤ `0.8 × (rSelf + rOther)`,
/// where `r = (w + h) / 4` (average half-extent).
pub const STACK_PROXIMITY_FACTOR: f32 = 0.80;

/// Has the pointer travelled past the 4-DIP drag threshold from the
/// mouse-down origin? Euclidean (matches Tauri's `Math.hypot(dx, dy) < 4`),
/// integer-only — no `f32::hypot`, no `sqrt`. `dx`/`dy` are logical-DIP
/// deltas from the mouse-down origin. Widened to `i64` so the squared sum
/// cannot overflow for any realistic delta.
///
/// Returns `true` once `dx² + dy² ≥ 4²` (i.e. distance ≥ 4 DIP). The Tauri
/// rule is "< 4 ⇒ still a click", so `≥ 4` is the first frame that counts as
/// a drag, latching the gesture.
#[inline]
pub fn exceeds_drag_threshold(dx: i32, dy: i32) -> bool {
    let dx = dx as i64;
    let dy = dy as i64;
    let thresh = (ZONE_DRAG_THRESHOLD_DIP as i64).pow(2);
    dx * dx + dy * dy >= thresh
}

/// Axis-selective result of one resize frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZoneResizeGeometry {
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub home_x: Option<i32>,
    pub home_y: Option<i32>,
}

/// Resolve one of the eight standard resize handles from the immutable
/// mouse-down snapshot. The opposite selected edge remains fixed. An inactive
/// axis is always `None`, so a clipped persisted dimension cannot be
/// accidentally overwritten by an orthogonal edge drag.
pub fn directional_resize_geometry(
    session: ZoneResizeSession,
    pointer_x: f32,
    pointer_y: f32,
    viewport: Size,
    min_width: f32,
    min_height: f32,
) -> ZoneResizeGeometry {
    let viewport_width = finite_non_negative(viewport.width).floor();
    let viewport_height = finite_non_negative(viewport.height).floor();
    let delta_x = finite_or(pointer_x, session.start_pointer_x) - session.start_pointer_x;
    let delta_y = finite_or(pointer_y, session.start_pointer_y) - session.start_pointer_y;

    let mut result = ZoneResizeGeometry::default();
    if session.handle.resizes_horizontally() {
        let start_selected = if session.handle.drags_left() {
            session.start_panel.x
        } else {
            session.start_panel.right()
        };
        let fixed = if session.handle.drags_left() {
            session.start_panel.right()
        } else {
            session.start_panel.x
        };
        let available = if session.handle.drags_left() {
            fixed
        } else {
            viewport_width - fixed
        }
        .max(0.0);
        let floor = finite_non_negative(min_width)
            .min(session.start_panel.width.max(0.0))
            .min(available);
        let desired = finite_or(start_selected + delta_x, start_selected);
        let mut selected = if session.handle.drags_left() {
            desired.clamp(0.0, fixed - floor)
        } else {
            desired.clamp(fixed + floor, viewport_width)
        };

        let moves_home = (session.handle.drags_left() && !session.anchor_right)
            || (!session.handle.drags_left() && session.anchor_right);
        if moves_home {
            let requested_home = session.start_home_x as f32 + selected - start_selected;
            let max_home = (viewport_width - session.start_capsule.width)
                .max(0.0)
                .floor() as i32;
            let mut home = requested_home.round() as i32;
            home = home.clamp(0, max_home);
            let center_limit = viewport_width * 0.5 - session.start_capsule.width * 0.5;
            home = if session.anchor_right {
                home.max(center_limit.floor() as i32 + 1)
            } else {
                home.min(center_limit.floor() as i32)
            }
            .clamp(0, max_home);
            // The placement quadrant is derived rather than persisted. Re-run
            // the real placement resolver after integer rounding so its strict
            // `>` centre-line rule remains the final authority. The arithmetic
            // clamp above normally makes this a no-op; a one-DIP correction is
            // sufficient only for a representational boundary tie.
            let horizontal_capsule = bentodesk_style::Rect {
                x: home as f32,
                ..session.start_capsule
            };
            if expanded_zone_placement(
                horizontal_capsule,
                session.start_persisted_width as f32,
                session.start_persisted_height as f32,
                viewport,
            )
            .anchor_right
                != session.anchor_right
            {
                home = if session.anchor_right {
                    home.saturating_add(1).min(max_home)
                } else {
                    home.saturating_sub(1)
                };
            }
            selected = if session.handle.drags_left() {
                home as f32
            } else {
                home as f32 + session.start_capsule.width
            };
            // The active home axis is snapshot-relative too. Returning the
            // start home on the down-point frame lets the live seam restore a
            // value that a previous frame changed; inactive axes stay `None`.
            result.home_x = Some(home);
        }

        let selected = selected.round();
        let width = if selected == start_selected.round() {
            session.start_persisted_width
        } else {
            (fixed.round() - selected).abs() as i32
        };
        // Always emit the active axis, including its mouse-down value.
        result.width = Some(width);
    }

    if session.handle.resizes_vertically() {
        let start_selected = if session.handle.drags_top() {
            session.start_panel.y
        } else {
            session.start_panel.bottom()
        };
        let fixed = if session.handle.drags_top() {
            session.start_panel.bottom()
        } else {
            session.start_panel.y
        };
        let available = if session.handle.drags_top() {
            fixed
        } else {
            viewport_height - fixed
        }
        .max(0.0);
        let floor = finite_non_negative(min_height)
            .min(session.start_panel.height.max(0.0))
            .min(available);
        let desired = finite_or(start_selected + delta_y, start_selected);
        let mut selected = if session.handle.drags_top() {
            desired.clamp(0.0, fixed - floor)
        } else {
            desired.clamp(fixed + floor, viewport_height)
        };

        let moves_home = (session.handle.drags_top() && !session.anchor_bottom)
            || (!session.handle.drags_top() && session.anchor_bottom);
        if moves_home {
            let requested_home = session.start_home_y as f32 + selected - start_selected;
            let max_home = (viewport_height - session.start_capsule.height)
                .max(0.0)
                .floor() as i32;
            let mut home = requested_home.round() as i32;
            home = home.clamp(0, max_home);
            let center_limit = viewport_height * 0.5 - session.start_capsule.height * 0.5;
            home = if session.anchor_bottom {
                home.max(center_limit.floor() as i32 + 1)
            } else {
                home.min(center_limit.floor() as i32)
            }
            .clamp(0, max_home);
            let vertical_capsule = bentodesk_style::Rect {
                y: home as f32,
                ..session.start_capsule
            };
            if expanded_zone_placement(
                vertical_capsule,
                session.start_persisted_width as f32,
                session.start_persisted_height as f32,
                viewport,
            )
            .anchor_bottom
                != session.anchor_bottom
            {
                home = if session.anchor_bottom {
                    home.saturating_add(1).min(max_home)
                } else {
                    home.saturating_sub(1)
                };
            }
            selected = if session.handle.drags_top() {
                home as f32
            } else {
                home as f32 + session.start_capsule.height
            };
            result.home_y = Some(home);
        }

        let selected = selected.round();
        let height = if selected == start_selected.round() {
            session.start_persisted_height
        } else {
            (fixed.round() - selected).abs() as i32
        };
        // Always emit the active axis, including its mouse-down value.
        result.height = Some(height);
    }
    result
}

#[inline]
fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[inline]
fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// Port of `findOverlapStackTarget` (`BentoZone.tsx:755-848`). Given the
/// `dragged` zone's id (whose live rect is already written into `zones` by
/// the synchronous `MoveZone` reducer at drop time), returns the **anchor**
/// `ZoneId` the dragged zone should stack ONTO, or `None` when no candidate
/// qualifies.
///
/// Rule (per candidate, in DIP):
/// 1. Trigger A — overlap ratio ≥ [`STACK_OVERLAP_THRESHOLD`]: AABB
///    intersection area / smaller capsule area.
/// 2. Trigger B — proximity: centre distance ≤
///    `STACK_PROXIMITY_FACTOR × (rSelf + rOther)`, `r = (w + h) / 4`.
/// 3. Fire if **EITHER** A or B (`||`).
/// 4. Score = `overlapRatio > 0 ? overlapRatio + 1 : max(0, 1 - dist/radius)`;
///    keep the **highest-scoring** candidate (overlap always beats pure
///    proximity via the `+1` floor).
///
/// Guards (§4e — all enforced before scoring):
/// - skip self (`candidate.id == dragged`),
/// - skip a candidate that already shares the dragged zone's stack anchor,
/// - skip a **locked** candidate (a locked zone never absorbs a drop),
/// - skip an invisible or stacked-child candidate (not a visible drop
///   target; mirrors `hit_test_zone`'s filter).
pub fn stack_target_for_drop(zones: &ZoneList, dragged: ZoneId) -> Option<ZoneId> {
    let self_zone = zones.get(dragged)?;
    // Tauri routes an existing stack through `StackWrapper`'s rigid-cluster
    // drag path. That producer never runs `findOverlapStackTarget`, so a formed
    // stack must not be folded into another stack by the ordinary Zone drop
    // producer (which would create a nested/hidden tree in native's anchor model).
    if self_zone.is_stack_anchor() {
        return None;
    }
    let self_rect = zone_drag_capsule_rect(zones, self_zone);
    // The dragged zone's own stack anchor (if any) — used to skip stacking
    // onto a zone already in the same group (Tauri: `other.stack_id ===
    // selfStackId`). `stack_anchor_for` returns the dragged id itself when it
    // is its own anchor.
    let self_anchor = zones.stack_anchor_for(dragged);

    let mut best: Option<(ZoneId, f32)> = None;
    for candidate in zones.iter() {
        // Guard 1 — never stack onto self.
        if candidate.id == dragged {
            continue;
        }
        // Guard 3 — a locked zone must not absorb a dropped zone.
        if candidate.locked {
            continue;
        }
        // Guard 4 — invisible / stacked-child zones are not visible drop
        // targets (their anchor is the rendered surface).
        if !candidate.is_visible() || candidate.is_stacked_child() {
            continue;
        }
        // Guard 2 — already share a stack anchor → skip (no self-restack).
        if let Some(anchor) = self_anchor
            && zones.stack_anchor_for(candidate.id) == Some(anchor)
        {
            continue;
        }

        let other_rect = zone_drag_capsule_rect(zones, candidate);
        if let Some(score) = score_stack_candidate(self_rect, other_rect) {
            let better = match best {
                None => true,
                Some((_, best_score)) => score > best_score,
            };
            if better {
                best = Some((candidate.id, score));
            }
        }
    }
    best.map(|(id, _)| id)
}

/// Painted collapsed drop geometry, shared with the renderer/hit-test SSoT.
///
/// A Zone's persisted `w/h` describe its expanded panel. Tauri explicitly
/// scores `getCapsuleBoxPx`, so using `w/h` here creates a large invisible
/// merge halo whenever a panel is wider/taller than its capsule. Stack anchors
/// use their dedicated 220×52 capsule rather than the normal per-size pill.
#[inline]
pub fn zone_drag_capsule_rect(zones: &ZoneList, zone: &Zone) -> (i32, i32, i32, i32) {
    let rect = if zone.is_stack_anchor() {
        let member_count = 1 + zone
            .stack_members
            .iter()
            .filter(|member_id| {
                zones
                    .get(**member_id)
                    .and_then(|member| member.stack_parent)
                    .is_some_and(|parent| parent == zone.id)
            })
            .count();
        stack_capsule_layout_for_zone(zone, member_count).rect
    } else {
        pill_layout_for_zone(zone, zone.items.len()).rect
    };
    (
        rect.x.round() as i32,
        rect.y.round() as i32,
        rect.width.round() as i32,
        rect.height.round() as i32,
    )
}

/// Core Tauri overlap/proximity rule on two raw rects `(x, y, w, h)` in DIP.
/// Returns `Some(score)` when the pair qualifies for a stack (trigger A or
/// B), else `None`. Pure — independently unit-testable without a `ZoneList`.
///
/// `score = overlapRatio > 0 ? overlapRatio + 1 : max(0, 1 - dist/radius)`
/// (`BentoZone.tsx:825-828`), so any real overlap (`ratio > 0`) outscores
/// every pure-proximity candidate.
fn score_stack_candidate(
    self_rect: (i32, i32, i32, i32),
    other_rect: (i32, i32, i32, i32),
) -> Option<f32> {
    let (sx, sy, sw, sh) = self_rect;
    let (ox, oy, ow, oh) = other_rect;
    // Degenerate rects can't overlap meaningfully; punt.
    if sw <= 0 || sh <= 0 || ow <= 0 || oh <= 0 {
        return None;
    }

    let self_left = sx as f32;
    let self_top = sy as f32;
    let self_right = (sx + sw) as f32;
    let self_bottom = (sy + sh) as f32;
    let o_left = ox as f32;
    let o_top = oy as f32;
    let o_right = (ox + ow) as f32;
    let o_bottom = (oy + oh) as f32;

    // Trigger A — overlap ratio (BentoZone.tsx:804-810).
    let inter_w = (self_right.min(o_right) - self_left.max(o_left)).max(0.0);
    let inter_h = (self_bottom.min(o_bottom) - self_top.max(o_top)).max(0.0);
    let inter_area = inter_w * inter_h;
    let self_area = (sw as f32) * (sh as f32);
    let other_area = (ow as f32) * (oh as f32);
    let min_area = self_area.min(other_area).max(1.0);
    let overlap_ratio = inter_area / min_area;

    // Trigger B — proximity (BentoZone.tsx:812-817).
    let self_cx = self_left + (sw as f32) / 2.0;
    let self_cy = self_top + (sh as f32) / 2.0;
    let o_cx = o_left + (ow as f32) / 2.0;
    let o_cy = o_top + (oh as f32) / 2.0;
    let ddx = self_cx - o_cx;
    let ddy = self_cy - o_cy;
    let dist = (ddx * ddx + ddy * ddy).sqrt();
    let r_self = (sw as f32 + sh as f32) / 4.0;
    let r_other = (ow as f32 + oh as f32) / 4.0;
    let proximity_radius = (r_self + r_other) * STACK_PROXIMITY_FACTOR;

    // Fire if EITHER trigger (BentoZone.tsx:819-820).
    let fires = overlap_ratio >= STACK_OVERLAP_THRESHOLD || dist <= proximity_radius;
    if !fires {
        return None;
    }

    // Score (BentoZone.tsx:825-828).
    let score = if overlap_ratio > 0.0 {
        overlap_ratio + 1.0
    } else if proximity_radius > 0.0 {
        (1.0 - dist / proximity_radius).max(0.0)
    } else {
        0.0
    };
    Some(score)
}

#[cfg(test)]
mod tests {
    include!("zone_gesture_geometry/tests.rs");
}
