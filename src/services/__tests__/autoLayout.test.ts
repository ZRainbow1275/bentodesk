/**
 * Tests for auto-layout algorithms.
 *
 * These verify the pure math in `computeLayout` — the IPC-writing
 * `applyLayout` is exercised in the e2e smoke manual steps only, since
 * unit-testing it would require mocking `bulkUpdateZones`.
 */
import { describe, it, expect } from "vitest";
import { computeLayout } from "../autoLayout";
import type { BentoZone } from "../../types/zone";

function mkZones(n: number): BentoZone[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `z${i}`,
    name: `Zone ${i}`,
    icon: "folder",
    position: { x_percent: 0, y_percent: 0 },
    expanded_size: { w_percent: 20, h_percent: 20 },
    items: [],
    accent_color: null,
    sort_order: i,
    auto_group: null,
    grid_columns: 4,
    created_at: "",
    updated_at: "",
    capsule_size: "medium" as const,
    capsule_shape: "pill" as const,
  }));
}

describe("computeLayout", () => {
  it("returns empty array for zero zones", () => {
    expect(computeLayout("grid", [])).toEqual([]);
  });

  it("grid arranges 24 zones into ceil(sqrt) columns", () => {
    const pts = computeLayout("grid", mkZones(24));
    expect(pts).toHaveLength(24);
    const firstRowY = pts[0].y;
    const sameRowPts = pts.filter((p) => Math.abs(p.y - firstRowY) < 0.001);
    expect(sameRowPts.length).toBe(Math.ceil(Math.sqrt(24)));
  });

  it("row keeps all zones on a single y", () => {
    const pts = computeLayout("row", mkZones(6));
    const ys = new Set(pts.map((p) => p.y));
    expect(ys.size).toBe(1);
  });

  it("column keeps all zones on a single x", () => {
    const pts = computeLayout("column", mkZones(6));
    const xs = new Set(pts.map((p) => p.x));
    expect(xs.size).toBe(1);
  });

  it("spiral produces roughly equidistant points (no two identical)", () => {
    const pts = computeLayout("spiral", mkZones(12));
    for (let i = 1; i < pts.length; i++) {
      const dx = pts[i].x - pts[i - 1].x;
      const dy = pts[i].y - pts[i - 1].y;
      const d = Math.sqrt(dx * dx + dy * dy);
      expect(d).toBeGreaterThan(0.01);
    }
  });

  it("organic converges inside the viewport", () => {
    const pts = computeLayout("organic", mkZones(10));
    for (const p of pts) {
      expect(p.x).toBeGreaterThanOrEqual(5);
      expect(p.x).toBeLessThanOrEqual(95);
      expect(p.y).toBeGreaterThanOrEqual(5);
      expect(p.y).toBeLessThanOrEqual(95);
    }
  });
});
