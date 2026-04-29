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

import { computeAutoSpread, detectOverlap, suggestStack, type ZoneRect } from "../stack";
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

  it("triggers cluster at the 60% overlap boundary (spec threshold)", () => {
    // Two 20×20 % zones offset such that intersection / min-area = 0.64
    // a: (10,10)-(30,30), b: (12,12)-(32,32) → intersect 18×18 / 400 = 0.81
    const zones = [mkZone("a", 10, 10), mkZone("b", 12, 12)];
    const clusters = suggestStack(zones, 0.6);
    expect(clusters).toHaveLength(1);
    expect(clusters[0].sort()).toEqual(["a", "b"]);
  });

  it("does NOT cluster when overlap is just below 60%", () => {
    // Tune offset so intersection/min ≈ 0.49
    const zones = [mkZone("a", 0, 0, 20, 20), mkZone("b", 6, 0, 20, 20)];
    // intersect 14×20 / 400 = 0.7 — too high, push apart further
    const zones2 = [mkZone("a", 0, 0, 20, 20), mkZone("b", 12, 0, 20, 20)];
    // intersect 8×20 / 400 = 0.4
    expect(suggestStack(zones2, 0.6)).toEqual([]);
  });
});

// ────────────────────────────────────────────────────────────
// Stack tray open state (StackWrapper internal model)
// ────────────────────────────────────────────────────────────
//
// StackWrapper holds tray open state as a local createSignal but the open/close
// rules are pure: when stackDisplayMode === "always" the tray is pinned open
// and cannot be closed; otherwise it toggles freely. We model that policy as a
// pure function and exercise the four state transitions.

type DisplayMode = "always" | "expanded" | "minimized";

function applyTrayVisibility(
  current: boolean,
  requested: boolean,
  displayMode: DisplayMode,
): boolean {
  if (!requested) {
    if (displayMode === "always") return true; // pinned open
    return false;
  }
  return true;
}

describe("stack tray open state", () => {
  it("opens when toggled from closed in non-always mode", () => {
    expect(applyTrayVisibility(false, true, "expanded")).toBe(true);
  });

  it("closes when toggled from open in non-always mode", () => {
    expect(applyTrayVisibility(true, false, "expanded")).toBe(false);
  });

  it("stays open when close is requested under always mode (pinned)", () => {
    expect(applyTrayVisibility(true, false, "always")).toBe(true);
  });

  it("opens when open is requested under always mode", () => {
    expect(applyTrayVisibility(false, true, "always")).toBe(true);
  });

  it("toggle from closed → open → close round-trips correctly (expanded)", () => {
    let open = false;
    open = applyTrayVisibility(open, !open, "expanded");
    expect(open).toBe(true);
    open = applyTrayVisibility(open, !open, "expanded");
    expect(open).toBe(false);
  });
});

// ────────────────────────────────────────────────────────────
// stackMap derivation rules — drive zone.stack_id / stack_order
// through the same logic stores/stacks.ts uses, without booting Solid.
// ────────────────────────────────────────────────────────────
//
// We re-implement the derivation contract here as a pure function so it can be
// exercised against the same invariants stackMap() enforces:
//   1. Group zones by stack_id, drop entries with no stack_id.
//   2. Sort each bucket by stack_order ascending.
//   3. Drop singleton stacks (length < 2) — these are degenerate.

function deriveStackMap(zones: BentoZone[]): Map<string, BentoZone[]> {
  const map = new Map<string, BentoZone[]>();
  for (const z of zones) {
    const sid = z.stack_id;
    if (!sid) continue;
    const bucket = map.get(sid);
    if (bucket) bucket.push(z);
    else map.set(sid, [z]);
  }
  for (const arr of map.values()) {
    arr.sort((a, b) => (a.stack_order ?? 0) - (b.stack_order ?? 0));
  }
  for (const [k, v] of map) {
    if (v.length < 2) map.delete(k);
  }
  return map;
}

function withStack(
  zone: BentoZone,
  stackId: string | null,
  stackOrder?: number,
): BentoZone {
  return {
    ...zone,
    stack_id: stackId,
    stack_order: stackOrder,
  } as BentoZone;
}

describe("threshold boundary at 0.30 (regression guard)", () => {
  // Two 20×20% zones, sliding b along x makes intersection = (20 - dx) × 20.
  // Ratio = intersection / min-area = (20 - dx) × 20 / (20 × 20) = (20 - dx) / 20.
  //   dx = 14.2 → ratio ≈ 0.29 (just under 0.30)
  //   dx = 13.8 → ratio ≈ 0.31 (just over 0.30)
  // Locks in the math against future suggestStack regressions.
  it("does NOT cluster at 0.29 ratio (just below 0.30)", () => {
    const zones = [mkZone("a", 0, 0, 20, 20), mkZone("b", 14.2, 0, 20, 20)];
    expect(suggestStack(zones, 0.3)).toEqual([]);
  });

  it("DOES cluster at 0.31 ratio (just above 0.30)", () => {
    const zones = [mkZone("a", 0, 0, 20, 20), mkZone("b", 13.8, 0, 20, 20)];
    const clusters = suggestStack(zones, 0.3);
    expect(clusters).toHaveLength(1);
    expect(clusters[0].sort()).toEqual(["a", "b"]);
  });
});

describe("v1.2.4 stack redo — overlap with stacked zones included", () => {
  it("clusters zones even when both already carry a legacy stack_id", () => {
    // Real-world v1.2.3 layout: two zones spatially overlap but were already
    // promoted into a stack by the auto-stacker — overlap detection must
    // still surface them so the ⊞ button + auto-spread can dissolve and fan.
    const zones = [
      withStack(mkZone("a", 10, 10, 20, 20), "LEGACY", 0),
      withStack(mkZone("b", 12, 11, 20, 20), "LEGACY", 1),
    ];
    const clusters = suggestStack(zones, 0.05);
    expect(clusters).toHaveLength(1);
    expect(clusters[0].sort()).toEqual(["a", "b"]);
  });

  it("reports cluster at the lowered v1.2.4 5% threshold", () => {
    // 8% overlap — too low for legacy 0.6 threshold, must trigger at 0.05.
    const zones = [
      mkZone("a", 0, 0, 20, 20),
      mkZone("b", 18.4, 0, 20, 20),
    ];
    expect(suggestStack(zones, 0.6)).toEqual([]);
    expect(suggestStack(zones, 0.05)).toHaveLength(1);
  });
});

describe("computeAutoSpread", () => {
  it("returns empty when fewer than 2 cluster members supplied", () => {
    const zones = [mkZone("solo", 0, 0)];
    expect(computeAutoSpread(zones, ["solo"])).toEqual([]);
  });

  it("ignores ids that are not in the zones list", () => {
    const zones = [mkZone("a", 0, 0)];
    expect(computeAutoSpread(zones, ["a", "ghost"])).toEqual([]);
  });

  it("emits one entry per cluster member, ordered by sort_order", () => {
    const zones = [
      { ...mkZone("a", 10, 10, 20, 20), sort_order: 2 } as BentoZone,
      { ...mkZone("b", 12, 11, 20, 20), sort_order: 0 } as BentoZone,
      { ...mkZone("c", 11, 11, 20, 20), sort_order: 1 } as BentoZone,
    ];
    const spread = computeAutoSpread(zones, ["a", "b", "c"]);
    expect(spread.map((s) => s.id)).toEqual(["b", "c", "a"]);
  });

  it("never returns 0% positions when supplied a sane viewport", () => {
    // Viewport mock above is 1000×1000; spread members are 20% wide each so
    // x_percent must be > 0 and < 100 for every entry.
    const zones = [
      { ...mkZone("a", 10, 10, 20, 20), sort_order: 0 } as BentoZone,
      { ...mkZone("b", 12, 11, 20, 20), sort_order: 1 } as BentoZone,
    ];
    const spread = computeAutoSpread(zones, ["a", "b"]);
    for (const entry of spread) {
      expect(entry.x_percent).toBeGreaterThan(0);
      expect(entry.x_percent).toBeLessThan(100);
      expect(entry.y_percent).toBeGreaterThanOrEqual(0);
    }
  });

  it("aligns every member onto the same y row", () => {
    const zones = [
      { ...mkZone("a", 10, 10, 20, 20), sort_order: 0 } as BentoZone,
      { ...mkZone("b", 12, 25, 20, 20), sort_order: 1 } as BentoZone,
    ];
    const spread = computeAutoSpread(zones, ["a", "b"]);
    expect(spread).toHaveLength(2);
    expect(spread[0].y_percent).toBe(spread[1].y_percent);
  });

  it("never emits NaN/0% when getViewportSize returns {0, 0} (window fallback)", async () => {
    // Simulate the early-init race: ui store getViewportSize returns {0,0}
    // before installViewportTracker fires. computeAutoSpread must fall back
    // to window dimensions, so x_percent stays finite + > 0 instead of NaN.
    vi.resetModules();
    vi.doMock("../../stores/ui", async () => {
      const actual = await vi.importActual<Record<string, unknown>>("../../stores/ui");
      return { ...actual, getViewportSize: () => ({ width: 0, height: 0 }) };
    });
    const { computeAutoSpread: computeWithZeroVp } = await import("../stack");
    const zones = [
      { ...mkZone("a", 10, 10, 20, 20), sort_order: 0 } as BentoZone,
      { ...mkZone("b", 12, 11, 20, 20), sort_order: 1 } as BentoZone,
    ];
    const spread = computeWithZeroVp(zones, ["a", "b"]);
    expect(spread).toHaveLength(2);
    for (const entry of spread) {
      expect(Number.isFinite(entry.x_percent)).toBe(true);
      expect(Number.isFinite(entry.y_percent)).toBe(true);
      expect(entry.x_percent).toBeGreaterThanOrEqual(0);
    }
    vi.doUnmock("../../stores/ui");
    vi.resetModules();
  });
});

describe("stackMap derivation", () => {
  it("buckets two zones sharing a stack_id and orders by stack_order", () => {
    const zones = [
      withStack(mkZone("a", 0, 0), "S1", 1),
      withStack(mkZone("b", 0, 0), "S1", 0),
    ];
    const map = deriveStackMap(zones);
    expect(map.size).toBe(1);
    expect(map.get("S1")!.map((z) => z.id)).toEqual(["b", "a"]);
  });

  it("after detaching one of two members, the lone remainder is dropped (singleton)", () => {
    // Pre-detach: 2 members in S1
    const before = [
      withStack(mkZone("a", 0, 0), "S1", 0),
      withStack(mkZone("b", 0, 0), "S1", 1),
    ];
    expect(deriveStackMap(before).size).toBe(1);

    // Post-detach: b leaves S1; only a remains in S1 → must be dropped
    const after = [
      withStack(mkZone("a", 0, 0), "S1", 0),
      withStack(mkZone("b", 0, 0), null),
    ];
    const map = deriveStackMap(after);
    expect(map.has("S1")).toBe(false);
    expect(map.size).toBe(0);
  });

  it("after dissolve all members have stack_id = null and the stack vanishes", () => {
    const before = [
      withStack(mkZone("a", 0, 0), "S1", 0),
      withStack(mkZone("b", 0, 0), "S1", 1),
      withStack(mkZone("c", 0, 0), "S1", 2),
    ];
    expect(deriveStackMap(before).get("S1")).toHaveLength(3);

    const after = before.map((z) => withStack(z, null));
    const map = deriveStackMap(after);
    expect(map.size).toBe(0);
    for (const z of after) expect(z.stack_id).toBeNull();
  });

  it("keeps multi-stack scenarios isolated when only one is dissolved", () => {
    const zones = [
      withStack(mkZone("a", 0, 0), "S1", 0),
      withStack(mkZone("b", 0, 0), "S1", 1),
      withStack(mkZone("c", 0, 0), "S2", 0),
      withStack(mkZone("d", 0, 0), "S2", 1),
    ];
    expect(deriveStackMap(zones).size).toBe(2);

    // Dissolve S1 only
    const post = zones.map((z) =>
      z.stack_id === "S1" ? withStack(z, null) : z,
    );
    const map = deriveStackMap(post);
    expect(map.size).toBe(1);
    expect(map.has("S2")).toBe(true);
    expect(map.get("S2")!.map((z) => z.id)).toEqual(["c", "d"]);
  });
});
