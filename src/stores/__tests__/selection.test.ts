/**
 * Tests for the multi-selection store (Theme C).
 *
 * Covers the four gestures from spec C2: click replace, Shift range,
 * Ctrl toggle, marquee union.
 */
import { describe, it, expect, beforeEach } from "vitest";
import {
  selectZone,
  selectItem,
  unionMarqueeSelection,
  replaceWithMarqueeSelection,
  clearMultiSelection,
  isZoneMultiSelected,
  isItemMultiSelected,
  selectedZoneIds,
  selectedItemIds,
} from "../selection";

describe("selection store", () => {
  beforeEach(() => {
    clearMultiSelection();
  });

  it("plain click replaces selection", () => {
    selectZone("z1");
    selectZone("z2");
    expect(selectedZoneIds().size).toBe(1);
    expect(isZoneMultiSelected("z2")).toBe(true);
    expect(isZoneMultiSelected("z1")).toBe(false);
  });

  it("Ctrl+click toggles additively", () => {
    const order = ["z1", "z2", "z3"];
    selectZone("z1", { orderedIds: order });
    selectZone("z2", { ctrl: true, orderedIds: order });
    selectZone("z3", { ctrl: true, orderedIds: order });
    expect(selectedZoneIds().size).toBe(3);
    selectZone("z2", { ctrl: true, orderedIds: order });
    expect(selectedZoneIds().size).toBe(2);
    expect(isZoneMultiSelected("z2")).toBe(false);
  });

  it("Shift+click selects contiguous range using caller-provided order", () => {
    const order = ["a", "b", "c", "d", "e"];
    selectZone("b", { orderedIds: order });
    selectZone("d", { shift: true, orderedIds: order });
    expect([...selectedZoneIds()].sort()).toEqual(["b", "c", "d"]);
  });

  it("item Shift+click operates on item anchor per-zone", () => {
    const orderZ1 = ["i1", "i2", "i3", "i4"];
    selectItem("z1", "i1", { orderedItemIds: orderZ1 });
    selectItem("z1", "i3", { shift: true, orderedItemIds: orderZ1 });
    expect([...selectedItemIds()].sort()).toEqual(["i1", "i2", "i3"]);
  });

  it("unionMarqueeSelection adds to current selection without replacing", () => {
    selectZone("z1");
    unionMarqueeSelection(["z2", "z3"], ["item-a"]);
    expect(isZoneMultiSelected("z1")).toBe(true);
    expect(isZoneMultiSelected("z2")).toBe(true);
    expect(isItemMultiSelected("item-a")).toBe(true);
  });

  it("replaceWithMarqueeSelection discards prior selection", () => {
    selectZone("z1");
    replaceWithMarqueeSelection(["z2"], []);
    expect(isZoneMultiSelected("z1")).toBe(false);
    expect(isZoneMultiSelected("z2")).toBe(true);
  });
});
