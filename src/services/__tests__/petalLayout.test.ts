/**
 * Regression tests for v8 round-12 — petal-row layout solver.
 *
 * Round-11 placed bloom petals at polar coordinates around the cursor
 * with a collision-avoidance solver. Live testing showed the radial
 * layout failing visually at viewport edges. Round-12 replaces the
 * solver with a far simpler row layout: petals always render as a
 * horizontal row directly below the stack capsule, side by side.
 *
 * These tests pin the solver's contract:
 *   1. Centred row when capsule is in the middle of the viewport.
 *   2. Multiple petals fit in a single row when there is room.
 *   3. Right-edge clamp when capsule is near the right edge.
 *   4. Left-edge clamp when capsule is near the left edge.
 *   5. Vertical flip when capsule is near the bottom edge.
 *   6. Multi-row wrap when the single row cannot fit horizontally.
 *   7. Flip + clamp combined when capsule is in the bottom-right corner.
 *   8. Single-petal degenerate case.
 *   9. Custom gap / gapBelowCapsule respected.
 */
import { describe, it, expect } from "vitest";
import { resolvePetalRow, pickPetalSize } from "../petalLayout";

const DEFAULT_PETAL_SIZE = { width: 108, height: 96 };
const DEFAULT_VIEWPORT = { width: 1920, height: 1080 };
const CAPSULE_W = 220;
const CAPSULE_H = 52;

/** Build a capsule rect at the given top-left corner. */
function capsuleAt(x: number, y: number, w = CAPSULE_W, h = CAPSULE_H) {
  return { x, y, width: w, height: h };
}

describe("resolvePetalRow", () => {
  it("places 2 petals as a centred row below the capsule when there is room", () => {
    const capsule = capsuleAt(800, 400);
    const result = resolvePetalRow({
      capsuleRect: capsule,
      petalSize: DEFAULT_PETAL_SIZE,
      petalCount: 2,
      viewport: DEFAULT_VIEWPORT,
    });
    expect(result.flipped).toBe(false);
    expect(result.wrapped).toBe(false);
    expect(result.centers).toHaveLength(2);
    // Both petals share the same Y (single-row layout).
    expect(result.centers[0].y).toBe(result.centers[1].y);
    // Y is below the capsule by gapBelowCapsule (16) → 400 + 52 + 16 = 468.
    expect(result.centers[0].y).toBe(capsule.y + capsule.height + 16);
    // Centred horizontally on capsule centre (800 + 220/2 = 910).
    // Row width = 2 * 108 + 1 * 12 = 228. rowLeft = 910 - 114 = 796.
    expect(result.centers[0].x).toBe(796);
    expect(result.centers[1].x).toBe(796 + 108 + 12); // 916
  });

  it("places 4 petals as a single row when there is room", () => {
    const capsule = capsuleAt(800, 400);
    const result = resolvePetalRow({
      capsuleRect: capsule,
      petalSize: DEFAULT_PETAL_SIZE,
      petalCount: 4,
      viewport: DEFAULT_VIEWPORT,
    });
    expect(result.wrapped).toBe(false);
    expect(result.flipped).toBe(false);
    expect(result.centers).toHaveLength(4);
    // Every petal shares the same Y (single row).
    const ys = new Set(result.centers.map((c) => c.y));
    expect(ys.size).toBe(1);
    // Petals are evenly spaced by petalSize.width + gap.
    const spacing = result.centers[1].x - result.centers[0].x;
    expect(spacing).toBe(DEFAULT_PETAL_SIZE.width + 12);
    expect(result.centers[2].x - result.centers[1].x).toBe(spacing);
    expect(result.centers[3].x - result.centers[2].x).toBe(spacing);
  });

  it("clamps the row to the right edge when capsule is near the right side", () => {
    // Viewport 1920 wide, capsule near right edge with center close to 1900.
    const capsule = capsuleAt(1700, 100); // centre = 1810
    const result = resolvePetalRow({
      capsuleRect: capsule,
      petalSize: DEFAULT_PETAL_SIZE,
      petalCount: 4,
      viewport: DEFAULT_VIEWPORT,
    });
    expect(result.flipped).toBe(false);
    expect(result.wrapped).toBe(false);
    // Row width = 4 * 108 + 3 * 12 = 468. Without clamp, rowLeft would
    // be 1810 - 234 = 1576, rowRight = 1576 + 468 = 2044 > 1920 - 16
    // = 1904. Clamped to maxLeft = 1920 - 16 - 468 = 1436.
    expect(result.centers[0].x).toBe(1436);
    // Last petal right edge sits exactly at viewport.width - 16.
    const lastPetalLeft =
      result.centers[result.centers.length - 1].x;
    expect(lastPetalLeft + DEFAULT_PETAL_SIZE.width).toBe(
      DEFAULT_VIEWPORT.width - 16,
    );
  });

  it("clamps the row to the left edge when capsule is near the left side", () => {
    // Capsule at x=10 (very near left edge), centre = 120.
    const capsule = capsuleAt(10, 100); // centre = 120
    const result = resolvePetalRow({
      capsuleRect: capsule,
      petalSize: DEFAULT_PETAL_SIZE,
      petalCount: 4,
      viewport: DEFAULT_VIEWPORT,
    });
    expect(result.flipped).toBe(false);
    expect(result.wrapped).toBe(false);
    // Row width = 468. Without clamp, rowLeft = 120 - 234 = -114.
    // Clamped to viewportInset = 16.
    expect(result.centers[0].x).toBe(16);
    // Last petal sits at 16 + 3 * (108 + 12) = 376.
    expect(result.centers[3].x).toBe(16 + 3 * (108 + 12));
  });

  it("flips the row above the capsule when capsule is near the bottom edge", () => {
    // Capsule at the bottom of the viewport.
    const capsule = capsuleAt(800, 1010); // bottom = 1062
    // 1062 + 16 + 96 = 1174 > 1080 - 16 = 1064 → flip above.
    const result = resolvePetalRow({
      capsuleRect: capsule,
      petalSize: DEFAULT_PETAL_SIZE,
      petalCount: 3,
      viewport: DEFAULT_VIEWPORT,
    });
    expect(result.flipped).toBe(true);
    expect(result.wrapped).toBe(false);
    // Row Y = capsule.y - gap - petalH = 1010 - 16 - 96 = 898.
    expect(result.centers[0].y).toBe(898);
    // Every petal in the same row (Y is identical).
    const ys = new Set(result.centers.map((c) => c.y));
    expect(ys.size).toBe(1);
  });

  it("wraps to multiple rows when too many petals to fit horizontally", () => {
    // 12 petals, narrow viewport 800 wide.
    // Row width for 12 = 12 * 108 + 11 * 12 = 1296 + 132 = 1428 > 800 - 32 = 768.
    // petalsPerRow = floor((768 + 12) / (108 + 12)) = floor(780 / 120) = 6.
    // totalRows = ceil(12 / 6) = 2.
    const narrowViewport = { width: 800, height: 1080 };
    const capsule = capsuleAt(300, 200); // centre = 410
    const result = resolvePetalRow({
      capsuleRect: capsule,
      petalSize: DEFAULT_PETAL_SIZE,
      petalCount: 12,
      viewport: narrowViewport,
    });
    expect(result.wrapped).toBe(true);
    expect(result.flipped).toBe(false);
    expect(result.centers).toHaveLength(12);
    // Row 0 (indices 0–5) shares one Y, row 1 (indices 6–11) shares another.
    const row0Y = result.centers[0].y;
    const row1Y = result.centers[6].y;
    expect(row0Y).not.toBe(row1Y);
    expect(row1Y).toBeGreaterThan(row0Y);
    expect(row1Y - row0Y).toBe(DEFAULT_PETAL_SIZE.height + 12);
    // All petals in row 0 share row0Y.
    for (let i = 0; i < 6; i++) {
      expect(result.centers[i].y).toBe(row0Y);
    }
    for (let i = 6; i < 12; i++) {
      expect(result.centers[i].y).toBe(row1Y);
    }
  });

  it("handles capsule in the bottom-right corner (flipped + clamped)", () => {
    // Capsule near both bottom and right edges.
    const capsule = capsuleAt(1700, 1010); // bottom = 1062, far right
    const result = resolvePetalRow({
      capsuleRect: capsule,
      petalSize: DEFAULT_PETAL_SIZE,
      petalCount: 3,
      viewport: DEFAULT_VIEWPORT,
    });
    expect(result.flipped).toBe(true);
    expect(result.wrapped).toBe(false);
    // Y = 1010 - 16 - 96 = 898.
    expect(result.centers[0].y).toBe(898);
    // Row width 3 = 3*108 + 2*12 = 348. capsule centre = 1810.
    // rowLeft = 1810 - 174 = 1636. rowRight = 1636 + 348 = 1984 > 1904.
    // Clamped to maxLeft = 1920 - 16 - 348 = 1556.
    expect(result.centers[0].x).toBe(1556);
    // Last petal right edge = 1556 + 348 = 1904 = viewport.width - 16.
    expect(result.centers[2].x + DEFAULT_PETAL_SIZE.width).toBe(1904);
  });

  it("returns a single petal centred below capsule for the degenerate count=1 case", () => {
    const capsule = capsuleAt(800, 400);
    const result = resolvePetalRow({
      capsuleRect: capsule,
      petalSize: DEFAULT_PETAL_SIZE,
      petalCount: 1,
      viewport: DEFAULT_VIEWPORT,
    });
    expect(result.centers).toHaveLength(1);
    expect(result.flipped).toBe(false);
    expect(result.wrapped).toBe(false);
    // Row width = 108 (no gap). Centre on capsule centre 910.
    // rowLeft = 910 - 54 = 856.
    expect(result.centers[0].x).toBe(856);
    expect(result.centers[0].y).toBe(capsule.y + capsule.height + 16);
  });

  it("returns empty centres when petalCount is 0", () => {
    const capsule = capsuleAt(800, 400);
    const result = resolvePetalRow({
      capsuleRect: capsule,
      petalSize: DEFAULT_PETAL_SIZE,
      petalCount: 0,
      viewport: DEFAULT_VIEWPORT,
    });
    expect(result.centers).toHaveLength(0);
    expect(result.flipped).toBe(false);
    expect(result.wrapped).toBe(false);
  });

  it("respects custom gap and gapBelowCapsule", () => {
    const capsule = capsuleAt(800, 400);
    const result = resolvePetalRow({
      capsuleRect: capsule,
      petalSize: DEFAULT_PETAL_SIZE,
      petalCount: 3,
      viewport: DEFAULT_VIEWPORT,
      gap: 24,
      gapBelowCapsule: 32,
    });
    // Y = 400 + 52 + 32 = 484.
    expect(result.centers[0].y).toBe(484);
    // Petal spacing = 108 + 24 = 132.
    expect(result.centers[1].x - result.centers[0].x).toBe(132);
    expect(result.centers[2].x - result.centers[1].x).toBe(132);
  });

  it("never produces NaN/Infinity centres on extreme inputs", () => {
    const capsule = capsuleAt(0, 0);
    const result = resolvePetalRow({
      capsuleRect: capsule,
      petalSize: DEFAULT_PETAL_SIZE,
      petalCount: 5,
      viewport: { width: 100, height: 100 }, // pathologically tiny
    });
    // Row should wrap (single-row 5 = 540 + 48 = 588 > 100 - 32 = 68).
    expect(result.wrapped).toBe(true);
    for (const c of result.centers) {
      expect(Number.isFinite(c.x)).toBe(true);
      expect(Number.isFinite(c.y)).toBe(true);
    }
  });
});

/**
 * v8 round-14 — adaptive petal sizing for many-member stacks.
 *
 * `pickPetalSize` returns one of four buckets keyed on member count.
 * The buckets are tuned so that a 12-member stack (the user's typical
 * "many zones" feedback shape) fits cleanly in two rows on a 1920-wide
 * viewport instead of three rows at the round-12 default size.
 */
describe("pickPetalSize (v8 round-14)", () => {
  it("returns the round-12 default 108×96 with a 36 px icon for ≤ 4 members", () => {
    expect(pickPetalSize(1)).toEqual({ width: 108, height: 96, iconSize: 36 });
    expect(pickPetalSize(2)).toEqual({ width: 108, height: 96, iconSize: 36 });
    expect(pickPetalSize(4)).toEqual({ width: 108, height: 96, iconSize: 36 });
  });

  it("returns 92×84 with a 32 px icon for 5–8 members", () => {
    expect(pickPetalSize(5)).toEqual({ width: 92, height: 84, iconSize: 32 });
    expect(pickPetalSize(8)).toEqual({ width: 92, height: 84, iconSize: 32 });
  });

  it("returns 80×72 with a 28 px icon for 9–16 members", () => {
    expect(pickPetalSize(9)).toEqual({ width: 80, height: 72, iconSize: 28 });
    expect(pickPetalSize(12)).toEqual({ width: 80, height: 72, iconSize: 28 });
    expect(pickPetalSize(16)).toEqual({ width: 80, height: 72, iconSize: 28 });
  });

  it("returns the compact 72×64 with a 24 px icon for > 16 members", () => {
    expect(pickPetalSize(17)).toEqual({ width: 72, height: 64, iconSize: 24 });
    expect(pickPetalSize(24)).toEqual({ width: 72, height: 64, iconSize: 24 });
    expect(pickPetalSize(50)).toEqual({ width: 72, height: 64, iconSize: 24 });
  });

  it("buckets are monotonically non-increasing in size as count grows", () => {
    const counts = [1, 4, 5, 8, 9, 16, 17, 24];
    let prevW = Infinity;
    let prevH = Infinity;
    let prevIcon = Infinity;
    for (const n of counts) {
      const s = pickPetalSize(n);
      expect(s.width).toBeLessThanOrEqual(prevW);
      expect(s.height).toBeLessThanOrEqual(prevH);
      expect(s.iconSize).toBeLessThanOrEqual(prevIcon);
      prevW = s.width;
      prevH = s.height;
      prevIcon = s.iconSize;
    }
  });
});
