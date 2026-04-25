/**
 * Tests for anchorOrigin — the helpers that map BentoZone anchor snapshots
 * to CSS transform-origin keywords driving the spring-expand animation.
 *
 * Reproduces the v1.2.2 #5 bug: when the snapshot was cleared synchronously
 * on collapse, transform-origin yanked back to "left top" mid-spring and
 * the capsule appeared to flash to the bottom-right corner.
 */
import { describe, it, expect } from "vitest";
import {
  computeTransformOrigin,
  computeZonePositionStyle,
  canReleaseSnapshot,
  SPRING_TRANSITION_MS,
  type AnchorSnapshot,
} from "../anchorOrigin";

const make = (
  x: AnchorSnapshot["x"],
  y: AnchorSnapshot["y"],
  flipOffsetX = 0,
  flipOffsetY = 0,
): AnchorSnapshot => ({ x, y, flipOffsetX, flipOffsetY });

describe("computeTransformOrigin", () => {
  it("returns left/top when snapshot is null (default capsule placement)", () => {
    expect(computeTransformOrigin(null)).toEqual({ x: "left", y: "top" });
  });

  it("returns left/top for natural-anchor snapshots", () => {
    expect(computeTransformOrigin(make("left", "top"))).toEqual({
      x: "left",
      y: "top",
    });
  });

  it("returns right/center origin component for right-edge anchors", () => {
    expect(computeTransformOrigin(make("right", "top"))).toEqual({
      x: "right",
      y: "top",
    });
  });

  it("returns center/bottom origin component for bottom-edge anchors", () => {
    expect(computeTransformOrigin(make("left", "bottom"))).toEqual({
      x: "left",
      y: "bottom",
    });
  });

  it("returns right/bottom origin for bottom-right corner anchor", () => {
    // The exact case from the user video: capsule sits in the bottom-right
    // corner. transform-origin must be "right bottom" so the spring grows
    // toward the centre and retracts back into the same corner — never the
    // top-left default that caused the v1.2.2 flash.
    expect(computeTransformOrigin(make("right", "bottom"))).toEqual({
      x: "right",
      y: "bottom",
    });
  });

  it("ignores flipOffset values when mapping to CSS keywords", () => {
    expect(computeTransformOrigin(make("right", "bottom", 42, 99))).toEqual({
      x: "right",
      y: "bottom",
    });
  });
});

describe("canReleaseSnapshot", () => {
  it("releases immediately when snapshot is null", () => {
    expect(
      canReleaseSnapshot({
        expanded: false,
        snapshot: null,
        collapseStartedAt: null,
      }),
    ).toBe(true);
  });

  it("never releases while expanded", () => {
    const t0 = 1_000;
    expect(
      canReleaseSnapshot({
        expanded: true,
        snapshot: make("right", "bottom"),
        collapseStartedAt: t0,
        now: t0 + SPRING_TRANSITION_MS * 5,
      }),
    ).toBe(false);
  });

  it("never releases when collapse hasn't started", () => {
    expect(
      canReleaseSnapshot({
        expanded: false,
        snapshot: make("right", "bottom"),
        collapseStartedAt: null,
        now: 5_000,
      }),
    ).toBe(false);
  });

  it("does not release before the spring transition has fully settled", () => {
    const t0 = 1_000;
    expect(
      canReleaseSnapshot({
        expanded: false,
        snapshot: make("right", "bottom"),
        collapseStartedAt: t0,
        now: t0 + SPRING_TRANSITION_MS - 10,
      }),
    ).toBe(false);
  });

  it("releases once the spring transition has fully settled", () => {
    const t0 = 1_000;
    expect(
      canReleaseSnapshot({
        expanded: false,
        snapshot: make("right", "bottom"),
        collapseStartedAt: t0,
        now: t0 + SPRING_TRANSITION_MS,
      }),
    ).toBe(true);
  });

  it("releases for any anchor edge after the transition settles", () => {
    const t0 = 0;
    const now = t0 + SPRING_TRANSITION_MS + 100;
    for (const x of ["left", "right"] as const) {
      for (const y of ["top", "bottom"] as const) {
        expect(
          canReleaseSnapshot({
            expanded: false,
            snapshot: make(x, y),
            collapseStartedAt: t0,
            now,
          }),
        ).toBe(true);
      }
    }
  });
});

describe("SPRING_TRANSITION_MS", () => {
  it("matches the .spring-expand transition duration with a small cushion", () => {
    // animations.css keeps width/height transitions at 0.5s. The cushion
    // ensures the snapshot is held until after the overshoot has fully
    // relaxed — a value below 500 would cause early release and reproduce
    // the v1.2.2 flash.
    expect(SPRING_TRANSITION_MS).toBeGreaterThanOrEqual(500);
    expect(SPRING_TRANSITION_MS).toBeLessThanOrEqual(700);
  });
});

describe("computeZonePositionStyle", () => {
  const POS = { x_percent: 92, y_percent: 88 };

  it("uses left/top percent when no snapshot is present (initial mount)", () => {
    expect(
      computeZonePositionStyle({
        snapshot: null,
        pos: POS,
        isDraggingZen: false,
      }),
    ).toEqual({ left: "92%", top: "88%" });
  });

  it("emits right/bottom px when snapshot anchors to bottom-right corner", () => {
    expect(
      computeZonePositionStyle({
        snapshot: make("right", "bottom", 24, 32),
        pos: POS,
        isDraggingZen: false,
      }),
    ).toEqual({ right: "24px", bottom: "32px" });
  });

  // The team-lead-handoff scenario: a zone parked at the right edge must
  // emit the SAME right-anchor coordinate system in both the expanded and
  // zen states so the spring transition can interpolate width/height
  // without the rendered position jumping. We assert the style is
  // coordinate-system-stable across an expand → collapse cycle.
  it("KEY: keeps right-anchor stable across expand → collapse (no left/right flip)", () => {
    const snapshot = make("right", "top", 48, 0);
    // Frame A: expanded, snapshot active
    const expanded = computeZonePositionStyle({
      snapshot,
      pos: POS,
      isDraggingZen: false,
    });
    // Frame B: collapsed but snapshot still held during the spring
    const collapsing = computeZonePositionStyle({
      snapshot,
      pos: POS,
      isDraggingZen: false,
    });
    // Same anchor side in both frames — no system change for the browser
    // to fail to interpolate. This is what fixes the "flash" symptom.
    expect(expanded.right).toBe("48px");
    expect(collapsing.right).toBe("48px");
    expect(expanded.left).toBeUndefined();
    expect(collapsing.left).toBeUndefined();
  });

  it("KEY: keeps bottom-anchor stable across expand → collapse", () => {
    const snapshot = make("left", "bottom", 0, 64);
    const expanded = computeZonePositionStyle({
      snapshot,
      pos: POS,
      isDraggingZen: false,
    });
    const collapsing = computeZonePositionStyle({
      snapshot,
      pos: POS,
      isDraggingZen: false,
    });
    expect(expanded.bottom).toBe("64px");
    expect(collapsing.bottom).toBe("64px");
    expect(expanded.top).toBeUndefined();
    expect(collapsing.top).toBeUndefined();
  });

  it("falls back to left/top % during a live zen drag (right-anchor offset is stale)", () => {
    // While dragging, the captured flipOffsetX is no longer the capsule's
    // current offset from the viewport edge — using it would freeze the
    // capsule mid-drag.
    const draggingPos = { x_percent: 12, y_percent: 50 };
    const out = computeZonePositionStyle({
      snapshot: make("right", "bottom", 48, 64),
      pos: draggingPos,
      isDraggingZen: true,
    });
    expect(out).toEqual({ left: "12%", top: "50%" });
  });

  it("each axis is exclusive — exactly one of {left,right} and {top,bottom}", () => {
    for (const x of ["left", "right"] as const) {
      for (const y of ["top", "bottom"] as const) {
        const style = computeZonePositionStyle({
          snapshot: make(x, y, 10, 20),
          pos: POS,
          isDraggingZen: false,
        });
        const horizontal =
          (style.left !== undefined ? 1 : 0) +
          (style.right !== undefined ? 1 : 0);
        const vertical =
          (style.top !== undefined ? 1 : 0) +
          (style.bottom !== undefined ? 1 : 0);
        expect(horizontal).toBe(1);
        expect(vertical).toBe(1);
      }
    }
  });

  it("forwards the exact flipOffset px values without rounding", () => {
    const out = computeZonePositionStyle({
      snapshot: make("right", "bottom", 7, 13),
      pos: POS,
      isDraggingZen: false,
    });
    expect(out.right).toBe("7px");
    expect(out.bottom).toBe("13px");
  });
});
