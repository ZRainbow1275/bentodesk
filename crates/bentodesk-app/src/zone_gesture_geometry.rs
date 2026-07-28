//! Pure geometry helpers for M4 functional zone gestures (drag threshold +
//! drop-overlap → stack). Behaviourally 1:1 with the Tauri reference
//! (`BentoZone.tsx`, v1.3.0); see the locked native-migration gesture contract.
//!
//! Everything here is window-free, allocation-free, panic-free (§11), and
//! safe for the per-`WM_MOUSEMOVE` hot path (§10) — the threshold check is
//! integer-only with an `i64` widen so the squared sum cannot overflow, and
//! the O(n) stack-target scan runs only once on mouse-up, off the hot path.

use crate::zone_pill_geometry::{pill_layout_for_zone, stack_capsule_layout_for_zone};
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
        if let Some(anchor) = self_anchor {
            if zones.stack_anchor_for(candidate.id) == Some(anchor) {
                continue;
            }
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
    use super::*;
    use bentodesk_zone::{Zone, ZoneId, ZoneList};

    // ── exceeds_drag_threshold ────────────────────────────────────────────

    #[test]
    fn threshold_below_4_dip_is_still_a_click() {
        assert!(!exceeds_drag_threshold(0, 0));
        assert!(!exceeds_drag_threshold(3, 0));
        assert!(!exceeds_drag_threshold(0, 3));
        // (2,2) ⇒ 8 < 16 ⇒ still a click.
        assert!(!exceeds_drag_threshold(2, 2));
        // (-3, 0) symmetric with (3, 0).
        assert!(!exceeds_drag_threshold(-3, 0));
    }

    #[test]
    fn threshold_at_or_past_4_dip_is_a_drag() {
        // (4,0) ⇒ 16 >= 16 ⇒ drag (the first qualifying frame).
        assert!(exceeds_drag_threshold(4, 0));
        assert!(exceeds_drag_threshold(0, 4));
        // (3,3) ⇒ 18 >= 16 ⇒ drag.
        assert!(exceeds_drag_threshold(3, 3));
        assert!(exceeds_drag_threshold(-3, 3));
        assert!(exceeds_drag_threshold(100, -100));
    }

    #[test]
    fn threshold_does_not_overflow_on_large_deltas() {
        // 50_000² = 2.5e9 > i32::MAX (~2.1e9), so a same-width i32 multiply
        // would overflow; the i64 widen keeps it safe for any realistic
        // multi-monitor desktop delta (Tauri parity range is a few thousand).
        assert!(exceeds_drag_threshold(50_000, 50_000));
        assert!(exceeds_drag_threshold(-50_000, 50_000));
    }

    // ── score_stack_candidate (pure rect math) ────────────────────────────

    #[test]
    fn score_far_apart_does_not_fire() {
        // Two 200x200 zones, centres ~1000 apart, no overlap, far beyond
        // proximity radius (0.8 * (100 + 100) = 160).
        assert!(score_stack_candidate((0, 0, 200, 200), (1000, 0, 200, 200)).is_none());
    }

    #[test]
    fn score_full_overlap_fires_with_high_score() {
        // Identical rects ⇒ overlapRatio ≈ 1.0 ⇒ score ≈ 2.0.
        let s = score_stack_candidate((0, 0, 200, 200), (0, 0, 200, 200)).unwrap();
        assert!((s - 2.0).abs() < 1e-3, "score was {s}");
    }

    #[test]
    fn score_25_percent_overlap_below_threshold_with_far_centres() {
        // Construct ~25% overlap of the smaller area but keep centres outside
        // the proximity radius so trigger B does NOT rescue it.
        // self 200x200 at (0,0): area 40000. other 200x200 shifted so the
        // intersection area is ~25% of 40000 = 10000 ⇒ overlap rect 100x100.
        // overlap 100x100 needs other to start at (100,100): inter = [100,200)
        // both axes ⇒ 100x100 = 10000 ⇒ ratio 0.25 < 0.30.
        // BUT centres: self (100,100), other (200,200) ⇒ dist ~141.4, radius
        // 160 ⇒ proximity WOULD fire. So push other further to (160,160):
        // inter_w = 200-160 = 40, area 1600 ⇒ ratio 0.04; centres (100,100)
        // vs (260,260) ⇒ dist ~226 > 160 ⇒ neither fires.
        assert!(score_stack_candidate((0, 0, 200, 200), (160, 160, 200, 200)).is_none());
    }

    #[test]
    fn score_35_percent_overlap_fires() {
        // self 200x200 at (0,0); other shifted to give >30% overlap of the
        // smaller area. other at (74,0): inter_w = 200-74 = 126, inter_h 200
        // ⇒ 25200 / 40000 = 0.63 > 0.30 ⇒ fires.
        let s = score_stack_candidate((0, 0, 200, 200), (74, 0, 200, 200)).unwrap();
        // overlap > 0 ⇒ score = ratio + 1 > 1.
        assert!(s > 1.0, "overlapping score must exceed 1.0, was {s}");
    }

    #[test]
    fn score_proximity_only_fires_below_one() {
        // No overlap but centres within 0.8*(rSelf+rOther). Two 100x100 zones
        // (r = 50 each), radius = 0.8 * 100 = 80. Place them edge-touching so
        // there is zero overlap (gap of 0) but centres 100 apart along x:
        // self (0,0,100,100) centre (50,50); other (100,0,100,100) centre
        // (150,50) ⇒ dist 100 > 80 ⇒ no fire. Move closer: other at (60,0):
        // inter_w = 100-60 = 40 ⇒ overlap fires. To get PURE proximity with
        // zero overlap we need a gap on one axis but proximity on centres —
        // use a vertical gap: self (0,0,100,100), other (0,120,100,100):
        // no overlap (gap 20 on y), centres (50,50) vs (50,170) ⇒ dist 120 >
        // 80 ⇒ no fire. Tighten gap: other (0,100,100,100): touching, dist
        // 100 > 80 still. The proximity trigger only beats overlap when boxes
        // are SMALL relative to radius; use 100x100 with a diagonal near-touch
        // that has zero AABB overlap is geometrically impossible here, so test
        // proximity via a thin gap that yields ratio 0 yet dist <= radius:
        // self (0,0,200,40) centre (100,20) r=(200+40)/4=60; other
        // (0,50,200,40) centre (100,70) r=60; radius=0.8*120=96; dist=50<=96;
        // AABB overlap: y [0,40) vs [50,90) ⇒ none ⇒ pure proximity.
        let s = score_stack_candidate((0, 0, 200, 40), (0, 50, 200, 40)).unwrap();
        // Pure proximity ⇒ score in (0, 1].
        assert!(
            s > 0.0 && s <= 1.0,
            "proximity score must be in (0,1], was {s}"
        );
    }

    // ── stack_target_for_drop (ZoneList integration, no window) ───────────

    fn zone_at(id: u64, x: i32, y: i32, w: i32, h: i32) -> Zone {
        Zone::new(ZoneId(id), "z", x, y, w, h)
    }

    #[test]
    fn drop_far_from_all_returns_none() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 0, 0, 200, 200));
        zones.add(zone_at(2, 5000, 5000, 200, 200));
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn expanded_panel_size_does_not_create_an_invisible_merge_halo() {
        let mut zones = ZoneList::new();
        // The persisted expanded panels overlap by 300×600 DIP, but the
        // painted medium capsules are only 214×48 and sit 286 DIP apart.
        // Tauri scores the capsules, so this drop must remain independent.
        zones.add(zone_at(1, 0, 100, 800, 600));
        zones.add(zone_at(2, 500, 100, 800, 600));

        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn painted_capsule_overlap_forms_a_stack_regardless_of_panel_size() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 100, 100, 80, 60));
        zones.add(zone_at(2, 180, 100, 900, 700));

        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), Some(ZoneId(2)));
    }

    #[test]
    fn drop_onto_overlapping_zone_returns_that_anchor() {
        let mut zones = ZoneList::new();
        // Dragged (id 1) fully inside id 2 → high overlap.
        zones.add(zone_at(1, 50, 50, 100, 100));
        zones.add(zone_at(2, 0, 0, 300, 300));
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), Some(ZoneId(2)));
    }

    #[test]
    fn self_is_never_returned() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 0, 0, 200, 200));
        // Only the dragged zone exists → no target.
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn highest_scoring_candidate_wins() {
        let mut zones = ZoneList::new();
        // Dragged at (100,100,100,100), centre (150,150).
        zones.add(zone_at(1, 100, 100, 100, 100));
        // Candidate 2: small overlap.
        zones.add(zone_at(2, 180, 100, 100, 100)); // inter_w=20 ⇒ ratio 0.2 (<0.30) but proximity may fire
        // Candidate 3: near-full overlap ⇒ much higher score.
        zones.add(zone_at(3, 110, 110, 100, 100));
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), Some(ZoneId(3)));
    }

    #[test]
    fn locked_candidate_is_skipped() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 50, 50, 100, 100));
        let mut locked = zone_at(2, 0, 0, 300, 300);
        locked.locked = true;
        zones.add(locked);
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn invisible_candidate_is_skipped() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 50, 50, 100, 100));
        let mut hidden = zone_at(2, 0, 0, 300, 300);
        hidden.visible = false;
        zones.add(hidden);
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn stacked_child_candidate_is_skipped() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 50, 50, 100, 100));
        let mut child = zone_at(2, 0, 0, 300, 300);
        child.stack_parent = Some(ZoneId(9)); // is_stacked_child() == true
        zones.add(child);
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn same_stack_candidate_is_skipped() {
        let mut zones = ZoneList::new();
        // Anchor (id 2) overlaps dragged (id 1); make id 1 already a member of
        // id 2's stack so they share the same anchor ⇒ skip the restack.
        zones.add(zone_at(1, 50, 50, 100, 100));
        zones.add(zone_at(2, 0, 0, 300, 300));
        assert!(zones.stack(ZoneId(2), ZoneId(1)));
        // Now dragged (1) shares anchor (2) with the only candidate (2, the
        // anchor). is_stacked_child(1)==true is irrelevant for the dragged;
        // the candidate 2 shares the anchor with 1 ⇒ skip ⇒ None.
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn existing_stack_anchor_does_not_enter_the_free_zone_auto_merge_path() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 100, 100, 400, 300));
        zones.add(zone_at(2, 100, 100, 400, 300));
        zones.add(zone_at(3, 100, 100, 400, 300));
        assert!(zones.stack(ZoneId(1), ZoneId(2)));

        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }
}
