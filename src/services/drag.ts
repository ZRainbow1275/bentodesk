/**
 * Drag interaction coordinator.
 *
 * Two drag modes:
 * 1. **OLE drag** — Detects drag intent from mousedown + movement that exits
 *    the webview, then delegates to the backend OLE drag operation via IPC.
 *    Used when dragging items OUT to Windows Explorer.
 * 2. **Internal reorder drag** — Tracks an item being moved within or between
 *    zones. Ghost card follows cursor, other cards shift. On drop, calls
 *    reorderItems() or moveItem() on the zones store.
 */
import { createSignal } from "solid-js";
import { startDrag } from "./ipc";
import { reorderItems, moveItem, getZoneById } from "../stores/zones";

/** Minimum pixel movement to detect drag intent */
const DRAG_THRESHOLD_PX = 5;

// ─── Internal reorder drag state (reactive) ─────────────────

export interface InternalDragState {
  /** Item being dragged */
  itemId: string;
  /** Source zone */
  sourceZoneId: string;
  /** Current hover target zone (may differ from source for cross-zone) */
  targetZoneId: string;
  /** Ghost insertion index within the target zone's item list */
  targetIndex: number;
  /** Current cursor position for ghost card positioning */
  cursorX: number;
  cursorY: number;
  /** File path of the dragged item (for preview icon) */
  filePath: string;
  /** Display name of the dragged item (for preview label) */
  itemName: string;
}

const [internalDrag, setInternalDrag] = createSignal<InternalDragState | null>(
  null
);

export { internalDrag };

// ─── Multi-drag state (Theme C) ─────────────────────────────

/**
 * Additional items fanned out behind the primary drag ghost when the user
 * picked up a selection larger than one. `primary` is still `internalDrag`;
 * this stores the *passenger* items so the ghost can render a deck + the
 * commit path can move them all under a single IPC.
 */
export interface InternalDragPassenger {
  itemId: string;
  sourceZoneId: string;
  filePath: string;
  itemName: string;
}

const [internalDragMulti, setInternalDragMulti] = createSignal<
  InternalDragPassenger[] | null
>(null);
/**
 * `true` when Alt is held at mousedown → multi-drag copies instead of moves.
 * Reset on mouseup.
 */
let multiDragCopyMode = false;

export { internalDragMulti };

export function getMultiDragCopyMode(): boolean {
  return multiDragCopyMode;
}

export function setMultiDragPassengers(
  passengers: InternalDragPassenger[] | null,
  copy: boolean
): void {
  setInternalDragMulti(passengers);
  multiDragCopyMode = copy;
}

export function clearMultiDrag(): void {
  setInternalDragMulti(null);
  multiDragCopyMode = false;
}

export function updateInternalDragTarget(
  targetZoneId: string,
  targetIndex: number
): void {
  setInternalDrag((prev) => {
    if (!prev) return null;
    return { ...prev, targetZoneId, targetIndex };
  });
}

export function updateInternalDragCursor(x: number, y: number): void {
  setInternalDrag((prev) => {
    if (!prev) return null;
    return { ...prev, cursorX: x, cursorY: y };
  });
}

/**
 * Complete the internal drag — reorder within zone or move across zones.
 * MUST be awaited so the store is updated before the ghost card is removed.
 *
 * When `internalDragMulti()` is non-empty, passenger items follow the
 * primary into the target zone. Alt-held multi-drag is accepted by the
 * copy flag but v1.2.0 still performs a move (copy semantics land with a
 * backend `duplicate_items` IPC in a follow-up).
 */
async function commitInternalDrag(state: InternalDragState): Promise<void> {
  if (state.sourceZoneId === state.targetZoneId) {
    // Same zone: reorder — compute new item order from the store directly
    const zone = getZoneById(state.sourceZoneId);
    if (!zone) return;
    const currentIds = zone.items.map((i) => i.id);
    const filtered = currentIds.filter((id) => id !== state.itemId);
    const insertAt = Math.min(state.targetIndex, filtered.length);
    filtered.splice(insertAt, 0, state.itemId);
    await reorderItems(state.sourceZoneId, filtered);
  } else {
    // Cross-zone: move item
    await moveItem(state.sourceZoneId, state.targetZoneId, state.itemId);
  }

  const passengers = internalDragMulti();
  if (passengers && passengers.length > 0) {
    for (const p of passengers) {
      if (p.itemId === state.itemId) continue;
      if (p.sourceZoneId === state.targetZoneId) continue;
      try {
        await moveItem(p.sourceZoneId, state.targetZoneId, p.itemId);
      } catch (err) {
        console.warn("multi-drag passenger failed:", err);
      }
    }
    clearMultiDrag();
  }
}

function cancelInternalDrag(): void {
  setInternalDrag(null);
}

// ─── OLE drag state (non-reactive) ─────────────────────────

interface OleDragState {
  startX: number;
  startY: number;
  filePaths: string[];
  itemId: string;
  zoneId: string;
  itemName: string;
  isDragging: boolean;
  cleanup: (() => void) | null;
}

let activeDrag: OleDragState | null = null;

/**
 * Initiate drag tracking on mousedown for an item.
 * Detects whether the drag becomes an internal reorder (small movements within
 * the webview) or an OLE external drag (larger movement).
 *
 * @param filePaths - Paths of files being dragged (for OLE mode)
 * @param startX - Initial clientX from the mouse event
 * @param startY - Initial clientY from the mouse event
 * @param itemId - ID of the item being dragged
 * @param zoneId - ID of the zone the item belongs to
 * @param itemName - Display name of the item (for drag preview)
 */
export function beginDragTracking(
  filePaths: string[],
  startX: number,
  startY: number,
  itemId?: string,
  zoneId?: string,
  itemName?: string
): void {
  // Clean up any existing drag
  cancelDragTracking();

  const state: OleDragState = {
    startX,
    startY,
    filePaths,
    itemId: itemId ?? "",
    zoneId: zoneId ?? "",
    itemName: itemName ?? "",
    isDragging: false,
    cleanup: null,
  };

  const onMouseMove = (e: MouseEvent) => {
    if (state.isDragging) {
      // Already in internal drag mode — update cursor position
      updateInternalDragCursor(e.clientX, e.clientY);

      // Hit-test which zone/grid cell the cursor is over
      const targetEl = document.elementFromPoint(e.clientX, e.clientY);
      if (targetEl) {
        const zoneEl = targetEl.closest(".bento-zone");
        if (zoneEl) {
          const targetZoneId =
            zoneEl.getAttribute("data-zone-id") ?? state.zoneId;
          const gridEl = targetEl.closest(".item-grid");
          if (gridEl) {
            // Cursor is over a grid — compute precise insertion index
            const cards = Array.from(gridEl.querySelectorAll(".item-card"));
            let insertIndex = cards.length;

            for (let i = 0; i < cards.length; i++) {
              const rect = cards[i].getBoundingClientRect();
              const midY = rect.top + rect.height / 2;
              const midX = rect.left + rect.width / 2;
              if (
                e.clientY < midY ||
                (e.clientY < rect.bottom && e.clientX < midX)
              ) {
                insertIndex = i;
                break;
              }
            }

            updateInternalDragTarget(targetZoneId, insertIndex);
          } else {
            // Cursor is over a zone but not a grid (e.g. zen capsule or header)
            // — mark this zone as the target, append at end
            updateInternalDragTarget(targetZoneId, 0);
          }
        }
      }
      return;
    }

    const dx = e.clientX - state.startX;
    const dy = e.clientY - state.startY;
    const distance = Math.sqrt(dx * dx + dy * dy);

    if (distance >= DRAG_THRESHOLD_PX) {
      state.isDragging = true;

      if (state.itemId && state.zoneId) {
        // Start internal reorder drag
        setInternalDrag({
          itemId: state.itemId,
          sourceZoneId: state.zoneId,
          targetZoneId: state.zoneId,
          targetIndex: 0,
          cursorX: e.clientX,
          cursorY: e.clientY,
          filePath: state.filePaths[0] ?? "",
          itemName: state.itemName,
        });
      } else {
        // No item/zone context — fallback to OLE external drag
        void executeDrag(state.filePaths);
        cleanupListeners();
      }
    }
  };

  const onMouseUp = async () => {
    const dragState = internalDrag();
    if (dragState) {
      // MUST await so the store is updated BEFORE the ghost card is removed.
      // Without await, clearing internalDrag causes items to momentarily
      // snap back to their old positions (flicker/jump bug).
      await commitInternalDrag(dragState);
      cancelInternalDrag();
    }
    cleanupListeners();
  };

  const cleanupListeners = () => {
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseup", onMouseUp);
    activeDrag = null;
  };

  state.cleanup = cleanupListeners;

  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("mouseup", onMouseUp);

  activeDrag = state;
}

/**
 * Cancel any active drag tracking.
 */
export function cancelDragTracking(): void {
  cancelInternalDrag();
  if (activeDrag?.cleanup) {
    activeDrag.cleanup();
    activeDrag = null;
  }
}

/**
 * Execute the actual OLE drag operation via the backend.
 */
async function executeDrag(filePaths: string[]): Promise<string> {
  try {
    const result = await startDrag(filePaths);
    return result;
  } catch (err) {
    console.error("Drag operation failed:", err);
    return "cancelled";
  }
}
