/**
 * v9 — z-stack ladder contract test.
 *
 * Pre-v9 had two divergent z-index ladders (BentoZone idle = sort_order
 * + 10, StackWrapper idle = sort_order + 30) which made stacks
 * unconditionally outrank free zones at rest. The user reported
 * "stack appears on top of zone" with no visual rationale.
 *
 * v9 fix: both surfaces share one ladder defined in `src/styles/zStack`.
 * This test pins down the ladder so a future drift (e.g. someone
 * bumping Z_ZONE_HOVER below Z_ZONE_PROMOTED) fails CI loudly rather
 * than silently regressing the layering.
 *
 * Assertions:
 *   1. Each constant has the documented value.
 *   2. The ladder is monotonically strictly increasing in the order
 *      idle < hover < promoted (bloom) < expanded < drag.
 *   3. `sort_order + Z_ZONE_IDLE_OFFSET` for any reasonable sort_order
 *      (0-700) lands BELOW Z_ZONE_HOVER, so a hovered zone always
 *      outranks an idle one.
 */
import { describe, it, expect } from "vitest";
import {
  Z_ZONE_IDLE_OFFSET,
  Z_ZONE_HOVER,
  Z_ZONE_PROMOTED,
  Z_ZONE_EXPANDED,
  Z_ZONE_DRAG,
} from "../zStack";

describe("v9 z-stack ladder contract", () => {
  it("Z_ZONE_IDLE_OFFSET is 10 (matches pre-v9 BentoZone baseline)", () => {
    expect(Z_ZONE_IDLE_OFFSET).toBe(10);
  });

  it("Z_ZONE_HOVER is 800", () => {
    expect(Z_ZONE_HOVER).toBe(800);
  });

  it("Z_ZONE_PROMOTED (bloom) is 950", () => {
    expect(Z_ZONE_PROMOTED).toBe(950);
  });

  it("Z_ZONE_EXPANDED is 1000", () => {
    expect(Z_ZONE_EXPANDED).toBe(1000);
  });

  it("Z_ZONE_DRAG is 1100 — top of ladder", () => {
    expect(Z_ZONE_DRAG).toBe(1100);
  });

  it("ladder is strictly increasing: idle < hover < promoted < expanded < drag", () => {
    // Pick a reasonable sort_order for the idle baseline. sort_order in
    // production is bounded by the user's zone count (typically <50).
    // We test the upper edge of "reasonable" at 700, which still must
    // sit below HOVER.
    const idleHigh = 700 + Z_ZONE_IDLE_OFFSET;
    expect(idleHigh).toBeLessThan(Z_ZONE_HOVER);
    expect(Z_ZONE_HOVER).toBeLessThan(Z_ZONE_PROMOTED);
    expect(Z_ZONE_PROMOTED).toBeLessThan(Z_ZONE_EXPANDED);
    expect(Z_ZONE_EXPANDED).toBeLessThan(Z_ZONE_DRAG);
  });

  it("sort_order=0 lands at Z_ZONE_IDLE_OFFSET (deterministic baseline)", () => {
    const idle = 0 + Z_ZONE_IDLE_OFFSET;
    expect(idle).toBe(10);
  });

  it("a hovered zone outranks any reasonable idle zone (sort_order ≤ 700)", () => {
    for (const sortOrder of [0, 1, 50, 100, 500, 700]) {
      expect(sortOrder + Z_ZONE_IDLE_OFFSET).toBeLessThan(Z_ZONE_HOVER);
    }
  });
});
