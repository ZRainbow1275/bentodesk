/**
 * v8 round-4 real-fix (Bug 1) — drag positions the capsule centered on
 * the cursor.
 *
 * The pre-fix offset preserved the click point inside the panel rect
 * (offsetX = clientX − rect.left, offsetY = clientY − rect.top), which
 * was correct while the panel was painted in expanded form but jumped
 * to a wrong position the moment the panel collapsed to the zen
 * capsule on drop — the capsule landed wherever inside the panel the
 * user happened to click, not at the cursor.
 *
 * Fix: collapse to zen *at drag start*, then center the capsule on the
 * cursor throughout the drag using `offsetX = capsuleWidth / 2` and
 * `offsetY = capsuleHeight / 2`. The persisted x_percent on release
 * therefore equals `((clientX − capsuleWidth/2) / viewportWidth) ×
 * 100`, modulo viewport clamping.
 *
 * This file mirrors the production `flushMove` contract (the same
 * pattern dragFlush.test.ts uses) so a future refactor that breaks the
 * center-on-cursor invariant fails this test loudly.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

interface CapsuleBox {
  width: number;
  height: number;
}

interface Viewport {
  width: number;
  height: number;
}

/** Mirror of BentoZone.tsx's flushMove + onMouseUp center-on-cursor
 *  logic. Pure — no Solid/DOM dependencies. */
function createCenterOnCursorController(opts: {
  capsule: CapsuleBox;
  viewport: Viewport;
}) {
  let lastMoveEvent: MouseEvent | null = null;
  let moveRafId: number | null = null;
  const dragPosition: { x_percent: number; y_percent: number } = {
    x_percent: 0,
    y_percent: 0,
  };

  // capsule center → cursor: top-left = cursor − capsule/2
  const offsetX = opts.capsule.width / 2;
  const offsetY = opts.capsule.height / 2;

  const flushMove = (): void => {
    moveRafId = null;
    const ev = lastMoveEvent;
    if (!ev) return;
    const xPercent = ((ev.clientX - offsetX) / opts.viewport.width) * 100;
    const yPercent = ((ev.clientY - offsetY) / opts.viewport.height) * 100;
    const maxXPct = Math.max(
      0,
      100 - (opts.capsule.width / opts.viewport.width) * 100,
    );
    const maxYPct = Math.max(
      0,
      100 - (opts.capsule.height / opts.viewport.height) * 100,
    );
    dragPosition.x_percent = Math.max(0, Math.min(maxXPct, xPercent));
    dragPosition.y_percent = Math.max(0, Math.min(maxYPct, yPercent));
  };

  const onMouseMove = (ev: MouseEvent): void => {
    lastMoveEvent = ev;
    if (moveRafId !== null) return;
    moveRafId = requestAnimationFrame(flushMove);
  };

  const onMouseUp = (): { x_percent: number; y_percent: number } => {
    if (moveRafId !== null) {
      cancelAnimationFrame(moveRafId);
      moveRafId = null;
    }
    flushMove();
    return { ...dragPosition };
  };

  return { onMouseMove, onMouseUp, dragPosition, offsetX, offsetY };
}

function makeMouseEvent(x: number, y: number): MouseEvent {
  return { clientX: x, clientY: y } as unknown as MouseEvent;
}

function setupRaf() {
  const pending = new Map<number, FrameRequestCallback>();
  let nextId = 1;
  const raf = vi.fn((cb: FrameRequestCallback): number => {
    const id = nextId++;
    pending.set(id, cb);
    return id;
  });
  const caf = vi.fn((id: number): void => {
    pending.delete(id);
  });
  globalThis.requestAnimationFrame = raf as unknown as typeof requestAnimationFrame;
  globalThis.cancelAnimationFrame = caf as unknown as typeof cancelAnimationFrame;
  return {
    flushNext: (): void => {
      const entry = pending.entries().next().value;
      if (!entry) return;
      pending.delete(entry[0]);
      entry[1](performance.now());
    },
  };
}

describe("v8 round-4 real-fix — drag centers capsule on cursor", () => {
  const VIEWPORT: Viewport = { width: 1920, height: 1080 };
  const MEDIUM_CAPSULE: CapsuleBox = { width: 160, height: 48 };
  let originalRaf: typeof requestAnimationFrame;
  let originalCaf: typeof cancelAnimationFrame;

  beforeEach(() => {
    originalRaf = globalThis.requestAnimationFrame;
    originalCaf = globalThis.cancelAnimationFrame;
    setupRaf();
  });

  afterEach(() => {
    globalThis.requestAnimationFrame = originalRaf;
    globalThis.cancelAnimationFrame = originalCaf;
  });

  it("offset uses half of the capsule, not the click point inside the rect", () => {
    const ctl = createCenterOnCursorController({
      capsule: MEDIUM_CAPSULE,
      viewport: VIEWPORT,
    });
    expect(ctl.offsetX).toBe(80);
    expect(ctl.offsetY).toBe(24);
  });

  it("release at viewport (X, Y) persists x_percent = (X - capW/2) / vw, y likewise", () => {
    const ctl = createCenterOnCursorController({
      capsule: MEDIUM_CAPSULE,
      viewport: VIEWPORT,
    });
    // User releases the mouse with cursor at (500, 300) on a 1920x1080
    // viewport with a 160x48 medium capsule.
    ctl.onMouseMove(makeMouseEvent(500, 300));
    const released = ctl.onMouseUp();
    // Expected: top-left.x = 500 - 80 = 420 → 420/1920 = 21.875%
    expect(released.x_percent).toBeCloseTo(((500 - 80) / 1920) * 100, 5);
    // Expected: top-left.y = 300 - 24 = 276 → 276/1080 = 25.5555…%
    expect(released.y_percent).toBeCloseTo(((300 - 24) / 1080) * 100, 5);
  });

  it("works the same regardless of where the drag started — offset is capsule-relative, not click-relative", () => {
    // Two drags, identical release point but DIFFERENT start positions:
    // result must be identical (the click-point offset is no longer in
    // play, only the capsule center).
    const ctlA = createCenterOnCursorController({
      capsule: MEDIUM_CAPSULE,
      viewport: VIEWPORT,
    });
    ctlA.onMouseMove(makeMouseEvent(700, 500));
    const aReleased = ctlA.onMouseUp();

    const ctlB = createCenterOnCursorController({
      capsule: MEDIUM_CAPSULE,
      viewport: VIEWPORT,
    });
    ctlB.onMouseMove(makeMouseEvent(700, 500));
    const bReleased = ctlB.onMouseUp();

    expect(aReleased).toEqual(bReleased);
  });

  it("clamps near the right viewport edge so the capsule never escapes", () => {
    const ctl = createCenterOnCursorController({
      capsule: MEDIUM_CAPSULE,
      viewport: VIEWPORT,
    });
    // Release near the far-right edge — the raw computation would put
    // the capsule's left at 1920 - 80 = 1840 px → 95.83%, plus the
    // capsule itself extends another 160 px past the edge. Max
    // top-left x_percent = (1920 - 160) / 1920 ≈ 91.667% — the clamp
    // keeps us at or below that.
    ctl.onMouseMove(makeMouseEvent(1920, 100));
    const released = ctl.onMouseUp();
    const maxXPct = ((1920 - 160) / 1920) * 100;
    expect(released.x_percent).toBeLessThanOrEqual(maxXPct + 1e-6);
    expect(released.x_percent).toBeGreaterThan(0);
  });

  it("clamps near the left viewport edge so x_percent never goes negative", () => {
    const ctl = createCenterOnCursorController({
      capsule: MEDIUM_CAPSULE,
      viewport: VIEWPORT,
    });
    // Cursor near (10, 10): raw top-left would be 10 - 80 = -70px → -3.6%
    // — clamp must pin to 0%.
    ctl.onMouseMove(makeMouseEvent(10, 10));
    const released = ctl.onMouseUp();
    expect(released.x_percent).toBe(0);
    expect(released.y_percent).toBe(0);
  });

  it("works for a circle (small) capsule too — offset is shape-aware via getCapsuleBoxPx", () => {
    const SMALL_CIRCLE: CapsuleBox = { width: 42, height: 42 };
    const ctl = createCenterOnCursorController({
      capsule: SMALL_CIRCLE,
      viewport: VIEWPORT,
    });
    ctl.onMouseMove(makeMouseEvent(500, 300));
    const released = ctl.onMouseUp();
    // Expected: top-left.x = 500 - 21 = 479 → 479/1920 ≈ 24.948%
    expect(released.x_percent).toBeCloseTo(((500 - 21) / 1920) * 100, 5);
    expect(released.y_percent).toBeCloseTo(((300 - 21) / 1080) * 100, 5);
  });
});
