/**
 * Regression tests for v8 round-4 #2 — permissive stack-on-drop proximity.
 *
 * The drop-on-zone → form-stack flow used to require the dropped capsule's
 * center to land INSIDE another zone's rect (strict center-in-rect). For
 * two ~220×52 pill capsules side by side that meant a 220-px wide sweet
 * spot, which felt brittle. v8 round-4 #2 replaces it with two permissive
 * triggers (AABB overlap ≥ 30%, OR center-distance ≤ (rSelf + rOther) × 0.8)
 * and best-of-many candidate selection.
 *
 * These tests pin the four behaviours the proximity rewrite is most at
 * risk of regressing:
 *   1. Proximity-only hit (overlap < threshold, distance < radius) → picks target.
 *   2. Far apart (overlap = 0, distance > radius) → returns null.
 *   3. Multiple candidates → highest-overlap (or closest) wins.
 *   4. Same-stack candidate → skipped, never returned.
 */
import { describe, it, expect } from "vitest";
import {
  findStackProximityHit,
  DEFAULT_OVERLAP_THRESHOLD,
  DEFAULT_PROXIMITY_FACTOR,
  type ProximityCandidate,
} from "../stackProximity";

/** Build a self-rect at the given position with default 220×52 capsule dims. */
function self(left: number, top: number, w = 220, h = 52) {
  return {
    selfId: "self",
    selfStackId: null as string | null,
    selfLeft: left,
    selfTop: top,
    selfWidth: w,
    selfHeight: h,
  };
}

/** Build a candidate rect at the given position with default 220×52 capsule dims. */
function candidate(
  id: string,
  left: number,
  top: number,
  opts: { stackId?: string | null; w?: number; h?: number } = {},
): ProximityCandidate {
  return {
    id,
    stackId: opts.stackId ?? null,
    left,
    top,
    width: opts.w ?? 220,
    height: opts.h ?? 52,
  };
}

describe("findStackProximityHit", () => {
  it("triggers on proximity when overlap < 30% but centers are within (rSelf+rOther)*0.8", () => {
    // Two 220×52 capsules. r* = (220+52)/4 = 68 each.
    // proximityRadius = (68 + 68) × 0.8 = 108.8 px.
    // self at (0,0,220,52); other at (0,60,220,52) — 8 px vertical gap.
    // AABB overlap: x [0,220], y intersection = max(0, min(52,112) - max(0,60))
    //             = max(0, 52-60) = 0 → ratio 0, threshold not met.
    // Centers: self (110, 26) ↔ other (110, 86). Distance = 60 px < 108.8 → trigger.
    const result = findStackProximityHit({
      ...self(0, 0),
      candidates: [candidate("near", 0, 60)],
    });
    expect(result).not.toBeNull();
    expect(result?.targetId).toBe("near");
    // Proximity-only hit → score < 1 (no overlap bonus).
    expect(result?.score).toBeGreaterThan(0);
    expect(result?.score).toBeLessThan(1);
  });

  it("triggers on overlap when AABB overlap ratio >= 30%", () => {
    // self at (0,0,220,52); other at (100,0,220,52).
    // Intersection: 220-100 = 120 × 52 = 6240; min area 11440 → ratio ~0.545 ≥ 0.3.
    const result = findStackProximityHit({
      ...self(0, 0),
      candidates: [candidate("overlapper", 100, 0)],
    });
    expect(result).not.toBeNull();
    expect(result?.targetId).toBe("overlapper");
    // Overlapping hit → score = overlapRatio + 1 ≈ 1.545.
    expect(result?.score).toBeGreaterThan(1);
  });

  it("returns null when candidate is far away (no overlap, distance > radius)", () => {
    // self at (0,0,220,52); other at (1000,1000,220,52).
    // Center distance ~1000+ ≫ proximity radius ~109. Zero overlap.
    const result = findStackProximityHit({
      ...self(0, 0),
      candidates: [candidate("far", 1000, 1000)],
    });
    expect(result).toBeNull();
  });

  it("picks the highest-overlap candidate when multiple are in range", () => {
    // self at (0,0,220,52).
    // candidate "small" overlaps ~18% (below 0.3 threshold AND distance 180>108.8) → no hit.
    // candidate "large" overlaps ~73% — deeply intersecting → score ≈ 1.73.
    // candidate "kiss" no overlap but adjacent vertically (proximity-only, score < 1).
    // Expected winner: "large" — overlap dominates over proximity-only "kiss".
    const result = findStackProximityHit({
      ...self(0, 0),
      candidates: [
        candidate("small", 180, 0), // inter 40×52 = 2080 / 11440 = 0.182
        candidate("large", 60, 0),  // inter 160×52 = 8320 / 11440 = 0.727
        candidate("kiss", 0, 60),    // 0 overlap, ~86 px center-dist, within radius
      ],
    });
    expect(result).not.toBeNull();
    expect(result?.targetId).toBe("large");
  });

  it("among proximity-only candidates, picks the closer one (higher score)", () => {
    // No overlap for either; both inside proximity radius (108.8 px).
    // "a" at (0, 60) → center distance 60 → score = 1 - 60/108.8 ≈ 0.449
    // "b" at (0, 80) → center distance 80 → score = 1 - 80/108.8 ≈ 0.265
    // Expected winner: "a" (closer → higher score).
    const result = findStackProximityHit({
      ...self(0, 0),
      candidates: [candidate("b", 0, 80), candidate("a", 0, 60)],
    });
    expect(result).not.toBeNull();
    expect(result?.targetId).toBe("a");
  });

  it("skips candidates that share the dragged zone's stack_id", () => {
    // self is part of stack "S1"; "sib" is in the same stack — must be skipped
    // even though it's overlapping. "other" is free-standing and is the only
    // legal target.
    const result = findStackProximityHit({
      selfId: "self",
      selfStackId: "S1",
      selfLeft: 0,
      selfTop: 0,
      selfWidth: 220,
      selfHeight: 52,
      candidates: [
        candidate("sib", 100, 0, { stackId: "S1" }),    // would overlap, skipped
        candidate("other", 0, 60, { stackId: null }),    // proximity hit, eligible
      ],
    });
    expect(result).not.toBeNull();
    expect(result?.targetId).toBe("other");
  });

  it("returns null when ALL candidates are in the same stack as self", () => {
    const result = findStackProximityHit({
      selfId: "self",
      selfStackId: "S1",
      selfLeft: 0,
      selfTop: 0,
      selfWidth: 220,
      selfHeight: 52,
      candidates: [
        candidate("sib1", 100, 0, { stackId: "S1" }),
        candidate("sib2", 0, 60, { stackId: "S1" }),
      ],
    });
    expect(result).toBeNull();
  });

  it("never picks the dragged zone itself", () => {
    // Candidate with the same id as self must be ignored even if perfectly overlapping.
    const result = findStackProximityHit({
      ...self(0, 0),
      candidates: [
        candidate("self", 0, 0), // dragged-zone snapshot, must be skipped
        candidate("real", 100, 0),
      ],
    });
    expect(result).not.toBeNull();
    expect(result?.targetId).toBe("real");
  });

  it("does not trigger when overlap is below threshold AND distance is just outside radius", () => {
    // 220×52 self at (0,0). proximityRadius for matching pair = 108.8.
    // Place at (250, 0): distance 250, no overlap. Should be null.
    const result = findStackProximityHit({
      ...self(0, 0),
      candidates: [candidate("just-outside", 250, 0)],
    });
    expect(result).toBeNull();
  });

  it("default thresholds match the BentoZone implementation", () => {
    // Pinned constants — if these change, BentoZone.findOverlapStackTarget
    // must change in lockstep (the docs claim the drop-merge uses 0.3 / 0.8).
    expect(DEFAULT_OVERLAP_THRESHOLD).toBe(0.3);
    expect(DEFAULT_PROXIMITY_FACTOR).toBe(0.8);
  });

  it("custom thresholds tighten/loosen the trigger envelope predictably", () => {
    // Same pair as the proximity case above. Default 0.8 factor → hit.
    // Lower factor to 0.3 → radius shrinks below the 86 px distance → no hit.
    const baseInput = {
      ...self(0, 0),
      candidates: [candidate("near", 0, 60)],
    };
    expect(findStackProximityHit(baseInput)).not.toBeNull();
    expect(
      findStackProximityHit({ ...baseInput, proximityFactor: 0.3 }),
    ).toBeNull();
  });
});
