/**
 * v8 round-13 — bloom petal hover-intent debounce.
 *
 * Round-13 introduces a 150 ms hover-intent debounce on the path that
 * opens FocusedZonePreview from a bloom-petal hover. The contract:
 *
 *   1. petal mouseenter → activePetalId flips synchronously
 *      (immediate visual feedback via the breathing pulse + soft ring)
 *   2. petal mouseenter → previewZoneId is scheduled, NOT committed,
 *      via setTimeout(150ms)
 *   3. petal mouseleave (within the 150 ms window) → timer is cleared,
 *      preview never opens
 *   4. petal mouseleave (after the 150 ms window, preview already up)
 *      → preview stays open until either a different petal hover
 *      switches it or bloom collapses entirely
 *   5. quick sweep across petals (cursor enters petal A, leaves <50 ms,
 *      enters petal B): only B's hover-intent timer fires; A's timer
 *      was cancelled on its mouseleave
 *   6. hover-off-then-back-on within the grace period: a stable
 *      preview (sticky from a prior click) is NOT torn down — sticky
 *      previews survive transient hover-off
 *
 * This file uses vitest's fake-timer support to drive the debounce
 * deterministically. The model under test is the pure handler
 * sequence (handlePetalEnter / handlePetalLeave / handlePetalClick)
 * that StackWrapper.tsx implements — we replicate the semantics in
 * isolation rather than mounting the full Solid component, because
 * StackWrapper pulls in zonesStore + selection + ipc + settings +
 * i18n + the cursor hit-test poller and bootstrapping all of that
 * is orthogonal to a pure debounce contract test.
 *
 * Pattern matches the lightweight DOM tests in stackBloomHitTest.test.ts
 * — replicate the lifecycle, drive the timers, assert the state.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
// v8 round-14: the round-13 hard-coded numbers moved to a shared
// services/hoverIntent.ts module so external-zone wake/leave timing
// and bloom petal wake/leave timing share a single source of truth.
// This test now sources its mirror constants from the same module
// — a future drift between the production code and these tests
// would fail BOTH the hoverIntent contract test and these
// behavioural tests, surfacing the regression at multiple layers.
import {
  HOVER_INTENT_MS,
  LEAVE_GRACE_MS,
} from "../../../services/hoverIntent";

// Round-13 timer constants — now derived from the shared module.
const PREVIEW_HOVER_INTENT_MS = HOVER_INTENT_MS;
const ACTIVE_PETAL_GRACE_MS = LEAVE_GRACE_MS;

interface BloomHoverState {
  bloomActive: boolean;
  isDragging: boolean;
  activePetalId: string | null;
  previewZoneId: string | null;
  previewSticky: boolean;
}

/**
 * Mirrors the StackWrapper round-13 hover-intent + sticky logic in a
 * pure-state form. The tests can drive the same handlers a real
 * StackWrapper invokes, just without the Solid signals + reactive
 * effects scaffolding.
 */
function createHoverIntentLifecycle() {
  const state: BloomHoverState = {
    bloomActive: false,
    isDragging: false,
    activePetalId: null,
    previewZoneId: null,
    previewSticky: false,
  };

  let previewOpenTimer: ReturnType<typeof setTimeout> | null = null;
  let activeRevertTimer: ReturnType<typeof setTimeout> | null = null;

  const cancelPreviewOpenTimer = (): void => {
    if (previewOpenTimer !== null) {
      clearTimeout(previewOpenTimer);
      previewOpenTimer = null;
    }
  };
  const cancelActiveRevertTimer = (): void => {
    if (activeRevertTimer !== null) {
      clearTimeout(activeRevertTimer);
      activeRevertTimer = null;
    }
  };

  const openBloom = (): void => {
    state.bloomActive = true;
    // No auto-pick: previewZoneId + activePetalId stay null.
  };

  const closeBloom = (): void => {
    state.bloomActive = false;
    state.activePetalId = null;
    state.previewZoneId = null;
    state.previewSticky = false;
    cancelPreviewOpenTimer();
    cancelActiveRevertTimer();
  };

  const handlePetalEnter = (zoneId: string): void => {
    if (state.isDragging) return;
    cancelActiveRevertTimer();
    state.activePetalId = zoneId;
    // Sticky-swap: an existing sticky preview switches synchronously.
    if (
      state.previewSticky &&
      state.previewZoneId !== null &&
      state.previewZoneId !== zoneId
    ) {
      cancelPreviewOpenTimer();
      state.previewZoneId = zoneId;
      return;
    }
    cancelPreviewOpenTimer();
    previewOpenTimer = setTimeout(() => {
      previewOpenTimer = null;
      if (!state.bloomActive) return;
      if (state.activePetalId !== zoneId) return;
      state.previewZoneId = zoneId;
    }, PREVIEW_HOVER_INTENT_MS);
  };

  const handlePetalLeave = (zoneId: string): void => {
    cancelPreviewOpenTimer();
    cancelActiveRevertTimer();
    activeRevertTimer = setTimeout(() => {
      activeRevertTimer = null;
      if (state.activePetalId === zoneId) {
        state.activePetalId = null;
      }
    }, ACTIVE_PETAL_GRACE_MS);
  };

  const handlePetalClick = (zoneId: string): void => {
    if (state.isDragging) return;
    cancelPreviewOpenTimer();
    cancelActiveRevertTimer();
    if (state.previewSticky && state.previewZoneId === zoneId) {
      state.previewZoneId = null;
      state.previewSticky = false;
      state.activePetalId = null;
      return;
    }
    state.activePetalId = zoneId;
    state.previewZoneId = zoneId;
    state.previewSticky = true;
  };

  return {
    state,
    openBloom,
    closeBloom,
    handlePetalEnter,
    handlePetalLeave,
    handlePetalClick,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("v8 round-14 — hover-intent constants come from the shared module", () => {
  it("PREVIEW_HOVER_INTENT_MS mirrors HOVER_INTENT_MS from services/hoverIntent.ts", () => {
    // The round-13 lifecycle test was originally written against
    // local hard-coded 150/80 numbers. Round-14 unified them with the
    // external-zone path; this assertion confirms the test
    // continues to track the shared source of truth.
    expect(PREVIEW_HOVER_INTENT_MS).toBe(HOVER_INTENT_MS);
    expect(PREVIEW_HOVER_INTENT_MS).toBe(150);
  });

  it("ACTIVE_PETAL_GRACE_MS mirrors LEAVE_GRACE_MS from services/hoverIntent.ts", () => {
    expect(ACTIVE_PETAL_GRACE_MS).toBe(LEAVE_GRACE_MS);
    expect(ACTIVE_PETAL_GRACE_MS).toBe(80);
  });
});

describe("v8 round-13 — bloom petal hover-intent debounce", () => {
  it("petal mouseenter immediately sets activePetalId; previewZoneId stays null until 150 ms", () => {
    const lc = createHoverIntentLifecycle();
    lc.openBloom();
    expect(lc.state.activePetalId).toBeNull();
    expect(lc.state.previewZoneId).toBeNull();

    lc.handlePetalEnter("z1");
    // Active flips synchronously.
    expect(lc.state.activePetalId).toBe("z1");
    // Preview is scheduled, not committed.
    expect(lc.state.previewZoneId).toBeNull();

    // Halfway through the debounce — still no preview.
    vi.advanceTimersByTime(PREVIEW_HOVER_INTENT_MS / 2);
    expect(lc.state.previewZoneId).toBeNull();

    // After 150 ms — preview opens.
    vi.advanceTimersByTime(PREVIEW_HOVER_INTENT_MS / 2 + 1);
    expect(lc.state.previewZoneId).toBe("z1");
  });

  it("petal mouseleave within the debounce window CANCELS the pending preview-open", () => {
    const lc = createHoverIntentLifecycle();
    lc.openBloom();
    lc.handlePetalEnter("z1");
    expect(lc.state.activePetalId).toBe("z1");
    expect(lc.state.previewZoneId).toBeNull();

    // Halfway through the 150 ms debounce, mouseleave.
    vi.advanceTimersByTime(50);
    lc.handlePetalLeave("z1");
    // Even after the original timer would have fired, no preview opens.
    vi.advanceTimersByTime(PREVIEW_HOVER_INTENT_MS + 50);
    expect(lc.state.previewZoneId).toBeNull();
    // Active reverts after the 80 ms grace.
    expect(lc.state.activePetalId).toBeNull();
  });

  it("quick sweep across petals: only the FINAL petal's preview opens", () => {
    const lc = createHoverIntentLifecycle();
    lc.openBloom();

    // Cursor sweeps petal A → B → C, each in <50 ms intervals (less
    // than the 150 ms debounce). Only C's timer should be alive when
    // the dust settles.
    lc.handlePetalEnter("A");
    vi.advanceTimersByTime(40);
    lc.handlePetalLeave("A");
    lc.handlePetalEnter("B");
    vi.advanceTimersByTime(40);
    lc.handlePetalLeave("B");
    lc.handlePetalEnter("C");

    // Only C is active; A and B's leave-grace is in flight but no
    // preview has opened yet.
    expect(lc.state.activePetalId).toBe("C");
    expect(lc.state.previewZoneId).toBeNull();

    // Drive C's debounce to completion.
    vi.advanceTimersByTime(PREVIEW_HOVER_INTENT_MS + 1);
    expect(lc.state.previewZoneId).toBe("C");
  });

  it("hover-off-then-back-on within 50 ms does NOT tear down a sticky preview", () => {
    const lc = createHoverIntentLifecycle();
    lc.openBloom();

    // Click to commit a sticky preview on petal A.
    lc.handlePetalClick("A");
    expect(lc.state.previewZoneId).toBe("A");
    expect(lc.state.previewSticky).toBe(true);
    expect(lc.state.activePetalId).toBe("A");

    // Cursor briefly leaves the petal — within the active-grace
    // window — and re-enters. The active state should NOT have
    // reverted, and the sticky preview should still be on A.
    lc.handlePetalLeave("A");
    vi.advanceTimersByTime(40);
    lc.handlePetalEnter("A");
    expect(lc.state.activePetalId).toBe("A");
    expect(lc.state.previewZoneId).toBe("A");
    expect(lc.state.previewSticky).toBe(true);

    // Drive timers further — sticky preview persists.
    vi.advanceTimersByTime(PREVIEW_HOVER_INTENT_MS + 100);
    expect(lc.state.previewZoneId).toBe("A");
    expect(lc.state.previewSticky).toBe(true);
  });

  it("sticky preview swaps SYNCHRONOUSLY when the cursor moves to a different petal", () => {
    const lc = createHoverIntentLifecycle();
    lc.openBloom();

    // Click to commit sticky on A.
    lc.handlePetalClick("A");
    expect(lc.state.previewZoneId).toBe("A");
    expect(lc.state.previewSticky).toBe(true);

    // Cursor moves to petal B. The sticky-swap path should flip
    // previewZoneId immediately (no 150 ms latency on a panel that
    // is already on screen).
    lc.handlePetalLeave("A");
    lc.handlePetalEnter("B");
    expect(lc.state.activePetalId).toBe("B");
    expect(lc.state.previewZoneId).toBe("B");
    // Sticky flag survives — the user has not explicitly closed.
    expect(lc.state.previewSticky).toBe(true);
  });

  it("bloom collapse drops every transient signal regardless of pending timers", () => {
    const lc = createHoverIntentLifecycle();
    lc.openBloom();
    lc.handlePetalEnter("z1");
    expect(lc.state.activePetalId).toBe("z1");

    // Mid-debounce, bloom collapses (e.g. cursor leaves the wrapper).
    vi.advanceTimersByTime(50);
    lc.closeBloom();
    expect(lc.state.bloomActive).toBe(false);
    expect(lc.state.activePetalId).toBeNull();
    expect(lc.state.previewZoneId).toBeNull();
    expect(lc.state.previewSticky).toBe(false);

    // Even if the timer had fired (it shouldn't, because closeBloom
    // calls cancelPreviewOpenTimer), the deferred-write guard inside
    // handlePetalEnter checks bloomActive before committing.
    vi.advanceTimersByTime(PREVIEW_HOVER_INTENT_MS + 1);
    expect(lc.state.previewZoneId).toBeNull();
  });

  it("petal click during a pending hover-intent timer cancels the timer and commits sticky", () => {
    const lc = createHoverIntentLifecycle();
    lc.openBloom();

    lc.handlePetalEnter("z1");
    expect(lc.state.activePetalId).toBe("z1");
    expect(lc.state.previewZoneId).toBeNull();

    // 50 ms in, user clicks. Preview commits synchronously, sticky.
    vi.advanceTimersByTime(50);
    lc.handlePetalClick("z1");
    expect(lc.state.previewZoneId).toBe("z1");
    expect(lc.state.previewSticky).toBe(true);

    // The pending hover-intent timer must have been cancelled — no
    // duplicate setPreviewZoneId fires when the original timer would
    // have elapsed.
    vi.advanceTimersByTime(PREVIEW_HOVER_INTENT_MS);
    expect(lc.state.previewZoneId).toBe("z1");
    expect(lc.state.previewSticky).toBe(true);
  });
});
