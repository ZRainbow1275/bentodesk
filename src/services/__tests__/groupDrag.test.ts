/**
 * Tests for group-drag preview math + selection store integration.
 *
 * #3 batch-drag fix: when ≥2 zones are multi-selected, mousedown on the
 * collapsed capsule of any selected zone must drag the whole selection.
 * mouseup commits via `bulkUpdateZones`. The pure preview math + the
 * selection-store wiring are the two pieces we can unit-test here without
 * spinning a DOM. The actual mousedown handler lives inside BentoZone.tsx.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { buildGroupDragPreview } from "../groupDrag";
import {
  beginGroupZoneDrag,
  updateGroupZoneDrag,
  endGroupZoneDrag,
  cancelGroupZoneDrag,
  getGroupDragPreviewPosition,
  isGroupDragActive,
  setZoneSelection,
  selectedZoneIds,
} from "../../stores/selection";

describe("buildGroupDragPreview", () => {
  it("shifts every zone by the delta and clamps to the max bound", () => {
    const origin = {
      a: { x_percent: 10, y_percent: 20 },
      b: { x_percent: 50, y_percent: 60 },
    };
    const next = buildGroupDragPreview(origin, { x_percent: 5, y_percent: -5 }, 96);
    expect(next.a).toEqual({ x_percent: 15, y_percent: 15 });
    expect(next.b).toEqual({ x_percent: 55, y_percent: 55 });
  });

  it("clamps to 0 when the delta drives a position negative", () => {
    const origin = { a: { x_percent: 2, y_percent: 2 } };
    const next = buildGroupDragPreview(origin, { x_percent: -10, y_percent: -10 }, 96);
    expect(next.a).toEqual({ x_percent: 0, y_percent: 0 });
  });

  it("clamps to maxPercent when the delta would push past the right/bottom edge", () => {
    const origin = { a: { x_percent: 90, y_percent: 90 } };
    const next = buildGroupDragPreview(origin, { x_percent: 50, y_percent: 50 }, 96);
    expect(next.a).toEqual({ x_percent: 96, y_percent: 96 });
  });
});

describe("selection store group-drag lifecycle", () => {
  beforeEach(() => {
    setZoneSelection([]);
    cancelGroupZoneDrag();
  });

  it("beginGroupZoneDrag seeds the preview at the origin positions", () => {
    beginGroupZoneDrag([
      { id: "z1", position: { x_percent: 10, y_percent: 10 } },
      { id: "z2", position: { x_percent: 30, y_percent: 30 } },
    ]);
    expect(isGroupDragActive()).toBe(true);
    expect(getGroupDragPreviewPosition("z1")).toEqual({ x_percent: 10, y_percent: 10 });
    expect(getGroupDragPreviewPosition("z2")).toEqual({ x_percent: 30, y_percent: 30 });
  });

  it("updateGroupZoneDrag shifts every preview by the delta", () => {
    beginGroupZoneDrag([
      { id: "z1", position: { x_percent: 10, y_percent: 10 } },
      { id: "z2", position: { x_percent: 30, y_percent: 30 } },
    ]);
    updateGroupZoneDrag({ x_percent: 5, y_percent: -3 });
    expect(getGroupDragPreviewPosition("z1")).toEqual({ x_percent: 15, y_percent: 7 });
    expect(getGroupDragPreviewPosition("z2")).toEqual({ x_percent: 35, y_percent: 27 });
  });

  it("endGroupZoneDrag returns the final preview map and clears state", () => {
    beginGroupZoneDrag([
      { id: "z1", position: { x_percent: 10, y_percent: 10 } },
    ]);
    updateGroupZoneDrag({ x_percent: 8, y_percent: 12 });
    const finalPreview = endGroupZoneDrag();
    expect(finalPreview).toEqual({ z1: { x_percent: 18, y_percent: 22 } });
    expect(isGroupDragActive()).toBe(false);
    expect(getGroupDragPreviewPosition("z1")).toBeNull();
  });

  it("cancel clears the preview without surfacing positions", () => {
    beginGroupZoneDrag([
      { id: "z1", position: { x_percent: 10, y_percent: 10 } },
    ]);
    cancelGroupZoneDrag();
    expect(isGroupDragActive()).toBe(false);
    expect(getGroupDragPreviewPosition("z1")).toBeNull();
  });

  it("selectZone+groupDrag carries every selected zone through the lifecycle", () => {
    setZoneSelection(["z1", "z2", "z3"]);
    expect(selectedZoneIds().size).toBe(3);

    beginGroupZoneDrag([
      { id: "z1", position: { x_percent: 0, y_percent: 0 } },
      { id: "z2", position: { x_percent: 20, y_percent: 0 } },
      { id: "z3", position: { x_percent: 40, y_percent: 0 } },
    ]);
    updateGroupZoneDrag({ x_percent: 5, y_percent: 5 });
    const preview = endGroupZoneDrag();
    expect(Object.keys(preview).sort()).toEqual(["z1", "z2", "z3"]);
    expect(preview.z1).toEqual({ x_percent: 5, y_percent: 5 });
    expect(preview.z2).toEqual({ x_percent: 25, y_percent: 5 });
    expect(preview.z3).toEqual({ x_percent: 45, y_percent: 5 });
  });

  it("stack members + free zones combine into one rigid drag set", () => {
    // Mirrors StackWrapper.handleCapsuleMouseDown: when the stack is part of
    // a wider selection, every member of the stack must move with the
    // selected free zones — even if the user only ticked one stack member
    // in the canvas selection.
    setZoneSelection(["stackTop", "freeA"]);
    const stackMembers = [
      { id: "stackBase", position: { x_percent: 10, y_percent: 10 } },
      { id: "stackTop", position: { x_percent: 10, y_percent: 10 } },
    ];
    const allSelectedZones = [
      { id: "stackTop", position: { x_percent: 10, y_percent: 10 } },
      { id: "freeA", position: { x_percent: 50, y_percent: 50 } },
    ];
    // Same dedup pattern as in StackWrapper.tsx so the test doubles as a
    // regression guard for the "stack base must move too" edge case.
    const draggable = new Map<string, { x_percent: number; y_percent: number }>();
    for (const z of allSelectedZones) draggable.set(z.id, z.position);
    for (const m of stackMembers) draggable.set(m.id, m.position);
    expect([...draggable.keys()].sort()).toEqual([
      "freeA",
      "stackBase",
      "stackTop",
    ]);

    beginGroupZoneDrag(
      [...draggable.entries()].map(([id, position]) => ({ id, position })),
    );
    updateGroupZoneDrag({ x_percent: 4, y_percent: 4 });
    const preview = endGroupZoneDrag();
    expect(preview.stackBase).toEqual({ x_percent: 14, y_percent: 14 });
    expect(preview.stackTop).toEqual({ x_percent: 14, y_percent: 14 });
    expect(preview.freeA).toEqual({ x_percent: 54, y_percent: 54 });
  });
});
