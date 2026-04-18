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

/** Last known cursor position for idle detection in PASSTHROUGH state. */
let lastCursorX = -1;
let lastCursorY = -1;
/** Consecutive frames where cursor has not moved (PASSTHROUGH only). */
let idleFrameCount = 0;
/** True when poller has switched to slow setTimeout mode due to idle. */
let idleMode = false;
/** setTimeout handle used in idle mode. */
let idleTimerId: ReturnType<typeof setTimeout> | null = null;
/** Frames of no movement before dropping to idle setTimeout polling (~5s at 30fps). */
const IDLE_FRAME_THRESHOLD = 150;

/** Current state machine state. */
let state: HitTestState = "PASSTHROUGH";

/** Hit-zone inflate values (CSS pixels) — expands the hit rect directionally. */
export interface RegisterZoneInflate {
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
}

/** Options accepted when registering a zone element for hit-testing. */
export interface RegisterZoneOpts {
  inflate?: RegisterZoneInflate;
}

/** Registered zone bounding rects, keyed by zone element reference. */
const zoneElements = new Map<HTMLElement, RegisterZoneInflate>();

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
 * `opts.inflate` directionally enlarges the hit rect in CSS pixels — useful
 * for capsules sitting near screen edges where the user mouse path approaches
 * from outside the capsule bounds.
 */
export function registerZoneElement(
  el: HTMLElement,
  opts?: RegisterZoneOpts,
): void {
  zoneElements.set(el, opts?.inflate ?? {});
}

/**
 * Update the inflate values for an already-registered zone element.
 * No-op if the element was not previously registered.
 */
export function updateZoneInflate(
  el: HTMLElement,
  inflate: RegisterZoneInflate,
): void {
  if (zoneElements.has(el)) {
    zoneElements.set(el, inflate);
  }
}

/**
 * Compute directional hit-zone inflate based on a zone's percentage position.
 * Zones within 10%/90% of either axis get 12px inflate outward (toward the
 * nearest screen edge) so the capsule "pulls in" cursors approaching from
 * off-edge taskbar territory.
 *
 * Single source of truth for both BentoZone's runtime registration and
 * DebugOverlay's visualization.
 */
export function computeInflateForPosition(
  pos: { x_percent: number; y_percent: number },
): RegisterZoneInflate {
  const EDGE_THRESHOLD = 90;
  const EXPAND_PX = 12;
  const inflate: RegisterZoneInflate = {};
  if (pos.y_percent > EDGE_THRESHOLD) inflate.bottom = EXPAND_PX;
  if (pos.y_percent < 10) inflate.top = EXPAND_PX;
  if (pos.x_percent > EDGE_THRESHOLD) inflate.right = EXPAND_PX;
  if (pos.x_percent < 10) inflate.left = EXPAND_PX;
  return inflate;
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
 *
 * The window's screen position (`windowScreenX/Y`) is needed when the taskbar
 * is on the left or top edge, because the work area (and thus the window) is
 * offset from (0,0) in screen coordinates.
 */
function isPointInElement(
  screenX: number,
  screenY: number,
  el: HTMLElement,
  inflate?: RegisterZoneInflate,
): boolean {
  const rect = el.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const inT = (inflate?.top ?? 0) * dpr;
  const inR = (inflate?.right ?? 0) * dpr;
  const inB = (inflate?.bottom ?? 0) * dpr;
  const inL = (inflate?.left ?? 0) * dpr;
  // Convert viewport-relative CSS pixels to physical screen coordinates
  // by scaling by DPR and adding the cached window screen position.
  const elLeft = rect.left * dpr + windowScreenX - inL;
  const elTop = rect.top * dpr + windowScreenY - inT;
  const elRight = rect.right * dpr + windowScreenX + inR;
  const elBottom = rect.bottom * dpr + windowScreenY + inB;

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

    // Idle detection: track cursor movement in PASSTHROUGH to throttle IPC polling
    if (state === "PASSTHROUGH") {
      if (pos.x === lastCursorX && pos.y === lastCursorY) {
        idleFrameCount++;
        if (!idleMode && idleFrameCount >= IDLE_FRAME_THRESHOLD) {
          enterIdleMode();
        }
      } else {
        // Cursor moved — exit idle mode if active
        if (idleMode) {
          exitIdleMode();
          return; // exitIdleMode schedules the next rAF poll; don't double-schedule
        }
        idleFrameCount = 0;
        lastCursorX = pos.x;
        lastCursorY = pos.y;
      }
    } else {
      // Not in PASSTHROUGH — always reset idle state so rAF resumes if we return to it
      if (idleMode) {
        idleMode = false;
        idleFrameCount = 0;
      }
      lastCursorX = pos.x;
      lastCursorY = pos.y;
    }

    let foundZone: HTMLElement | null = null;

    for (const [el, inflate] of zoneElements) {
      if (isPointInElement(pos.x, pos.y, el, inflate)) {
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

  // In idle mode (PASSTHROUGH + cursor stationary), use slow setTimeout instead of rAF
  if (idleMode) {
    idleTimerId = setTimeout(() => {
      idleTimerId = null;
      void pollCursorPosition();
    }, 100);
    return;
  }

  animFrameId = requestAnimationFrame(() => {
    // Increment with wraparound to prevent unbounded growth.
    // Using bitwise OR 0 to keep it as a 32-bit integer.
    frameCount = (frameCount + 1) | 0;
    // Throttle to ~30fps in PASSTHROUGH state — no need for 60fps precision
    // when nothing interactive is happening. Each poll is an async IPC call.
    if (state === "PASSTHROUGH" && (frameCount & 1) !== 0) {
      scheduleNextPoll();
      return;
    }
    void pollCursorPosition();
  });
}

/**
 * Enter idle polling mode: cancel rAF loop, switch to setTimeout(100ms).
 * Called when cursor has been stationary in PASSTHROUGH for IDLE_FRAME_THRESHOLD frames.
 */
function enterIdleMode(): void {
  if (idleMode) return;
  idleMode = true;
  // Cancel current rAF if pending — idle mode uses setTimeout instead
  if (animFrameId !== null) {
    cancelAnimationFrame(animFrameId);
    animFrameId = null;
  }
}

/**
 * Exit idle polling mode: reset idle counters and resume requestAnimationFrame.
 * Called when cursor movement is detected.
 */
function exitIdleMode(): void {
  if (!idleMode) return;
  idleMode = false;
  idleFrameCount = 0;
  if (idleTimerId !== null) {
    clearTimeout(idleTimerId);
    idleTimerId = null;
  }
  scheduleNextPoll();
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
  idleMode = false;
  idleFrameCount = 0;
  clearGraceTimer();
  if (animFrameId !== null) {
    cancelAnimationFrame(animFrameId);
    animFrameId = null;
  }
  if (idleTimerId !== null) {
    clearTimeout(idleTimerId);
    idleTimerId = null;
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
