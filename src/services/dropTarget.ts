/**
 * Drop Target service — handles file drag-drop FROM Windows Explorer into BentoZone panels.
 *
 * Uses Tauri v2's OS-level onDragDropEvent which works regardless of
 * setIgnoreCursorEvents state, bypassing the HTML5 DnD limitation where
 * drag sessions started by Explorer don't flow into transparent webviews.
 *
 * Also retains HTML5 DnD handlers as a secondary path for when passthrough
 * is already disabled (e.g. zone is expanded and user drags a file onto it).
 */
import { createSignal } from "solid-js";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { UnlistenFn, Event as TauriEvent } from "@tauri-apps/api/event";
import type { DragDropEvent } from "@tauri-apps/api/webview";
import { addItem } from "../stores/zones";

/** Reactive signal tracking which zone ID is currently a drop target (if any) */
const [activeDropZone, setActiveDropZone] = createSignal<string | null>(null);

/** Track nested dragenter/dragleave events per zone to avoid flicker */
const enterCountMap = new Map<string, number>();

export { activeDropZone };

// ─── Zone element registry (mirrored from hitTest for drop position matching) ─

const dropZoneElements = new Map<HTMLElement, string>();

/**
 * Register a zone element for OS-level drop target matching.
 * Called from BentoZone onMount alongside hitTest's registerZoneElement.
 */
export function registerDropZoneElement(el: HTMLElement, zoneId: string): void {
  dropZoneElements.set(el, zoneId);
}

/**
 * Unregister a zone element from drop target matching.
 */
export function unregisterDropZoneElement(el: HTMLElement): void {
  dropZoneElements.delete(el);
}

/**
 * Find which zone (if any) contains the given physical screen coordinates.
 */
function findZoneAtPosition(screenX: number, screenY: number): string | null {
  const dpr = window.devicePixelRatio || 1;
  for (const [el, zoneId] of dropZoneElements) {
    const rect = el.getBoundingClientRect();
    const elLeft = rect.left * dpr;
    const elTop = rect.top * dpr;
    const elRight = rect.right * dpr;
    const elBottom = rect.bottom * dpr;

    if (
      screenX >= elLeft &&
      screenX <= elRight &&
      screenY >= elTop &&
      screenY <= elBottom
    ) {
      return zoneId;
    }
  }
  return null;
}

// ─── Tauri OS-level drag-drop listener ───────────────────────

let unlistenDragDrop: UnlistenFn | null = null;

/**
 * Start listening for OS-level drag-drop events from Tauri.
 * This receives drag events even when setIgnoreCursorEvents(true),
 * so files dragged from Explorer always work.
 */
export async function startDragDropListener(): Promise<void> {
  if (unlistenDragDrop) return;

  const webview = getCurrentWebview();
  unlistenDragDrop = await webview.onDragDropEvent((event: TauriEvent<DragDropEvent>) => {
    const payload = event.payload;
    switch (payload.type) {
      case "enter": {
        // Files are being dragged over the window — find which zone
        const zoneId = findZoneAtPosition(payload.position.x, payload.position.y);
        if (zoneId) {
          setActiveDropZone(zoneId);
        }
        break;
      }
      case "over": {
        // Cursor is moving during drag — update active drop zone
        const zoneId = findZoneAtPosition(payload.position.x, payload.position.y);
        setActiveDropZone(zoneId);
        break;
      }
      case "drop": {
        // Files were dropped — find target zone and add items
        const zoneId = findZoneAtPosition(payload.position.x, payload.position.y);
        setActiveDropZone(null);

        if (zoneId && payload.paths.length > 0) {
          for (const filePath of payload.paths) {
            void addItem(zoneId, filePath);
          }
        }
        break;
      }
      case "leave": {
        // Drag left the window entirely
        setActiveDropZone(null);
        break;
      }
    }
  });
}

/**
 * Stop listening for OS-level drag-drop events.
 */
export function stopDragDropListener(): void {
  if (unlistenDragDrop) {
    unlistenDragDrop();
    unlistenDragDrop = null;
  }
}

// ─── HTML5 DnD handlers (secondary path) ─────────────────────

/**
 * Check whether a DragEvent carries external files (from Explorer).
 * Returns false for internal drag operations (no files in DataTransfer).
 */
function hasExternalFiles(e: DragEvent): boolean {
  if (!e.dataTransfer) return false;
  return e.dataTransfer.types.includes("Files");
}

/**
 * Extract absolute file paths from a drop event's DataTransfer.
 * Returns an array of path strings. Filters out entries without paths.
 */
function extractFilePaths(e: DragEvent): string[] {
  if (!e.dataTransfer?.files) return [];
  const paths: string[] = [];
  for (let i = 0; i < e.dataTransfer.files.length; i++) {
    const file = e.dataTransfer.files[i];
    const filePath = (file as File & { path?: string }).path;
    if (filePath) {
      paths.push(filePath);
    }
  }
  return paths;
}

/**
 * Create HTML5 drag-drop event handlers for a zone element.
 * These serve as a secondary path for when passthrough is already disabled
 * (zone expanded, cursor hovering). The primary path is the Tauri OS-level listener.
 *
 * @param zoneId - The zone to add dropped items to
 */
export function createDropHandlers(zoneId: string) {
  const onDragEnter = (e: DragEvent) => {
    if (!hasExternalFiles(e)) return;
    e.preventDefault();
    e.stopPropagation();

    const count = (enterCountMap.get(zoneId) ?? 0) + 1;
    enterCountMap.set(zoneId, count);

    if (count === 1) {
      setActiveDropZone(zoneId);
    }
  };

  const onDragOver = (e: DragEvent) => {
    if (!hasExternalFiles(e)) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) {
      e.dataTransfer.dropEffect = "copy";
    }
  };

  const onDragLeave = (e: DragEvent) => {
    if (!hasExternalFiles(e)) return;
    e.preventDefault();
    e.stopPropagation();

    const count = Math.max(0, (enterCountMap.get(zoneId) ?? 0) - 1);
    enterCountMap.set(zoneId, count);

    if (count === 0) {
      setActiveDropZone((current) => (current === zoneId ? null : current));
    }
  };

  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();

    enterCountMap.set(zoneId, 0);
    setActiveDropZone((current) => (current === zoneId ? null : current));

    const paths = extractFilePaths(e);
    if (paths.length === 0) return;

    for (const filePath of paths) {
      void addItem(zoneId, filePath);
    }
  };

  return { onDragEnter, onDragOver, onDragLeave, onDrop };
}
