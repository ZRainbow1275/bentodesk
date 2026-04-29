/**
 * v8 round-14 — shared hover-intent timing constants.
 *
 * These tests pin the exported values so future refactors can't
 * silently drift the wake/leave timing thresholds. Drift between
 * external-zone and bloom-petal timing is exactly the regression
 * round-14 was created to fix ("打开stack后内部的zone的唤醒和离开
 * 与外部的zone不一致，统一").
 *
 * The values themselves are UX decisions — changing them affects
 * the perceived responsiveness of every hover surface. If a future
 * round needs to adjust these, update the test thresholds AND the
 * corresponding constants in lockstep.
 */
import { describe, it, expect } from "vitest";
import {
  HOVER_INTENT_MS,
  LEAVE_GRACE_MS,
  STICKY_GRACE_MS,
} from "../hoverIntent";

describe("v8 round-14 — shared hover-intent constants", () => {
  it("exports HOVER_INTENT_MS = 150 (matches settings default expand_delay_ms)", () => {
    expect(HOVER_INTENT_MS).toBe(150);
  });

  it("exports LEAVE_GRACE_MS = 80 (matches the round-13 active-revert grace)", () => {
    expect(LEAVE_GRACE_MS).toBe(80);
  });

  it("exports STICKY_GRACE_MS = 200 (longer than LEAVE_GRACE_MS for committed surfaces)", () => {
    expect(STICKY_GRACE_MS).toBe(200);
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

  it("STICKY_GRACE_MS is longer than LEAVE_GRACE_MS (committed > transient)", () => {
    // A click-committed surface is more deliberate than a hover-only
    // affordance, so the threshold for tearing it down should be
    // proportionally more lenient.
    expect(STICKY_GRACE_MS).toBeGreaterThan(LEAVE_GRACE_MS);
  });
});
