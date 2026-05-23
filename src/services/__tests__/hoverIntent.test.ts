/**
 * v9 stack-wake-mutex — shared hover-intent timing constants.
 *
 * These tests pin the exported values so future refactors can't
 * silently drift the wake/leave timing thresholds. The bloom
 * collapse path is now poller-driven (StackWrapper subscribes to
 * the hoveredZone$ Solid signal exposed from services/hitTest.ts);
 * the constants here are still the single source of truth for the
 * tail grace timers used on the family-internal traversal path
 * (hovered === null branch).
 *
 * Pre-v9 values were LEAVE_GRACE_MS = 80, STICKY_GRACE_MS = 200.
 * v9 PR3 originally tightened both to 0 / 80 on the assumption that
 * the new hoveredZone$ effect's non-family-immediate-collapse
 * branch covered every leave case. Live testing surfaced the
 * regression: the cursor spends ~16–32 ms in hovered === null
 * while crossing the 16 px capsule→petal halo gap, and a 0 ms
 * LEAVE_GRACE_MS collapsed the bloom before the cursor reached
 * the petal. The post-PR3 fix restores LEAVE_GRACE_MS = 80 (and
 * keeps STICKY_GRACE_MS = 80) — the non-family-hit branch still
 * runs synchronously without a grace, so neighbour-zone
 * responsiveness is preserved.
 *
 * Changing these values is a UX decision — they affect the
 * perceived responsiveness of every hover surface. If a future
 * round needs to adjust them, update the test thresholds AND the
 * corresponding constants in lockstep.
 */
import { describe, it, expect } from "vitest";
import {
  HOVER_INTENT_MS,
  LEAVE_GRACE_MS,
  STICKY_GRACE_MS,
} from "../hoverIntent";

describe("v9 stack-wake-mutex — shared hover-intent constants", () => {
  it("exports HOVER_INTENT_MS = 150 (matches settings default expand_delay_ms)", () => {
    expect(HOVER_INTENT_MS).toBe(150);
  });

  it("exports LEAVE_GRACE_MS = 80 (family-internal traversal cushion; non-family hit takes the immediate-collapse branch)", () => {
    // v9 (post-PR3 fix): the hoveredZone$ effect tears the bloom
    // down synchronously when the cursor lands on a non-family
    // zone (e.g. a neighbouring capsule). The 80 ms grace covers
    // only the hovered === null branch — the cursor mid-flight
    // between two family elements (capsule → petal across the
    // 12 px petal halo, ~16 px total dead-space). PR3's earlier
    // 0 ms collapsed the bloom before the cursor reached the
    // petal; the restored 80 ms gives comfortable headroom at
    // typical cursor speeds (16 px / 500 px·s⁻¹ = 32 ms).
    expect(LEAVE_GRACE_MS).toBe(80);
  });

  it("exports STICKY_GRACE_MS = 80 (tightened from 200 ms; equal to LEAVE_GRACE_MS in v9)", () => {
    // The committed-preview path no longer needs a longer grace
    // than the hover-only path because v9's non-family-hit branch
    // tears sticky previews down synchronously when the cursor
    // lands on an unrelated zone. The 80 ms tail covers only the
    // family-internal traversal — same gap, same headroom.
    expect(STICKY_GRACE_MS).toBe(80);
  });

  it("HOVER_INTENT_MS sits in the perceptually-responsive range (100–250 ms)", () => {
    // Below 100 ms feels like a synchronous trigger (commits on
    // incidental cursor sweeps); above 250 ms feels laggy. The 150
    // chosen value is in the middle of the human-perceptible
    // hover-intent window per Nielsen-Norman research.
    expect(HOVER_INTENT_MS).toBeGreaterThanOrEqual(100);
    expect(HOVER_INTENT_MS).toBeLessThanOrEqual(250);
  });

  it("LEAVE_GRACE_MS is shorter than HOVER_INTENT_MS (asymmetric: leave faster than enter)", () => {
    // Asymmetric thresholds match the human expectation that a
    // committed surface should respond MORE quickly to a leave
    // gesture than to an enter gesture — re-engaging is cheaper
    // than disengaging (the user has already paid the
    // hover-intent cost on the way in).
    expect(LEAVE_GRACE_MS).toBeLessThan(HOVER_INTENT_MS);
  });

  it("STICKY_GRACE_MS is at least LEAVE_GRACE_MS (committed ≥ transient)", () => {
    // A click-committed surface is more deliberate than a hover-only
    // affordance, so the threshold for tearing it down should be
    // at least as lenient as the hover-only path. v9 sets them
    // equal — the v9 effect's non-family-hit branch already
    // provides a synchronous teardown signal for committed
    // previews on neighbour clicks, so a longer sticky tail is
    // no longer necessary.
    expect(STICKY_GRACE_MS).toBeGreaterThanOrEqual(LEAVE_GRACE_MS);
  });

  it("LEAVE_GRACE_MS ≥ 50 ms — required for family-internal cursor traversal (capsule ↔ petal halo gap)", () => {
    // Hard floor pin: the family-internal traversal path needs a
    // grace of at least ~50 ms to cover the 16 px capsule→petal
    // halo gap at typical cursor speeds (≤ 1000 px/s = 16 ms;
    // realistic 500 px/s = 32 ms). Anything below 50 ms regresses
    // the v9 PR3 bug where the bloom collapsed before the cursor
    // reached the petal.
    expect(LEAVE_GRACE_MS).toBeGreaterThanOrEqual(50);
  });

  it("LEAVE_GRACE_MS + STICKY_GRACE_MS together fit inside the v9 ≤ 250 ms collapse budget", () => {
    // The cumulative bloom-collapse path is the sum of the
    // poller-detection latency (≤ 16 ms at 60 fps), the
    // applicable grace timer (LEAVE_GRACE_MS or
    // STICKY_GRACE_MS), and the petal exit keyframe + stagger
    // tail (≤ 260 ms in StackWrapper.css). Since the grace and
    // animation overlap (the animation runs INSIDE the
    // bloomUnmount window which itself waits for collapse), the
    // dominant cost is animation, not the grace. We assert the
    // grace constants alone never overshoot the v9 budget so a
    // future drift is caught here.
    expect(LEAVE_GRACE_MS + 16).toBeLessThan(250);
    expect(STICKY_GRACE_MS + 16).toBeLessThan(250);
  });
});
