/**
 * Monitor geometry helpers.
 *
 * Bridges the BentoZone anchor-flip logic (R1) to the Rust
 * `display::monitors` IPC. Callers stay synchronous during render by
 * consulting a cached snapshot; the cache is refreshed on app start,
 * on `resolution_changed`, and on explicit `invalidate()`.
 *
 * Coordinate model:
 * - Tauri returns **physical** pixels from `list_monitors`.
 * - The webview's `window.innerWidth/Height` reports **logical** (CSS)
 *   pixels equal to the *primary* monitor's work-area at scale 1.0 —
 *   because the overlay window lives on the primary monitor.
 * - For multi-monitor we compute overflow in physical pixels using the
 *   capsule's `getBoundingClientRect()` translated via `devicePixelRatio`.
 */
import { invoke } from "@tauri-apps/api/core";
import type { MonitorInfo } from "../types/monitor";

let cache: MonitorInfo[] | null = null;
let cacheAt = 0;
/** Cache lifetime — short enough to catch hot-plug within a few seconds. */
const CACHE_TTL_MS = 2500;

export async function listMonitors(): Promise<MonitorInfo[]> {
  return invoke<MonitorInfo[]>("list_monitors");
}

export async function getMonitorForPoint(
  x: number,
  y: number
): Promise<MonitorInfo | null> {
  const m = await invoke<MonitorInfo | null>("get_monitor_for_point", { x, y });
  return m ?? null;
}

export async function getMonitorForWindow(): Promise<MonitorInfo | null> {
  const m = await invoke<MonitorInfo | null>("get_monitor_for_window");
  return m ?? null;
}

/** Populate / refresh the monitor cache. */
export async function refreshMonitors(): Promise<MonitorInfo[]> {
  const m = await listMonitors();
  cache = m;
  cacheAt = Date.now();
  return m;
}

/** Drop the cache so the next lookup fetches fresh. */
export function invalidateMonitorCache(): void {
  cache = null;
  cacheAt = 0;
}

/** Synchronous cached lookup — null until first `refreshMonitors()`. */
export function cachedMonitors(): MonitorInfo[] | null {
  if (cache && Date.now() - cacheAt > CACHE_TTL_MS) {
    // Stale — trigger refresh but still return last value so callers
    // render something instead of flashing.
    void refreshMonitors();
  }
  return cache;
}

/**
 * Find the monitor containing a given physical-pixel point in the
 * current cache. Falls back to the primary monitor, then to the first
 * entry.
 */
export function findMonitorForPointSync(
  physX: number,
  physY: number
): MonitorInfo | null {
  const mons = cache;
  if (!mons || mons.length === 0) return null;
  for (const m of mons) {
    const r = m.rect_full;
    if (
      physX >= r.x &&
      physX < r.x + r.width &&
      physY >= r.y &&
      physY < r.y + r.height
    ) {
      return m;
    }
  }
  return mons.find((m) => m.is_primary) ?? mons[0];
}

/**
 * Resolve the monitor that contains the capsule at a given DOM rect.
 * Uses `devicePixelRatio` to convert from CSS pixels (what
 * `getBoundingClientRect()` returns) to physical pixels (what the
 * Rust side stores). On DPI-mixed setups the primary monitor's scale
 * determines `devicePixelRatio`, so secondary-monitor placement still
 * works as long as the capsule lives in overlay-window coordinates.
 */
export function monitorForClientRect(
  rect: { left: number; top: number; width: number; height: number }
): MonitorInfo | null {
  const dpr = window.devicePixelRatio || 1;
  const centerX = Math.round((rect.left + rect.width / 2) * dpr);
  const centerY = Math.round((rect.top + rect.height / 2) * dpr);
  return findMonitorForPointSync(centerX, centerY);
}
