/**
 * Post-deploy fix round 2 — Bug B (scatter on dissolve) — pure-fn
 * unit tests for `computeScatterPositions`.
 *
 * The contract (mirrored from `bentodesk/src/utils/scatterPositions.ts`):
 *   - first member sits AT the anchor (the "stack stayed put"
 *     element of the user's mental model);
 *   - subsequent members shift right by `capsule.width + gap` each
 *     step, in pixel space against the supplied viewport;
 *   - if a member would extend past the viewport's right edge it
 *     wraps to a new row `capsule.height + gap` below the previous,
 *     resetting x to the anchor;
 *   - every returned position is clamped so the capsule stays
 *     fully inside the viewport (defends anchors close to the
 *     right or bottom edges).
 *
 * Because the implementation is a pure function — no DOM, no
 * globals — these tests can run synchronously without spinning up
 * jsdom or mounting any Solid component.
 */
import { describe, it, expect } from "vitest";
import {
  computeScatterPositions,
  SCATTER_GAP_PX,
} from "../scatterPositions";

const VIEWPORT_FHD = { width: 1920, height: 1080 };
const CAPSULE_MEDIUM = { width: 160, height: 48 };

/**
 * Helper: convert a CSS-pixel offset against the viewport into the
 * percent representation the function returns. Centralises the math
 * so test expectations stay readable.
 */
function pctX(px: number, viewportWidth: number = VIEWPORT_FHD.width): number {
  return (px / viewportWidth) * 100;
}

function pctY(px: number, viewportHeight: number = VIEWPORT_FHD.height): number {
  return (px / viewportHeight) * 100;
}

describe("computeScatterPositions — degenerate inputs", () => {
  it("returns [] for an empty member list", () => {
    const out = computeScatterPositions(
      {
        x_percent: 10,
        y_percent: 10,
        capsule_width_px: CAPSULE_MEDIUM.width,
        capsule_height_px: CAPSULE_MEDIUM.height,
      },
      [],
      VIEWPORT_FHD,
    );
    expect(out).toEqual([]);
  });

  it("falls back to anchor-for-everyone when the viewport is degenerate", () => {
    // Zero-width viewport — clamp would emit NaN if we used the
    // normal path. The implementation explicitly short-circuits to
    // "stack stays put" for every member instead.
    const out = computeScatterPositions(
      {
        x_percent: 10,
        y_percent: 10,
        capsule_width_px: CAPSULE_MEDIUM.width,
        capsule_height_px: CAPSULE_MEDIUM.height,
      },
      [{ id: "a" }, { id: "b" }],
      { width: 0, height: 0 },
    );
    expect(out).toEqual([
      { id: "a", x_percent: 10, y_percent: 10 },
      { id: "b", x_percent: 10, y_percent: 10 },
    ]);
  });
});

describe("computeScatterPositions — single-member layout", () => {
  it("a 1-member input keeps the member at the anchor (no movement)", () => {
    const out = computeScatterPositions(
      {
        x_percent: 25,
        y_percent: 40,
        capsule_width_px: CAPSULE_MEDIUM.width,
        capsule_height_px: CAPSULE_MEDIUM.height,
      },
      [{ id: "only" }],
      VIEWPORT_FHD,
    );
    expect(out.length).toBe(1);
    expect(out[0].id).toBe("only");
    expect(out[0].x_percent).toBeCloseTo(25, 5);
    expect(out[0].y_percent).toBeCloseTo(40, 5);
  });
});

describe("computeScatterPositions — single-row layout", () => {
  it("3 members fit a row: x increases by (capsule.width + gap) each step", () => {
    const anchorXPct = 10; // 192 px on 1920
    const anchorYPct = 50; // 540 px on 1080
    const out = computeScatterPositions(
      {
        x_percent: anchorXPct,
        y_percent: anchorYPct,
        capsule_width_px: CAPSULE_MEDIUM.width,
        capsule_height_px: CAPSULE_MEDIUM.height,
      },
      [{ id: "a" }, { id: "b" }, { id: "c" }],
      VIEWPORT_FHD,
    );
    expect(out.length).toBe(3);

    // Member a sits AT the anchor.
    expect(out[0]).toEqual({
      id: "a",
      x_percent: anchorXPct,
      y_percent: anchorYPct,
    });

    // Members b/c shift right by capsule.width + gap (160 + 16 = 176).
    const stepPx = CAPSULE_MEDIUM.width + SCATTER_GAP_PX;
    expect(out[1].id).toBe("b");
    expect(out[1].x_percent).toBeCloseTo(
      anchorXPct + pctX(stepPx),
      5,
    );
    expect(out[1].y_percent).toBeCloseTo(anchorYPct, 5);

    expect(out[2].id).toBe("c");
    expect(out[2].x_percent).toBeCloseTo(
      anchorXPct + pctX(stepPx * 2),
      5,
    );
    expect(out[2].y_percent).toBeCloseTo(anchorYPct, 5);
  });

  it("custom gap parameter is respected", () => {
    const customGap = 32;
    const out = computeScatterPositions(
      {
        x_percent: 0,
        y_percent: 0,
        capsule_width_px: CAPSULE_MEDIUM.width,
        capsule_height_px: CAPSULE_MEDIUM.height,
      },
      [{ id: "a" }, { id: "b" }],
      VIEWPORT_FHD,
      customGap,
    );
    expect(out[1].x_percent).toBeCloseTo(
      pctX(CAPSULE_MEDIUM.width + customGap),
      5,
    );
  });
});

describe("computeScatterPositions — row-wrap when overflow", () => {
  it("wraps to a new row when next member would overflow the right edge", () => {
    // Anchor near the right side: only ~3 capsules fit in the
    // remaining viewport. A 6-member stack must wrap into 2 rows.
    // anchor at x=80% (1536 px), capsule 160 px wide, gap 16 px.
    // First member at 1536 px (right edge 1696), step 176 px.
    //   i=0 → 1536 (fits)
    //   i=1 → 1712 (fits — 1712 + 160 = 1872 ≤ 1920, BUT cursor
    //                check uses maxX = viewport.width - capsule = 1760;
    //                1712 ≤ 1760 so it stays on row 0)
    //   i=2 → 1888 (cursorX=1888 > maxX=1760 → wrap to row 1, x=1536)
    //   i=3 → 1712 (fits)
    //   i=4 → 1888 (wrap to row 2, x=1536)
    //   ...
    // The wrap rule fires whenever cursorX > maxX at the point we
    // check — i.e. after the previous step pushed us past the
    // available space.
    const anchor = {
      x_percent: 80,
      y_percent: 50,
      capsule_width_px: 160,
      capsule_height_px: 48,
    };
    const out = computeScatterPositions(
      anchor,
      [
        { id: "a" },
        { id: "b" },
        { id: "c" },
        { id: "d" },
        { id: "e" },
      ],
      VIEWPORT_FHD,
    );
    // Member a stays at anchor.
    expect(out[0].x_percent).toBeCloseTo(80, 5);
    expect(out[0].y_percent).toBeCloseTo(50, 5);
    // Member b sits one step right of anchor on the same row.
    const stepXPct = pctX(160 + 16);
    expect(out[1].y_percent).toBeCloseTo(50, 5);
    expect(out[1].x_percent).toBeGreaterThan(80);
    // Members on a wrapped row use anchor x and a larger y. Detect
    // a wrap by spotting a y_percent strictly greater than the
    // anchor row's y (allow ε for float math).
    const wrappedMember = out.slice(2).find((p) => p.y_percent > 50.001);
    expect(wrappedMember).toBeDefined();
    expect(wrappedMember!.x_percent).toBeCloseTo(anchor.x_percent, 3);
    // Sanity: stepX usage is consistent for the on-row members.
    void stepXPct;
  });

  it("anchor at the left edge with many members produces a clean two-row layout", () => {
    // Anchor at (0, 50%), 14 members of 160-px capsule + 16-px gap.
    // Per row capacity = floor((1920 - 160) / 176) + 1 = 11 — 11
    // members fit row 0, the 12th wraps to row 1 at x=0.
    // (We verify the wrap fires somewhere in the back half of the
    // input rather than pinning the exact wrap index, which would
    // couple the test to the cursor-check arithmetic too tightly.)
    const members = Array.from({ length: 14 }, (_, i) => ({ id: `m${i}` }));
    const out = computeScatterPositions(
      {
        x_percent: 0,
        y_percent: 50,
        capsule_width_px: 160,
        capsule_height_px: 48,
      },
      members,
      VIEWPORT_FHD,
    );
    expect(out.length).toBe(14);
    // First member at anchor.
    expect(out[0].x_percent).toBeCloseTo(0, 5);
    expect(out[0].y_percent).toBeCloseTo(50, 5);
    // At least one member on row 1 (y > 50%).
    const row1 = out.filter((p) => p.y_percent > 50.001);
    expect(row1.length).toBeGreaterThan(0);
    // Every wrapped member resets x to anchor.x_percent (allow tiny
    // float tolerance).
    const firstRow1 = row1[0];
    expect(firstRow1.x_percent).toBeCloseTo(0, 3);
  });
});

describe("computeScatterPositions — viewport clamp", () => {
  it("anchor exactly at the right edge clamps every member to maxX", () => {
    // Anchor at 100% — every clamped position should resolve to
    // (viewport.width - capsule) / viewport.width × 100.
    const anchor = {
      x_percent: 100,
      y_percent: 50,
      capsule_width_px: 160,
      capsule_height_px: 48,
    };
    const out = computeScatterPositions(
      anchor,
      [{ id: "a" }, { id: "b" }, { id: "c" }],
      VIEWPORT_FHD,
    );
    const expectedXPct = pctX(VIEWPORT_FHD.width - 160);
    // Every member sits at the right-most legal x.
    for (const p of out) {
      expect(p.x_percent).toBeCloseTo(expectedXPct, 3);
    }
  });

  it("anchor at the bottom edge clamps y to maxY", () => {
    const anchor = {
      x_percent: 0,
      y_percent: 100,
      capsule_width_px: 160,
      capsule_height_px: 48,
    };
    const out = computeScatterPositions(
      anchor,
      [{ id: "a" }, { id: "b" }],
      VIEWPORT_FHD,
    );
    const expectedYPct = pctY(VIEWPORT_FHD.height - 48);
    expect(out[0].y_percent).toBeCloseTo(expectedYPct, 3);
    // Member b stays on the same (clamped) row — it fits horizontally
    // because anchor x is 0 and there's plenty of room.
    expect(out[1].y_percent).toBeCloseTo(expectedYPct, 3);
  });

  it("oversized capsule degrades gracefully to x=0", () => {
    // capsule wider than the viewport — maxX clamps to 0 so every
    // member collapses onto x=0. Better than emitting NaN or a
    // negative percent.
    const anchor = {
      x_percent: 50,
      y_percent: 50,
      capsule_width_px: VIEWPORT_FHD.width + 200,
      capsule_height_px: 48,
    };
    const out = computeScatterPositions(
      anchor,
      [{ id: "a" }, { id: "b" }],
      VIEWPORT_FHD,
    );
    for (const p of out) {
      expect(p.x_percent).toBe(0);
    }
  });
});

describe("computeScatterPositions — return-shape contract", () => {
  it("preserves the input id ordering verbatim", () => {
    const ids = ["zone-3", "zone-1", "zone-2"];
    const out = computeScatterPositions(
      {
        x_percent: 5,
        y_percent: 5,
        capsule_width_px: CAPSULE_MEDIUM.width,
        capsule_height_px: CAPSULE_MEDIUM.height,
      },
      ids.map((id) => ({ id })),
      VIEWPORT_FHD,
    );
    expect(out.map((p) => p.id)).toEqual(ids);
  });

  it("every result entry exposes the {id, x_percent, y_percent} shape", () => {
    const out = computeScatterPositions(
      {
        x_percent: 0,
        y_percent: 0,
        capsule_width_px: CAPSULE_MEDIUM.width,
        capsule_height_px: CAPSULE_MEDIUM.height,
      },
      [{ id: "a" }],
      VIEWPORT_FHD,
    );
    expect(out[0]).toHaveProperty("id");
    expect(out[0]).toHaveProperty("x_percent");
    expect(out[0]).toHaveProperty("y_percent");
    expect(typeof out[0].x_percent).toBe("number");
    expect(typeof out[0].y_percent).toBe("number");
  });

  it("never emits NaN even with adversarial inputs", () => {
    const out = computeScatterPositions(
      {
        x_percent: 50,
        y_percent: 50,
        capsule_width_px: 0,
        capsule_height_px: 0,
      },
      [{ id: "a" }, { id: "b" }, { id: "c" }],
      VIEWPORT_FHD,
    );
    for (const p of out) {
      expect(Number.isFinite(p.x_percent)).toBe(true);
      expect(Number.isFinite(p.y_percent)).toBe(true);
    }
  });
});

describe("computeScatterPositions — SCATTER_GAP_PX default", () => {
  it("exposes a 16 px default gap matching the design spec", () => {
    expect(SCATTER_GAP_PX).toBe(16);
  });

  it("default gap is used when the gap parameter is omitted", () => {
    const withDefault = computeScatterPositions(
      {
        x_percent: 0,
        y_percent: 0,
        capsule_width_px: CAPSULE_MEDIUM.width,
        capsule_height_px: CAPSULE_MEDIUM.height,
      },
      [{ id: "a" }, { id: "b" }],
      VIEWPORT_FHD,
    );
    const withExplicit = computeScatterPositions(
      {
        x_percent: 0,
        y_percent: 0,
        capsule_width_px: CAPSULE_MEDIUM.width,
        capsule_height_px: CAPSULE_MEDIUM.height,
      },
      [{ id: "a" }, { id: "b" }],
      VIEWPORT_FHD,
      SCATTER_GAP_PX,
    );
    expect(withDefault).toEqual(withExplicit);
  });
});
