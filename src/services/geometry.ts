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
 *
 * v8 round-4 fix: on mixed-DPI multi-monitor setups (e.g. 96 dpi primary +
 * 192 dpi secondary) using the global `window.devicePixelRatio` to scale
 * EVERY candidate rect into physical px is wrong — `devicePixelRatio`
 * only reflects the monitor the *overlay window* lives on, so a capsule
 * dragged onto a higher-DPI secondary lands at the wrong physical
 * coordinate and `findMonitorForPointSync` either picks the primary again
 * or no monitor at all. The repro is: a 1920×1080 primary at 100 % +
 * a 3840×2160 secondary at 200 %; dropping a capsule on the secondary
 * leaves the anchor flip stuck on primary-side defaults.
 *
 * Strategy: try each monitor's own `dpi_scale` and pick the first match.
 * Falls back to the global DPR if no monitor's scaling matches (defensive
 * for callers that pre-date the multi-DPI patch).
 *
 * Rounding: a 0.5 px offset toward the rect interior nudges off-by-one
 * boundaries away from the seam between two monitors, where `Math.round`
 * could otherwise land the lookup point exactly on `r.x + r.width` and
 * fall into the neighbour.
 */
export function monitorForClientRect(
  rect: { left: number; top: number; width: number; height: number }
): MonitorInfo | null {
  const cssCenterX = rect.left + rect.width / 2;
  const cssCenterY = rect.top + rect.height / 2;

  if (cache && cache.length > 0) {
    for (const m of cache) {
      const dpr = m.dpi_scale && m.dpi_scale > 0 ? m.dpi_scale : null;
      if (dpr === null) continue;
      // Translate the candidate's CSS center into THIS monitor's
      // physical-px space and test containment. The 0.5 px nudge keeps
      // exact-boundary points on the side they came from.
      const physX = Math.floor(cssCenterX * dpr + 0.5);
      const physY = Math.floor(cssCenterY * dpr + 0.5);
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
  }

  // Fallback: global DPR for legacy / single-monitor builds.
  const dpr = window.devicePixelRatio || 1;
  const centerX = Math.floor(cssCenterX * dpr + 0.5);
  const centerY = Math.floor(cssCenterY * dpr + 0.5);
  return findMonitorForPointSync(centerX, centerY);
}
