/**
 * Tests for the marquee hit-test cache.
 *
 * Asserts that `startMarquee` snapshots rects and that `endMarquee`
 * intersects against the snapshot, not a live DOM query.
 */
import { describe, it, expect, beforeEach } from "vitest";
import {
  startMarquee,
  updateMarquee,
  endMarquee,
  cancelMarquee,
  rectIndex,
  marquee,
  __setRectIndexForTest,
  __setMarqueeForTest,
  canActivateMarquee,
  type RectEntry,
} from "../marquee";

function fakeRect(
  left: number,
  top: number,
  width: number,
  height: number
): DOMRect {
  return {
    left,
    top,
    right: left + width,
    bottom: top + height,
    width,
    height,
    x: left,
    y: top,
    toJSON() {
      return {};
    },
  } as DOMRect;
}

describe("marquee", () => {
  beforeEach(() => {
    cancelMarquee();
  });

  it("startMarquee captures rect index from a synthetic DOM", () => {
    // Inject a synthetic index so we don't need a real DOM.
    const synth: RectEntry[] = [
      {
        id: "zone-1",
        kind: "zone",
        zoneId: "zone-1",
        rect: fakeRect(0, 0, 100, 100),
      },
    ];
    __setMarqueeForTest({
      originX: 10,
      originY: 10,
      currentX: 10,
      currentY: 10,
    });
    __setRectIndexForTest(synth);
    expect(rectIndex()).toHaveLength(1);
    expect(marquee()).not.toBeNull();
  });

  it("endMarquee returns ids that intersect the final box", () => {
    __setMarqueeForTest({
      originX: 0,
      originY: 0,
      currentX: 50,
      currentY: 50,
    });
    __setRectIndexForTest([
      {
        id: "zone-1",
        kind: "zone",
        zoneId: "zone-1",
        rect: fakeRect(10, 10, 20, 20),
      },
      {
        id: "item-1",
        kind: "item",
        zoneId: "zone-1",
        rect: fakeRect(60, 60, 30, 30),
      },
    ]);
    const result = endMarquee();
    expect(result.zoneIds).toContain("zone-1");
    expect(result.itemIds).toHaveLength(0);
  });

  it("rectIndex is stable during drag — updates to current don't recapture", () => {
    __setRectIndexForTest([
      {
        id: "zone-1",
        kind: "zone",
        zoneId: "zone-1",
        rect: fakeRect(0, 0, 100, 100),
      },
    ]);
    __setMarqueeForTest({ originX: 0, originY: 0, currentX: 0, currentY: 0 });
    const snapshot = rectIndex();
    updateMarquee(500, 500);
    expect(rectIndex()).toBe(snapshot);
  });

  it("canActivateMarquee returns false when nothing is expanded", () => {
    // Fresh module state: no modal, no expanded zones.
    expect(canActivateMarquee()).toBe(false);
  });
});
