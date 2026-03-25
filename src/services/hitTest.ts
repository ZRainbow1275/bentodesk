/**
 * Hit-testing state machine using Tauri's setIgnoreCursorEvents + cursorPosition polling.
 *
 * States:
 *   PASSTHROUGH  — default: clicks go through to the desktop
 *   ZONE_HOVER   — cursor is over a zone, clicks captured by webview
 *   DRAGGING     — a zone capsule is being repositioned; always captured
 *   MODAL_OPEN   — a modal/dialog is visible; always captured
 *   GRACE_PERIOD — cursor just left a zone; short delay before PASSTHROUGH
 *                  so that DOM mouseLeave fires and collapse timers start
 *
 * Transitions:
 *   PASSTHROUGH  → ZONE_HOVER   (poller detects cursor over zone)
 *   ZONE_HOVER   → GRACE_PERIOD (poller detects cursor left zone)
 *   ZONE_HOVER   → DRAGGING     (acquireDragLock called)
 *   GRACE_PERIOD → PASSTHROUGH  (grace timer expires, cursor still outside)
 *   GRACE_PERIOD → ZONE_HOVER   (cursor re-enters a zone before timer expires)
 *   DRAGGING     → ZONE_HOVER   (releaseDragLock, cursor over zone)
 *   DRAGGING     → GRACE_PERIOD (releaseDragLock, cursor outside zone)
 *   any          → MODAL_OPEN   (acquireModalLock, modalLockCount becomes 1)
 *   MODAL_OPEN   → (previous)   (releaseModalLock, modalLockCount becomes 0)
 */
import { getCurrentWindow, cursorPosition, type Window as TauriWindow } from "@tauri-apps/api/window";

// ─── Types ───────────────────────────────────────────────────

type HitTestState =
  | "PASSTHROUGH"
  | "ZONE_HOVER"
  | "DRAGGING"
  | "MODAL_OPEN"
  | "GRACE_PERIOD";

// ─── State ───────────────────────────────────────────────────

let isIgnoring = false;
let pollingActive = false;
let animFrameId: number | null = null;

/** Frame counter for throttling polls in low-priority states. */
let frameCount = 0;

/** Current state machine state. */
let state: HitTestState = "PASSTHROUGH";

/** Registered zone bounding rects, keyed by zone element reference. */
const zoneElements = new Set<HTMLElement>();

/** When > 0, passthrough is force-disabled (modal/overlay captures events). */
let modalLockCount = 0;

/** When > 0, a drag operation locks passthrough off. */
let dragLockCount = 0;

/** Currently hovered zone element (null if cursor is not over any zone). */
let hoveredZone: HTMLElement | null = null;

/** Grace period timer ID. */
let graceTimerId: ReturnType<typeof setTimeout> | null = null;

/** Grace period duration in ms — long enough for DOM mouseLeave + collapse timer start. */
const GRACE_PERIOD_MS = 350;

// ─── Low-level passthrough toggling ──────────────────────────

async function setPassthrough(ignore: boolean): Promise<void> {
  if (isIgnoring === ignore) return;
  await getCurrentWindow().setIgnoreCursorEvents(ignore);
  isIgnoring = ignore;
}

function clearGraceTimer(): void {
  if (graceTimerId !== null) {
    clearTimeout(graceTimerId);
    graceTimerId = null;
  }
}

/**
 * Transition the state machine. This is the single place where state changes
 * and passthrough toggling are coordinated.
 */
function transitionTo(next: HitTestState): void {
  if (state === next) return;
  state = next;

  switch (next) {
    case "PASSTHROUGH":
      void setPassthrough(true);
      break;
    case "ZONE_HOVER":
    case "DRAGGING":
    case "MODAL_OPEN":
    case "GRACE_PERIOD":
      // All non-passthrough states: capture events
      void setPassthrough(false);
      break;
  }
}

// ─── Public API: passthrough control ─────────────────────────

/**
 * Enable click-through passthrough (clicks go to desktop).
 * Called on app mount to make the overlay transparent to input by default.
 * Also fetches the initial window screen position for hit-test coordinate conversion.
 */
export async function enablePassthrough(): Promise<void> {
  await setPassthrough(true);
  await refreshWindowPosition();
  state = "PASSTHROUGH";
}

/**
 * Disable passthrough so the window captures cursor events.
 * Called when the cursor enters a BentoDesk UI element.
 */
export async function disablePassthrough(): Promise<void> {
  await setPassthrough(false);
}

/**
 * Query current passthrough state.
 */
export function isPassthroughEnabled(): boolean {
  return isIgnoring;
}

// ─── Zone registration ───────────────────────────────────────

/**
 * Register a zone DOM element for cursor hit-testing.
 * The polling loop will check the cursor against this element's bounding rect.
 */
export function registerZoneElement(el: HTMLElement): void {
  zoneElements.add(el);
}

/**
 * Unregister a zone DOM element from cursor hit-testing.
 */
export function unregisterZoneElement(el: HTMLElement): void {
  zoneElements.delete(el);
  if (hoveredZone === el) {
    hoveredZone = null;
  }
}

// ─── Drag lock ───────────────────────────────────────────────

/**
 * Acquire a drag lock — forces passthrough off during zone repositioning.
 * The poller will NOT re-enable passthrough while the drag lock is held,
 * preventing the jerkiness caused by the element moving under the cursor.
 * Returns a release function.
 */
export function acquireDragLock(): () => void {
  dragLockCount++;
  clearGraceTimer();
  transitionTo("DRAGGING");

  let released = false;
  return () => {
    if (released) return;
    released = true;
    dragLockCount = Math.max(0, dragLockCount - 1);
    if (dragLockCount === 0) {
      // After drag ends, check if cursor is still over a zone
      if (modalLockCount > 0) {
        transitionTo("MODAL_OPEN");
      } else if (hoveredZone !== null) {
        transitionTo("ZONE_HOVER");
      } else {
        // Start grace period so DOM events can fire
        startGracePeriod();
      }
    }
  };
}

// ─── Modal lock ──────────────────────────────────────────────

/**
 * Acquire a modal lock — forces passthrough off while any modal is open.
 * Returns a release function.
 */
export function acquireModalLock(): () => void {
  modalLockCount++;
  clearGraceTimer();
  if (modalLockCount === 1) {
    transitionTo("MODAL_OPEN");
  }

  let released = false;
  return () => {
    if (released) return;
    released = true;
    modalLockCount = Math.max(0, modalLockCount - 1);
    if (modalLockCount === 0) {
      // Restore appropriate state
      if (dragLockCount > 0) {
        transitionTo("DRAGGING");
      } else if (hoveredZone !== null) {
        transitionTo("ZONE_HOVER");
      } else {
        transitionTo("PASSTHROUGH");
      }
    }
  };
}

// ─── Grace period ────────────────────────────────────────────

function startGracePeriod(): void {
  clearGraceTimer();
  transitionTo("GRACE_PERIOD");
  graceTimerId = setTimeout(() => {
    graceTimerId = null;
    // Only transition to passthrough if still in grace period
    // (cursor might have re-entered a zone during the grace window)
    if (state === "GRACE_PERIOD" && dragLockCount === 0 && modalLockCount === 0) {
      transitionTo("PASSTHROUGH");
    }
  }, GRACE_PERIOD_MS);
}

// ─── Window position cache ───────────────────────────────────

/** Cached window screen position (physical pixels). Updated periodically. */
let windowScreenX = 0;
let windowScreenY = 0;
/** Frame counter for window position refresh (every ~60 frames ≈ 1s). */
let windowPosRefreshCounter = 0;
const WINDOW_POS_REFRESH_INTERVAL = 60;

async function refreshWindowPosition(): Promise<void> {
  try {
    const pos = await getCurrentWindow().outerPosition();
    windowScreenX = pos.x;
    windowScreenY = pos.y;
  } catch {
    // Ignore — position stays at last known value
  }
}

// ─── Cursor position polling ─────────────────────────────────

/**
 * Check if a physical screen point (px) is inside an element's bounding rect.
 * cursorPosition() returns physical (screen) coordinates; getBoundingClientRect()
 * returns viewport-relative CSS pixels. We convert viewport-relative to screen
 * coordinates by scaling by DPR and adding the window's screen position.
 */
function isPointInElement(
  screenX: number,
  screenY: number,
  el: HTMLElement,
): boolean {
  const rect = el.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  // Convert viewport-relative CSS pixels to physical screen coordinates.
  // The window is positioned at (0,0) covering the work area, so no
  // additional offset is needed — screen coords and window coords align.
  const elLeft = rect.left * dpr;
  const elTop = rect.top * dpr;
  const elRight = rect.right * dpr;
  const elBottom = rect.bottom * dpr;

  return (
    screenX >= elLeft &&
    screenX <= elRight &&
    screenY >= elTop &&
    screenY <= elBottom
  );
}

async function pollCursorPosition(): Promise<void> {
  // Skip polling when modal or drag lock is active — passthrough is already disabled
  if (modalLockCount > 0 || dragLockCount > 0) {
    scheduleNextPoll();
    return;
  }

  // Periodically refresh window position to handle taskbar on left/top
  windowPosRefreshCounter++;
  if (windowPosRefreshCounter >= WINDOW_POS_REFRESH_INTERVAL) {
    windowPosRefreshCounter = 0;
    await refreshWindowPosition();
  }

  try {
    const pos = await cursorPosition();
    let foundZone: HTMLElement | null = null;

    for (const el of zoneElements) {
      if (isPointInElement(pos.x, pos.y, el)) {
        foundZone = el;
        break;
      }
    }

    if (foundZone !== null) {
      // Cursor is over a zone
      hoveredZone = foundZone;
      if (state !== "ZONE_HOVER") {
        clearGraceTimer();
        transitionTo("ZONE_HOVER");
      }
    } else if (hoveredZone !== null || state === "ZONE_HOVER") {
      // Cursor left all zones — start grace period instead of immediate passthrough
      hoveredZone = null;
      if (state === "ZONE_HOVER") {
        startGracePeriod();
      }
    }
    // If state is GRACE_PERIOD and cursor is still outside, the timer handles transition
    // If state is PASSTHROUGH and cursor is still outside, nothing to do
  } catch {
    // cursorPosition can fail if window is closing — silently ignore
  }

  scheduleNextPoll();
}

function scheduleNextPoll(): void {
  if (!pollingActive) return;
  animFrameId = requestAnimationFrame(() => {
    frameCount++;
    // Throttle to ~30fps in PASSTHROUGH state — no need for 60fps precision
    // when nothing interactive is happening. Each poll is an async IPC call.
    if (state === "PASSTHROUGH" && frameCount % 2 !== 0) {
      scheduleNextPoll();
      return;
    }
    void pollCursorPosition();
  });
}

/**
 * Start the cursor position polling loop.
 * Call once on app mount after enablePassthrough().
 */
export function startPolling(): void {
  if (pollingActive) return;
  pollingActive = true;
  scheduleNextPoll();
}

/**
 * Stop the cursor position polling loop.
 */
export function stopPolling(): void {
  pollingActive = false;
  clearGraceTimer();
  if (animFrameId !== null) {
    cancelAnimationFrame(animFrameId);
    animFrameId = null;
  }
}

// ─── Legacy handler factory (kept for backward compat) ───────

/**
 * Unified handler for pointer enter/leave on interactive elements.
 * Now that polling handles the passthrough toggling, these handlers
 * serve as a fast-path: when DOM events DO fire (because passthrough
 * was already disabled by the poller), they reinforce the correct state.
 */
export function createHitTestHandlers(): {
  onPointerEnter: () => void;
  onPointerLeave: () => void;
} {
  return {
    onPointerEnter: () => {
      void disablePassthrough();
    },
    onPointerLeave: () => {
      if (modalLockCount === 0 && dragLockCount === 0) {
        // Don't immediately enable passthrough — let the poller + grace period handle it.
        // This prevents race conditions where the DOM event fires before the poller
        // has had a chance to detect the cursor's new position.
      }
    },
  };
}
