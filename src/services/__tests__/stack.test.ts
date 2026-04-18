/**
 * Tests for D2 stack detection — overlap ratio math and union-find grouping.
 *
 * We unit-test `detectOverlap` directly (pure function) and `suggestStack`
 * with a mocked monitor/viewport so rects live on a single synthetic display.
 */
import { describe, it, expect, vi } from "vitest";

vi.mock("../geometry", () => ({
  monitorForClientRect: () => ({
    rect_full: { x: 0, y: 0, width: 1920, height: 1080 },
  }),
}));

vi.mock("../../stores/ui", async () => {
  const actual = await vi.importActual<Record<string, unknown>>("../../stores/ui");
  return {
    ...actual,
    getViewportSize: () => ({ width: 1000, height: 1000 }),
  };
});

import { detectOverlap, suggestStack, type ZoneRect } from "../stack";
import type { BentoZone } from "../../types/zone";

function rect(id: string, left: number, top: number, w: number, h: number): ZoneRect {
  return { id, left, top, width: w, height: h };
}

describe("detectOverlap", () => {
  it("returns 0 for disjoint rects", () => {
    const a = rect("a", 0, 0, 100, 100);
    const b = rect("b", 200, 0, 100, 100);
    expect(detectOverlap(a, b)).toBe(0);
  });

  it("returns ~1 when small rect is fully inside large rect", () => {
    const big = rect("big", 0, 0, 1000, 1000);
    const small = rect("small", 100, 100, 50, 50);
    expect(detectOverlap(big, small)).toBeCloseTo(1, 5);
  });

  it("returns 0.25 when quarter of min rect overlaps", () => {
    const a = rect("a", 0, 0, 100, 100);
    // 50×50 intersection over min area 100×100 = 0.25
    const b = rect("b", 50, 50, 100, 100);
    expect(detectOverlap(a, b)).toBeCloseTo(0.25, 5);
  });
});

function mkZone(id: string, x: number, y: number, w = 20, h = 20): BentoZone {
  return {
    id,
    name: id,
    icon: "folder",
    position: { x_percent: x, y_percent: y },
    expanded_size: { w_percent: w, h_percent: h },
    grid_columns: 4,
    accent_color: null,
    items: [],
  } as unknown as BentoZone;
}

describe("suggestStack", () => {
  it("returns no clusters when zones are separated", () => {
    const zones = [mkZone("a", 0, 0), mkZone("b", 50, 50)];
    expect(suggestStack(zones)).toEqual([]);
  });

  it("groups heavily-overlapping zones into a cluster", () => {
    const zones = [
      mkZone("a", 10, 10, 20, 20),
      mkZone("b", 12, 11, 20, 20), // near-identical placement → >60% overlap
    ];
    const clusters = suggestStack(zones, 0.6);
    expect(clusters).toHaveLength(1);
    expect(clusters[0].sort()).toEqual(["a", "b"]);
  });

  it("respects the threshold parameter", () => {
    const zones = [
      mkZone("a", 0, 0, 20, 20),
      mkZone("b", 10, 0, 20, 20), // 50% horizontal overlap → ratio 0.5
    ];
    expect(suggestStack(zones, 0.6)).toEqual([]);
    expect(suggestStack(zones, 0.4)).toHaveLength(1);
  });

  it("chains transitive overlaps into a single cluster", () => {
    const zones = [
      mkZone("a", 10, 10),
      mkZone("b", 11, 10),
      mkZone("c", 10, 11),
    ];
    const clusters = suggestStack(zones, 0.6);
    expect(clusters).toHaveLength(1);
    expect(clusters[0].sort()).toEqual(["a", "b", "c"]);
  });
});
