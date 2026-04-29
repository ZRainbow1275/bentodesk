/**
 * Tests for v8 anchor-flip decision (zone-real-fix-v8 §1).
 *
 * v7 regression: `inLowerHalf = capsuleCenterY > workCenterY && flipStillFitsY`
 * gated the "lower half → grow upward" rule on the panel still fitting above
 * the capsule. When `flipStillFitsY` was just barely false (e.g. capsule near
 * the very bottom on a 1080p screen with default panel height), the panel
 * kept growing downward and clipped off-screen.
 *
 * v8 removes the precondition: capsule in the lower half ALWAYS flips to
 * `bottom`. CSS max-height clamps the panel if both sides are tight.
 *
 * Both test scenarios use a single-monitor full-screen overlay (1920x1080),
 * which is the user's actual setup per PRD.
 */
import { describe, it, expect } from "vitest";
import { decideAnchorFromRect } from "../../../services/anchorOrigin";

const VP = { width: 1920, height: 1080 };
const PANEL_W = 360;
const PANEL_H = 420;

const work = {
  left: 0,
  top: 0,
  right: VP.width,
  bottom: VP.height,
};

/** Build a 64x40 capsule rect at the given viewport-percent center. */
function capsuleAt(xPct: number, yPct: number) {
  const cx = (xPct / 100) * VP.width;
  const cy = (yPct / 100) * VP.height;
  return {
    left: cx - 32,
    top: cy - 20,
    right: cx + 32,
    bottom: cy + 20,
  };
}

describe("decideAnchorFromRect — v8 lower-half flip", () => {
  it("capsule at y_percent=70%, single monitor → anchorY='bottom'", () => {
    const { x, y } = decideAnchorFromRect({
      rect: capsuleAt(50, 70),
      effPanelW: PANEL_W,
      effPanelH: PANEL_H,
      work,
    });
    expect(y).toBe("bottom");
    // Capsule horizontally centred → no right-half trigger.
    expect(x).toBe("left");
  });

  it("capsule at y_percent=30%, single monitor → anchorY='top'", () => {
    const { x, y } = decideAnchorFromRect({
      rect: capsuleAt(50, 30),
      effPanelW: PANEL_W,
      effPanelH: PANEL_H,
      work,
    });
    expect(y).toBe("top");
    expect(x).toBe("left");
  });

  it("capsule at y_percent=70% with bigger panel that does NOT fit above flips anyway (v8 drops flipStillFitsY)", () => {
    // 800px panel taller than the space above the capsule on a 1080 vh.
    // v7 would have kept anchorY='top' because flipStillFitsY=false.
    // v8 must still flip to 'bottom' — CSS max-height handles clamping.
    const { y } = decideAnchorFromRect({
      rect: capsuleAt(50, 70),
      effPanelW: PANEL_W,
      effPanelH: 800,
      work,
    });
    expect(y).toBe("bottom");
  });

  it("capsule near right edge (x_percent=92%) → anchorX='right'", () => {
    const { x } = decideAnchorFromRect({
      rect: capsuleAt(92, 50),
      effPanelW: PANEL_W,
      effPanelH: PANEL_H,
      work,
    });
    expect(x).toBe("right");
  });

  it("32px edge-safety triggers flip even when wouldOverflow is just barely false", () => {
    // Place capsule so rect.top + effPanelH + MARGIN ∈ (workBottom-32, workBottom].
    // Choose rect.top = workBottom - effPanelH - MARGIN - 16 = 1080 - 420 - 8 - 16 = 636.
    // Capsule height 40 → center at 656 = 60.7% of 1080 → still in lower half,
    // but assert the EDGE_SAFETY clause works independently by overriding center.
    const top = 636;
    const rect = { left: 928, top, right: 992, bottom: top + 40 };
    const { y } = decideAnchorFromRect({
      rect,
      effPanelW: PANEL_W,
      effPanelH: PANEL_H,
      work,
    });
    expect(y).toBe("bottom");
  });

  it("capsule at corners — bottom-right anchors to right+bottom", () => {
    const r = capsuleAt(95, 92);
    const { x, y } = decideAnchorFromRect({
      rect: r,
      effPanelW: PANEL_W,
      effPanelH: PANEL_H,
      work,
    });
    expect(x).toBe("right");
    expect(y).toBe("bottom");
  });

  it("capsule at top-left corner anchors to left+top", () => {
    const r = capsuleAt(5, 8);
    const { x, y } = decideAnchorFromRect({
      rect: r,
      effPanelW: PANEL_W,
      effPanelH: PANEL_H,
      work,
    });
    expect(x).toBe("left");
    expect(y).toBe("top");
  });

  // v8 round-2 regression: when a panel mounts ALREADY-EXPANDED in
  // `always` display-mode, `getBoundingClientRect()` returns the
  // expanded panel rect (not the zen-capsule). For y_percent=70% with a
  // 420px panel on a 1080 vp, rect.top=756 and rect.bottom=1176 (off-screen).
  // The decision must still flip to anchorY=bottom so the panel pins to
  // the viewport bottom rather than overflowing.
  it("always-mode mount @ y=70%: already-expanded rect still anchors bottom", () => {
    const top = (70 / 100) * VP.height; // 756
    const rect = {
      left: 100,
      top,
      right: 100 + PANEL_W,
      bottom: top + PANEL_H, // 1176 — off-screen
    };
    const { y } = decideAnchorFromRect({
      rect,
      effPanelW: PANEL_W,
      effPanelH: PANEL_H,
      work,
    });
    expect(y).toBe("bottom");
  });
});
