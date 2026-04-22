/**
 * Solid.js store for BentoZone data with CRUD operations.
 * All mutations go through IPC to the backend, then update the local store.
 */
import { createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import type {
  BentoZone,
  BentoItem,
  RelativePosition,
  RelativeSize,
  ZoneUpdate,
} from "../types/zone";
import * as ipc from "../services/ipc";

interface ZonesState {
  zones: BentoZone[];
  loading: boolean;
  error: string | null;
}

const [state, setState] = createStore<ZonesState>({
  zones: [],
  loading: false,
  error: null,
});

/**
 * Structured error surfaced by the backend `add_item` command.
 * When JSON.parse(err) yields `{code: "OUTSIDE_DESKTOP", path, allowed_sources}`,
 * this signal is populated so UI layers (toasts / inline banners) can render
 * a friendly message listing the allowed Desktop sources.
 */
export interface OutsideDesktopError {
  path: string;
  allowed_sources: string[];
}

const [outsideDesktopError, setOutsideDesktopError] =
  createSignal<OutsideDesktopError | null>(null);

export function getOutsideDesktopError(): OutsideDesktopError | null {
  return outsideDesktopError();
}

export function clearOutsideDesktopError(): void {
  setOutsideDesktopError(null);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((s): s is string => typeof s === "string");
}

function parseOutsideDesktopError(message: string): OutsideDesktopError | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(message);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  if (!("code" in parsed) || parsed.code !== "OUTSIDE_DESKTOP") return null;
  if (!("path" in parsed) || typeof parsed.path !== "string") return null;
  if (!("allowed_sources" in parsed) || !isStringArray(parsed.allowed_sources)) return null;
  return { path: parsed.path, allowed_sources: parsed.allowed_sources };
}

// ─── Read-only accessors ─────────────────────────────────────

export function getZones(): BentoZone[] {
  return state.zones;
}

export function getZoneById(id: string): BentoZone | undefined {
  return state.zones.find((z) => z.id === id);
}

export function isLoading(): boolean {
  return state.loading;
}

export function getError(): string | null {
  return state.error;
}

// ─── Data loading ────────────────────────────────────────────

export async function loadZones(): Promise<void> {
  setState("loading", true);
  setState("error", null);
  try {
    const zones = await ipc.listZones();
    setState("zones", zones);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setState("error", message);
  } finally {
    setState("loading", false);
  }
}

// ─── Zone CRUD ───────────────────────────────────────────────

export async function createZone(
  name: string,
  icon: string,
  position: RelativePosition,
  expandedSize: RelativeSize
): Promise<BentoZone | null> {
  try {
    const zone = await ipc.createZone(name, icon, position, expandedSize);
    setState(
      produce((s) => {
        s.zones.push(zone);
      })
    );
    return zone;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setState("error", message);
    return null;
  }
}

export async function updateZone(
  id: string,
  updates: ZoneUpdate
): Promise<BentoZone | null> {
  try {
    const updated = await ipc.updateZone(id, updates);
    setState(
      produce((s) => {
        const idx = s.zones.findIndex((z) => z.id === id);
        if (idx !== -1) {
          s.zones[idx] = updated;
        }
      })
    );
    return updated;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setState("error", message);
    return null;
  }
}

export async function deleteZone(id: string): Promise<boolean> {
  try {
    await ipc.deleteZone(id);
    setState(
      produce((s) => {
        const idx = s.zones.findIndex((z) => z.id === id);
        if (idx !== -1) {
          s.zones.splice(idx, 1);
        }
      })
    );
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setState("error", message);
    return false;
  }
}

export async function reorderZones(zoneIds: string[]): Promise<boolean> {
  try {
    await ipc.reorderZones(zoneIds);
    setState(
      produce((s) => {
        s.zones.sort((a, b) => {
          const aIdx = zoneIds.indexOf(a.id);
          const bIdx = zoneIds.indexOf(b.id);
          return aIdx - bIdx;
        });
        // Update sort_order to match new positions
        for (let i = 0; i < s.zones.length; i++) {
          s.zones[i].sort_order = i;
        }
      })
    );
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setState("error", message);
    return false;
  }
}

// ─── Item operations ─────────────────────────────────────────

export async function addItem(
  zoneId: string,
  path: string
): Promise<BentoItem | null> {
  try {
    const item = await ipc.addItem(zoneId, path);
    setState(
      produce((s) => {
        const zone = s.zones.find((z) => z.id === zoneId);
        if (zone) {
          zone.items.push(item);
        }
      })
    );
    return item;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const structured = parseOutsideDesktopError(message);
    if (structured) {
      setOutsideDesktopError(structured);
      // The structured toast owns this case end-to-end. Leaving the raw
      // JSON in state.error would surface as a literal `{"code":...}`
      // string in the generic toast the moment the user dismisses the
      // structured one.
      setState("error", null);
    } else {
      setState("error", message);
    }
    return null;
  }
}

export async function removeItem(
  zoneId: string,
  itemId: string
): Promise<boolean> {
  try {
    await ipc.removeItem(zoneId, itemId);
    setState(
      produce((s) => {
        const zone = s.zones.find((z) => z.id === zoneId);
        if (zone) {
          const idx = zone.items.findIndex((i) => i.id === itemId);
          if (idx !== -1) {
            zone.items.splice(idx, 1);
          }
        }
      })
    );
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setState("error", message);
    return false;
  }
}

export async function moveItem(
  fromZoneId: string,
  toZoneId: string,
  itemId: string
): Promise<boolean> {
  try {
    await ipc.moveItem(fromZoneId, toZoneId, itemId);
    // Granular local update: move item between zones in the store.
    // Avoids loadZones() which replaces the entire zones array and
    // triggers full re-render of all zones (causing visual flicker).
    setState(
      produce((s) => {
        const fromZone = s.zones.find((z) => z.id === fromZoneId);
        const toZone = s.zones.find((z) => z.id === toZoneId);
        if (fromZone && toZone) {
          const idx = fromZone.items.findIndex((i) => i.id === itemId);
          if (idx !== -1) {
            const [item] = fromZone.items.splice(idx, 1);
            toZone.items.push(item);
          }
        }
      })
    );
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setState("error", message);
    return false;
  }
}

export async function reorderItems(
  zoneId: string,
  itemIds: string[]
): Promise<boolean> {
  try {
    await ipc.reorderItems(zoneId, itemIds);
    setState(
      produce((s) => {
        const zone = s.zones.find((z) => z.id === zoneId);
        if (zone) {
          zone.items.sort((a, b) => {
            const aIdx = itemIds.indexOf(a.id);
            const bIdx = itemIds.indexOf(b.id);
            return aIdx - bIdx;
          });
        }
      })
    );
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setState("error", message);
    return false;
  }
}

export async function toggleItemWide(
  zoneId: string,
  itemId: string
): Promise<BentoItem | null> {
  try {
    const updated = await ipc.toggleItemWide(zoneId, itemId);
    setState(
      produce((s) => {
        const zone = s.zones.find((z) => z.id === zoneId);
        if (zone) {
          const idx = zone.items.findIndex((i) => i.id === itemId);
          if (idx !== -1) {
            zone.items[idx] = updated;
          }
        }
      })
    );
    return updated;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setState("error", message);
    return null;
  }
}

// ─── File change reactions ───────────────────────────────────

/**
 * Handle a file system change event. Updates items whose path matches.
 *
 * event_type values (from backend): "create" | "modify" | "delete"
 * - delete: remove matching items from all zones
 * - modify: if old_path is set, treat as rename (update path + name);
 *           otherwise mark icon_hash stale and re-extract via preloadIcons
 * - create: reload zones if any zone has auto_group rules (new file may match)
 */
export function handleFileChanged(
  eventType: string,
  path: string,
  oldPath: string | null
): void {
  if (eventType === "delete") {
    setState(
      produce((s) => {
        for (const zone of s.zones) {
          const idx = zone.items.findIndex((i) => i.path === path);
          if (idx !== -1) {
            zone.items.splice(idx, 1);
          }
        }
      })
    );
  } else if (eventType === "modify") {
    const stalePaths: string[] = [];
    setState(
      produce((s) => {
        for (const zone of s.zones) {
          if (oldPath) {
            // File was renamed/moved — update path and display name
            for (const item of zone.items) {
              if (item.path === oldPath) {
                item.path = path;
                const segments = path.replace(/\\/g, "/").split("/");
                item.name = segments[segments.length - 1] ?? item.name;
                item.icon_hash = "";
                stalePaths.push(path);
              }
            }
          } else {
            // File content modified — icon may have changed
            for (const item of zone.items) {
              if (item.path === path) {
                item.icon_hash = "";
                stalePaths.push(path);
              }
            }
          }
        }
      })
    );
    // Re-extract icons for affected items so the UI refreshes
    if (stalePaths.length > 0) {
      void ipc.preloadIcons(stalePaths);
    }
  } else if (eventType === "create") {
    // A new file appeared on the desktop. If any zone uses auto-grouping,
    // try to automatically add the file to matching zones via the backend.
    const hasAutoGroupZones = state.zones.some((z) => z.auto_group !== null);
    if (hasAutoGroupZones) {
      void ipc.autoGroupNewFile(path).then((added) => {
        if (added.length > 0) {
          // Granular local update: splice each returned item into its zone.
          // Avoids loadZones() which replaces the entire zones array and
          // triggers full re-render of all zones (causing visual flicker).
          setState(
            produce((s) => {
              for (const [zoneId, item] of added) {
                const zone = s.zones.find((z) => z.id === zoneId);
                if (zone && !zone.items.some((i) => i.id === item.id)) {
                  zone.items.push(item);
                }
              }
            })
          );
        }
      }).catch((err) => {
        console.warn("Auto-group new file failed:", err);
      });
    }
  }
}

// ─── Store export ────────────────────────────────────────────

export const zonesStore = {
  get zones() {
    return state.zones;
  },
  get loading() {
    return state.loading;
  },
  get error() {
    return state.error;
  },
};
