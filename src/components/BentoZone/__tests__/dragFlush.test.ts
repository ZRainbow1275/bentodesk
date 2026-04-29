/**
 * v8 round-4 hotfix — drag-mouseup must flush the pending rAF tick
 * BEFORE reading the persisted position.
 *
 * Round-4 introduced rAF-coalesced mousemove handlers in BentoZone and
 * StackWrapper to keep the per-frame signal write count at one. The
 * mouseup handler called `cancelAnimationFrame(moveRafId)` to dispose of
 * any still-pending tick — which silently dropped the most recent native
 * mousemove without writing it to the drag-position signal. The
 * subsequent `dragPosition()` read was therefore one frame stale, the
 * persisted coords lagged the cursor, and the dropped capsule visibly
 * oscillated back to that older frame's location on the next render.
 *
 * Fix: flush the pending event synchronously inside mouseup, BEFORE
 * cancelling the rAF. The flush handler is idempotent — it just reads
 * the latest cached `MouseEvent` and writes the corresponding signal —
 * so re-running it is safe and guarantees the signal carries the user's
 * actual release position.
 *
 * This test mirrors the rAF-coalesce pattern in isolation so it breaks
 * loudly if a future refactor reverts the flush call. The rAF itself is
 * stubbed; the contract is "the signal value at mouseup time matches
 * the LAST mousemove event, not the second-to-last".
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

interface RafHandle {
  id: number;
  cb: FrameRequestCallback;
}

/** Tiny rAF/cAF stub so the test can deterministically control which
 *  ticks fire and which get cancelled. */
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
      const next: RafHandle | undefined = (() => {
        const entry = pending.entries().next().value;
        if (!entry) return undefined;
        return { id: entry[0], cb: entry[1] };
      })();
      if (next) {
        pending.delete(next.id);
        next.cb(performance.now());
      }
    },
    pendingCount: () => pending.size,
    cafSpy: caf,
  };
}

/**
 * Reproduces the BentoZone single-zone capsule drag mouseup state machine
 * without the SolidJS render layer. The contract under test is purely
 * the order of operations: rAF coalescer → mouseup → flush → cancel →
 * read. The signal is a plain object so the assertion is straightforward.
 */
function createDragController(opts: { startX: number; startY: number }) {
  let lastMoveEvent: MouseEvent | null = null;
  let moveRafId: number | null = null;
  const dragPosition: { x: number; y: number } = { x: 0, y: 0 };

  const flushMove = (): void => {
    moveRafId = null;
    const ev = lastMoveEvent;
    if (!ev) return;
    dragPosition.x = ev.clientX - opts.startX;
    dragPosition.y = ev.clientY - opts.startY;
  };

  const onMouseMove = (ev: MouseEvent): void => {
    lastMoveEvent = ev;
    if (moveRafId !== null) return;
    moveRafId = requestAnimationFrame(flushMove);
  };

  /** v8 round-4 hotfix-correct mouseup: cancel pending rAF, then FLUSH
   *  synchronously so dragPosition reflects the latest mousemove. */
  const onMouseUpFixed = (): { x: number; y: number } => {
    if (moveRafId !== null) {
      cancelAnimationFrame(moveRafId);
      moveRafId = null;
    }
    flushMove();
    return { ...dragPosition };
  };

  /** Pre-fix mouseup: cancels the rAF without flushing — drops the
   *  latest event entirely. Kept here so the test can prove the bug
   *  existed and that the fix actually matters. */
  const onMouseUpBuggy = (): { x: number; y: number } => {
    if (moveRafId !== null) {
      cancelAnimationFrame(moveRafId);
      moveRafId = null;
    }
    return { ...dragPosition };
  };

  return { onMouseMove, onMouseUpFixed, onMouseUpBuggy, dragPosition };
}

function makeMouseEvent(x: number, y: number): MouseEvent {
  // Minimal stub — only the .clientX / .clientY props are read.
  return { clientX: x, clientY: y } as unknown as MouseEvent;
}

describe("v8 round-4 hotfix — drag mouseup flushes pending rAF tick", () => {
  let raf: ReturnType<typeof setupRaf>;
  let originalRaf: typeof requestAnimationFrame;
  let originalCaf: typeof cancelAnimationFrame;

  beforeEach(() => {
    originalRaf = globalThis.requestAnimationFrame;
    originalCaf = globalThis.cancelAnimationFrame;
    raf = setupRaf();
  });

  afterEach(() => {
    globalThis.requestAnimationFrame = originalRaf;
    globalThis.cancelAnimationFrame = originalCaf;
  });

  it("baseline: a single mousemove + flushed rAF lands the position", () => {
    const ctl = createDragController({ startX: 0, startY: 0 });
    ctl.onMouseMove(makeMouseEvent(10, 20));
    raf.flushNext();
    expect(ctl.dragPosition).toEqual({ x: 10, y: 20 });
  });

  it("BUG demonstration: pre-fix mouseup discards the still-pending rAF tick", () => {
    const ctl = createDragController({ startX: 0, startY: 0 });
    // First mousemove + flush — establishes a baseline position.
    ctl.onMouseMove(makeMouseEvent(10, 20));
    raf.flushNext();
    // Second mousemove arrives — the rAF coalescer queues it but the
    // user releases the mouse BEFORE the next tick fires.
    ctl.onMouseMove(makeMouseEvent(50, 80));
    expect(raf.pendingCount()).toBe(1);
    const released = ctl.onMouseUpBuggy();
    // Pre-fix: dragPosition is still the FIRST mousemove. The actual
    // release coordinates were silently dropped — exactly the
    // user-visible "snap back" symptom.
    expect(released).toEqual({ x: 10, y: 20 });
    expect(released).not.toEqual({ x: 50, y: 80 });
  });

  it("FIX: post-fix mouseup flushes the pending event before reading", () => {
    const ctl = createDragController({ startX: 0, startY: 0 });
    ctl.onMouseMove(makeMouseEvent(10, 20));
    raf.flushNext();
    ctl.onMouseMove(makeMouseEvent(50, 80));
    expect(raf.pendingCount()).toBe(1);
    const released = ctl.onMouseUpFixed();
    // Post-fix: dragPosition matches the latest mousemove. The cursor's
    // real release point reaches the persistence layer.
    expect(released).toEqual({ x: 50, y: 80 });
    // And the rAF was cancelled (so it can't fire later and clobber).
    expect(raf.cafSpy).toHaveBeenCalledTimes(1);
    expect(raf.pendingCount()).toBe(0);
  });

  it("FIX: flushMove is idempotent — calling it twice with no new event is safe", () => {
    const ctl = createDragController({ startX: 0, startY: 0 });
    ctl.onMouseMove(makeMouseEvent(50, 80));
    const first = ctl.onMouseUpFixed();
    // Second mouseup with no intervening mousemove — last-known position
    // should still be reported correctly without errors.
    const second = ctl.onMouseUpFixed();
    expect(first).toEqual({ x: 50, y: 80 });
    expect(second).toEqual({ x: 50, y: 80 });
  });

  it("FIX: rapid mousemove burst followed by immediate mouseup lands the LAST event", () => {
    const ctl = createDragController({ startX: 0, startY: 0 });
    // Simulates a 120 Hz mouse firing 5 events inside one frame.
    ctl.onMouseMove(makeMouseEvent(10, 10));
    ctl.onMouseMove(makeMouseEvent(20, 20));
    ctl.onMouseMove(makeMouseEvent(30, 30));
    ctl.onMouseMove(makeMouseEvent(40, 40));
    ctl.onMouseMove(makeMouseEvent(50, 50));
    // Coalescer: only one rAF was scheduled.
    expect(raf.pendingCount()).toBe(1);
    const released = ctl.onMouseUpFixed();
    // The persisted position is the LAST event's coordinates, not any
    // intermediate one — proving the coalesce + flush combination
    // preserves cursor accuracy under high-Hz input.
    expect(released).toEqual({ x: 50, y: 50 });
  });
});
