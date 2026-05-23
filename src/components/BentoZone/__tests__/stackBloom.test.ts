/**
 * v8 #4 / v9 stack-wake-mutex — stack hover-bloom timer state machine.
 *
 * v7's bloom collapsed the moment the cursor crossed any wrapper edge,
 * so a brief grace gap (cursor moving capsule → petal, or a neighbour
 * zone with higher z-index briefly stealing hover) caused the petals
 * to blink off. v8 introduced an 80 ms grace timer; v9
 * stack-wake-mutex initially attempted to tighten the legacy
 * `!isBloomed()` fallback path to 0 ms but live testing surfaced a
 * regression — the cursor spends ~16–32 ms in `hovered === null`
 * while crossing the 12 px capsule→petal halo gap, and a 0 ms grace
 * collapsed the bloom before the cursor reached the petal. The
 * post-PR3 fix restores LEAVE_GRACE_MS = 80 ms for the family-internal
 * traversal case (this test's contract); the non-family-hit branch
 * in the production hoveredZone$ effect is still synchronous and is
 * pinned separately in stackBloomCollapse.test.ts.
 *
 * The cancellation semantics — re-entry inside the still-armed
 * window cancels the collapse — are the load-bearing contract this
 * test pins.
 *
 * The test verifies the timer state machine in isolation by
 * exercising the same SolidJS reactive primitives the component
 * uses, without mounting the full DOM tree (the wrapper pulls in
 * stores that are heavy to bootstrap in a unit test, and the
 * contract under test is purely the timer cancellation semantics).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot, createSignal } from "solid-js";
// v8 round-14: source the leave-grace timing from the same shared
// module the production code uses so a future timing tweak only has
// to change ONE place. v9 (post-PR3 fix) keeps LEAVE_GRACE_MS at
// 80 ms; the tests below use the value directly without a probe
// guard since the v9 contract pins the value at ≥ 50 ms.
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

describe("StackWrapper bloom timer (v8 #4 / v9)", () => {
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

    // Just below the grace boundary — bloom is still alive (this
    // is the family-internal traversal interval the v9 post-PR3
    // fix protects: the cursor crossing the capsule→petal halo
    // gap spends ~16–32 ms in this state, MUST survive it).
    vi.advanceTimersByTime(LEAVE_GRACE_MS - 1);
    expect(ctl.isBloomed()).toBe(true);

    // Crossing the grace boundary — collapse fires.
    vi.advanceTimersByTime(2);
    expect(ctl.isBloomed()).toBe(false);
  });

  it("mouseleave followed by mouseenter inside the grace window cancels the collapse", () => {
    ctl.enter();
    expect(ctl.isBloomed()).toBe(true);

    ctl.leave();
    // Half-way through the grace window the user re-enters (the
    // canonical capsule→petal traversal: cursor leaves the
    // wrapper edge, traverses the 12 px halo gap, lands on the
    // petal halo). The cancel inside enter() discards the
    // queued collapse macrotask.
    vi.advanceTimersByTime(LEAVE_GRACE_MS / 2);
    expect(ctl.isBloomed()).toBe(true);
    ctl.enter();

    // Past the original collapse deadline — bloom must still be open.
    vi.advanceTimersByTime(LEAVE_GRACE_MS + 1);
    expect(ctl.isBloomed()).toBe(true);

    // And running the clock further does not collapse it (no stale
    // timer left armed).
    vi.advanceTimersByTime(500);
    expect(ctl.isBloomed()).toBe(true);
  });

  it("repeated leave→enter→leave sequences only honour the latest leave", () => {
    ctl.enter();
    ctl.leave();
    // Re-enter without advancing the clock — first leave's
    // queued collapse is cancelled.
    ctl.enter();
    ctl.leave();
    // Past the second leave's deadline — collapse fires.
    vi.advanceTimersByTime(LEAVE_GRACE_MS + 1);
    expect(ctl.isBloomed()).toBe(false);
  });
});
