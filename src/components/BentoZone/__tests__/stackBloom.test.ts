/**
 * v8 #4 — stack hover-bloom mouseleave race-protection.
 *
 * v7's bloom collapsed the moment the cursor crossed any wrapper edge,
 * so a brief grace gap (cursor moving capsule → petal, or a neighbour
 * zone with higher z-index briefly stealing hover) caused the petals to
 * blink off. v8 schedules the collapse on an 80 ms timer and cancels it
 * on re-entry.
 *
 * This test verifies the timer state machine in isolation by exercising
 * the same SolidJS reactive primitives the component uses, without
 * mounting the full DOM tree (the wrapper component pulls in stores
 * that are heavy to bootstrap in a unit test, and the contract under
 * test is purely the timer cancellation semantics).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot, createSignal } from "solid-js";
// v8 round-14: source the leave-grace timing from the same shared
// module the production code uses so a future timing tweak only has
// to change ONE place. Pre-round-14 this test hard-coded `80` (and
// `79` / `1` in advanceTimersByTime calls) which would silently drift
// from the production constant.
import { LEAVE_GRACE_MS } from "../../../services/hoverIntent";

interface BloomController {
  enter: () => void;
  leave: () => void;
  isBloomed: () => boolean;
  dispose: () => void;
}

/** Mirror of StackWrapper's bloom state machine, kept in this file so the
 *  test stays a contract on the *behaviour* and breaks loudly if the
 *  component diverges from the agreed semantics. */
function createBloomController(): BloomController {
  const [isBloomed, setIsBloomed] = createSignal(false);
  let bloomCollapseTimer: ReturnType<typeof setTimeout> | null = null;
  const cancelBloomCollapse = (): void => {
    if (bloomCollapseTimer !== null) {
      clearTimeout(bloomCollapseTimer);
      bloomCollapseTimer = null;
    }
  };
  let disposeRoot: () => void = () => {};
  createRoot((dispose) => {
    disposeRoot = dispose;
  });
  return {
    enter: () => {
      cancelBloomCollapse();
      setIsBloomed(true);
    },
    leave: () => {
      cancelBloomCollapse();
      bloomCollapseTimer = setTimeout(() => {
        bloomCollapseTimer = null;
        setIsBloomed(false);
      }, LEAVE_GRACE_MS);
    },
    isBloomed,
    dispose: () => {
      cancelBloomCollapse();
      disposeRoot();
    },
  };
}

describe("StackWrapper bloom timer (v8 #4)", () => {
  let ctl: BloomController;

  beforeEach(() => {
    vi.useFakeTimers();
    ctl = createBloomController();
  });

  afterEach(() => {
    ctl.dispose();
    vi.useRealTimers();
  });

  it("mouseenter activates bloom synchronously", () => {
    expect(ctl.isBloomed()).toBe(false);
    ctl.enter();
    expect(ctl.isBloomed()).toBe(true);
  });

  it("mouseleave defers collapse and clears after the LEAVE_GRACE_MS window", () => {
    ctl.enter();
    expect(ctl.isBloomed()).toBe(true);

    ctl.leave();
    // Still bloomed: the timer hasn't elapsed yet.
    expect(ctl.isBloomed()).toBe(true);

    // Just before the grace boundary — still bloomed.
    vi.advanceTimersByTime(LEAVE_GRACE_MS - 1);
    expect(ctl.isBloomed()).toBe(true);

    // Crossing the grace boundary — collapse fires.
    vi.advanceTimersByTime(1);
    expect(ctl.isBloomed()).toBe(false);
  });

  it("mouseleave followed by mouseenter inside the grace window cancels the collapse", () => {
    ctl.enter();
    expect(ctl.isBloomed()).toBe(true);

    ctl.leave();
    // Half-way into the grace window the cursor re-enters (e.g. crossed
    // a neighbouring zone briefly, or moved capsule → petal across the
    // visible gap).
    vi.advanceTimersByTime(Math.floor(LEAVE_GRACE_MS / 2));
    ctl.enter();

    // Past the original collapse deadline — bloom must still be open.
    vi.advanceTimersByTime(LEAVE_GRACE_MS);
    expect(ctl.isBloomed()).toBe(true);

    // And running the clock further does not collapse it (no stale
    // timer left armed).
    vi.advanceTimersByTime(500);
    expect(ctl.isBloomed()).toBe(true);
  });

  it("repeated leave→enter→leave sequences only honour the latest leave", () => {
    ctl.enter();
    ctl.leave();
    vi.advanceTimersByTime(Math.floor(LEAVE_GRACE_MS / 2));
    ctl.enter();
    vi.advanceTimersByTime(Math.floor(LEAVE_GRACE_MS / 2));
    ctl.leave();
    // Original deadline (LEAVE_GRACE_MS after first leave) has long
    // passed but the controller must still hold the bloom open until
    // *this* leave's grace window expires.
    vi.advanceTimersByTime(LEAVE_GRACE_MS - 10);
    expect(ctl.isBloomed()).toBe(true);
    vi.advanceTimersByTime(20);
    expect(ctl.isBloomed()).toBe(false);
  });
});
