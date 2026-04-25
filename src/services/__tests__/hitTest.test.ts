/**
 * Tests for the hit-test state machine.
 *
 * Mocks @tauri-apps/api/window to avoid native Tauri dependencies.
 * Focuses on: state transitions, drag/modal locks, grace period, zone registration.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ─── Mock Tauri window API ──────────────────────────────────

const mockSetIgnoreCursorEvents = vi.fn<(ignore: boolean) => Promise<void>>().mockResolvedValue(undefined);
const mockOuterPosition = vi.fn().mockResolvedValue({ x: 0, y: 0 });

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setIgnoreCursorEvents: mockSetIgnoreCursorEvents,
    outerPosition: mockOuterPosition,
  }),
  cursorPosition: vi.fn().mockResolvedValue({ x: 0, y: 0 }),
}));

// Import after mock is set up
import {
  enablePassthrough,
  disablePassthrough,
  isPassthroughEnabled,
  registerZoneElement,
  unregisterZoneElement,
  acquireDragLock,
  acquireModalLock,
  startPolling,
  stopPolling,
  createHitTestHandlers,
  computeInflateForPosition,
} from "../hitTest";

// ─── Helpers ────────────────────────────────────────────────

/**
 * Flush microtask queue so fire-and-forget promises (void setPassthrough)
 * settle. Uses a real microtask, not setTimeout (which fake timers intercept).
 */
async function flush(): Promise<void> {
  // A resolved promise callback runs on the microtask queue, not the timer queue.
  // Chaining two ensures that even nested microtasks from the mock settle.
  await Promise.resolve();
  await Promise.resolve();
}

// ─── Tests ──────────────────────────────────────────────────

describe("hitTest state machine", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    stopPolling(); // Ensure clean polling state
  });

  afterEach(() => {
    stopPolling();
    vi.restoreAllMocks();
  });

  // ── enablePassthrough / disablePassthrough ──

  describe("enablePassthrough", () => {
    it("should call setIgnoreCursorEvents(true) and set state to PASSTHROUGH", async () => {
      await enablePassthrough();
      expect(mockSetIgnoreCursorEvents).toHaveBeenCalledWith(true);
      expect(isPassthroughEnabled()).toBe(true);
    });
  });

  describe("disablePassthrough", () => {
    it("should call setIgnoreCursorEvents(false)", async () => {
      await enablePassthrough();
      mockSetIgnoreCursorEvents.mockClear();
      await disablePassthrough();
      expect(mockSetIgnoreCursorEvents).toHaveBeenCalledWith(false);
      expect(isPassthroughEnabled()).toBe(false);
    });

    it("should be idempotent when already disabled", async () => {
      await disablePassthrough();
      mockSetIgnoreCursorEvents.mockClear();
      await disablePassthrough();
      expect(mockSetIgnoreCursorEvents).not.toHaveBeenCalled();
    });
  });

  // ── Zone registration ──

  describe("registerZoneElement / unregisterZoneElement", () => {
    it("should register and unregister zone elements without errors", () => {
      const el = document.createElement("div");
      registerZoneElement(el);
      unregisterZoneElement(el);
    });

    it("should allow registering multiple elements", () => {
      const el1 = document.createElement("div");
      const el2 = document.createElement("div");
      registerZoneElement(el1);
      registerZoneElement(el2);
      unregisterZoneElement(el1);
      unregisterZoneElement(el2);
    });

    it("should handle unregistering an element that was never registered", () => {
      const el = document.createElement("div");
      unregisterZoneElement(el);
    });
  });

  describe("computeInflateForPosition", () => {
    it("inflates outward near the bottom-right edge", () => {
      expect(
        computeInflateForPosition(
          { x_percent: 92, y_percent: 90 },
          {
            viewport: { width: 1000, height: 1000 },
            boxPx: { width: 160, height: 48 },
          },
        ),
      ).toEqual({ right: 14, bottom: 11 });
    });

    it("does not inflate interior zones", () => {
      expect(
        computeInflateForPosition(
          { x_percent: 40, y_percent: 45 },
          {
            viewport: { width: 1000, height: 1000 },
            boxPx: { width: 160, height: 48 },
          },
        ),
      ).toEqual({});
    });

    it("uses the stack profile when requested", () => {
      expect(
        computeInflateForPosition(
          { x_percent: 85, y_percent: 88 },
          {
            kind: "stack",
            viewport: { width: 1000, height: 1000 },
            boxPx: { width: 184, height: 56 },
          },
        ),
      ).toEqual({ right: 18, bottom: 13 });
    });

    // 4 edges × 2 capsule kinds — every edge inflate must point only outward.
    // The contract is: a zone hugging the left edge must inflate `left` only,
    // never `right` (which would push the hit area inward, eating screen
    // real estate from neighbouring zones / desktop interaction). The same
    // axiom applies for every edge × kind permutation.

    describe("edge inflate is single-direction (no inward bleed)", () => {
      const viewport = { width: 1000, height: 1000 };

      const cases: Array<{
        edge: "left" | "right" | "top" | "bottom";
        kind: "zone" | "stack";
        position: { x_percent: number; y_percent: number };
        boxPx: { width: number; height: number };
        expectedKey: "left" | "right" | "top" | "bottom";
        forbiddenKey: "left" | "right" | "top" | "bottom";
      }> = [
        // ── normal zone capsule ──
        {
          edge: "left",
          kind: "zone",
          position: { x_percent: 0, y_percent: 50 },
          boxPx: { width: 160, height: 48 },
          expectedKey: "left",
          forbiddenKey: "right",
        },
        {
          edge: "right",
          kind: "zone",
          position: { x_percent: 100, y_percent: 50 },
          boxPx: { width: 160, height: 48 },
          expectedKey: "right",
          forbiddenKey: "left",
        },
        {
          edge: "top",
          kind: "zone",
          position: { x_percent: 50, y_percent: 0 },
          boxPx: { width: 160, height: 48 },
          expectedKey: "top",
          forbiddenKey: "bottom",
        },
        {
          edge: "bottom",
          kind: "zone",
          position: { x_percent: 50, y_percent: 100 },
          boxPx: { width: 160, height: 48 },
          expectedKey: "bottom",
          forbiddenKey: "top",
        },
        // ── stack capsule ──
        {
          edge: "left",
          kind: "stack",
          position: { x_percent: 0, y_percent: 50 },
          boxPx: { width: 184, height: 56 },
          expectedKey: "left",
          forbiddenKey: "right",
        },
        {
          edge: "right",
          kind: "stack",
          position: { x_percent: 100, y_percent: 50 },
          boxPx: { width: 184, height: 56 },
          expectedKey: "right",
          forbiddenKey: "left",
        },
        {
          edge: "top",
          kind: "stack",
          position: { x_percent: 50, y_percent: 0 },
          boxPx: { width: 184, height: 56 },
          expectedKey: "top",
          forbiddenKey: "bottom",
        },
        {
          edge: "bottom",
          kind: "stack",
          position: { x_percent: 50, y_percent: 100 },
          boxPx: { width: 184, height: 56 },
          expectedKey: "bottom",
          forbiddenKey: "top",
        },
      ];

      for (const tc of cases) {
        it(`${tc.kind} capsule on ${tc.edge} edge inflates only outward`, () => {
          const inflate = computeInflateForPosition(tc.position, {
            kind: tc.kind,
            viewport,
            boxPx: tc.boxPx,
          });

          // The outward edge must carry an inflate value.
          expect(inflate[tc.expectedKey]).toBeGreaterThan(0);
          // The opposing inward edge must NOT be inflated — that would push
          // the hit area into the middle of the screen.
          expect(inflate[tc.forbiddenKey]).toBeUndefined();

          // Inflate magnitude must respect the kind-specific min/max bounds.
          const min = tc.kind === "stack" ? 8 : 6;
          const max = tc.kind === "stack" ? 18 : 14;
          const v = inflate[tc.expectedKey] as number;
          expect(v).toBeGreaterThanOrEqual(min);
          expect(v).toBeLessThanOrEqual(max);
        });
      }
    });

    it("stack and normal kinds use distinct edge inflate magnitudes at the same position", () => {
      // Same on-screen position handed to both kinds must produce different
      // inflate values because the stack profile carries larger min/max.
      const viewport = { width: 1000, height: 1000 };
      const position = { x_percent: 95, y_percent: 50 };

      const zoneInflate = computeInflateForPosition(position, {
        kind: "zone",
        viewport,
        boxPx: { width: 160, height: 48 },
      });
      const stackInflate = computeInflateForPosition(position, {
        kind: "stack",
        viewport,
        boxPx: { width: 184, height: 56 },
      });

      expect(zoneInflate.right).toBeDefined();
      expect(stackInflate.right).toBeDefined();
      expect(stackInflate.right).toBeGreaterThan(zoneInflate.right as number);
    });
  });

  // ── Drag lock ──

  describe("acquireDragLock", () => {
    it("should disable passthrough when drag lock is acquired", async () => {
      await enablePassthrough();
      mockSetIgnoreCursorEvents.mockClear();

      const release = acquireDragLock();
      await flush();

      expect(mockSetIgnoreCursorEvents).toHaveBeenCalledWith(false);
      expect(isPassthroughEnabled()).toBe(false);
      release();
    });

    it("should return a release function that can be called once", async () => {
      await enablePassthrough();
      const release = acquireDragLock();
      await flush();

      release();
      await flush();
      // Calling release again should be a no-op
      release();
      await flush();
    });

    it("should keep passthrough disabled with nested drag locks", async () => {
      await enablePassthrough();

      const release1 = acquireDragLock();
      const release2 = acquireDragLock();
      await flush();

      release1();
      await flush();
      // Still locked by release2
      expect(isPassthroughEnabled()).toBe(false);

      release2();
      await flush();
    });
  });

  // ── Modal lock ──

  describe("acquireModalLock", () => {
    it("should disable passthrough when modal lock is acquired", async () => {
      await enablePassthrough();
      mockSetIgnoreCursorEvents.mockClear();

      const release = acquireModalLock();
      await flush();

      expect(mockSetIgnoreCursorEvents).toHaveBeenCalledWith(false);
      expect(isPassthroughEnabled()).toBe(false);
      release();
    });

    it("should return a release function that can be called once", async () => {
      await enablePassthrough();
      const release = acquireModalLock();
      await flush();

      release();
      await flush();
      // Second call should be no-op
      release();
      await flush();
    });

    it("should keep passthrough disabled with nested modal locks", async () => {
      await enablePassthrough();

      const release1 = acquireModalLock();
      const release2 = acquireModalLock();
      await flush();

      release1();
      await flush();
      expect(isPassthroughEnabled()).toBe(false);

      release2();
      await flush();
    });

    it("should restore PASSTHROUGH state when all modal locks released and no zones hovered", async () => {
      await enablePassthrough();
      const release = acquireModalLock();
      await flush();

      expect(isPassthroughEnabled()).toBe(false);

      release();
      await flush();

      expect(isPassthroughEnabled()).toBe(true);
    });
  });

  // ── Drag lock + Modal lock interaction ──

  describe("drag and modal lock interaction", () => {
    it("should keep passthrough disabled when modal released but drag still active", async () => {
      await enablePassthrough();

      const releaseDrag = acquireDragLock();
      const releaseModal = acquireModalLock();
      await flush();

      releaseModal();
      await flush();

      // Drag lock still active
      expect(isPassthroughEnabled()).toBe(false);

      releaseDrag();
      await flush();
    });

    it("should keep passthrough disabled when drag released but modal still active", async () => {
      await enablePassthrough();

      const releaseDrag = acquireDragLock();
      await flush();
      const releaseModal = acquireModalLock();
      await flush();

      releaseDrag();
      await flush();

      // Modal lock still active
      expect(isPassthroughEnabled()).toBe(false);

      releaseModal();
      await flush();
    });
  });

  // ── createHitTestHandlers ──

  describe("createHitTestHandlers", () => {
    it("should return onPointerEnter and onPointerLeave handlers", () => {
      const handlers = createHitTestHandlers();
      expect(typeof handlers.onPointerEnter).toBe("function");
      expect(typeof handlers.onPointerLeave).toBe("function");
    });

    it("onPointerEnter should call disablePassthrough", async () => {
      await enablePassthrough();
      mockSetIgnoreCursorEvents.mockClear();

      const handlers = createHitTestHandlers();
      handlers.onPointerEnter();
      await flush();

      expect(mockSetIgnoreCursorEvents).toHaveBeenCalledWith(false);
    });

    it("onPointerLeave should be a no-op (poller handles transition)", () => {
      const handlers = createHitTestHandlers();
      handlers.onPointerLeave();
    });
  });

  // ── Polling lifecycle ──

  describe("startPolling / stopPolling", () => {
    it("should start and stop without errors", () => {
      startPolling();
      stopPolling();
    });

    it("startPolling should be idempotent", () => {
      startPolling();
      startPolling();
      stopPolling();
    });
  });
});
