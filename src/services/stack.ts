/**
 * Stack detection service — D2 Zone Stack Mode.
 *
 * Core responsibility: after a zone drag settles, scan pairs of zones on the
 * same monitor for substantial rect overlap. Overlap is measured as
 * `intersection / min(areaA, areaB)` (NOT IoU) so a small zone fully swallowed
 * by a much larger one still triggers at ~100% rather than getting diluted by
 * the big zone's extra area.
 *
 * Threshold: 60% by default, adjustable via the settings slider (20–80%).
 *
 * Multi-monitor safety: zones on different physical monitors are never grouped
 * even when their CSS rects appear to overlap — DPI scaling can produce
 * pseudo-overlaps across disjoint displays.
 */
import type { BentoZone } from "../types/zone";
import { getViewportSize } from "../stores/ui";
import { monitorForClientRect } from "./geometry";

export interface ZoneRect {
  id: string;
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface OverlapResult {
  zoneA: string;
  zoneB: string;
  /** `intersection / min(areaA, areaB)` — range 0..1. */
  ratio: number;
}

/** Convert a `BentoZone`'s percentage position/size to client-space pixels. */
export function zoneToClientRect(zone: BentoZone): ZoneRect {
  const vp = getViewportSize();
  const w = (zone.expanded_size.w_percent > 0 ? zone.expanded_size.w_percent : 25) / 100;
  const h = (zone.expanded_size.h_percent > 0 ? zone.expanded_size.h_percent : 25) / 100;
  return {
    id: zone.id,
    left: (zone.position.x_percent / 100) * vp.width,
    top: (zone.position.y_percent / 100) * vp.height,
    width: w * vp.width,
    height: h * vp.height,
  };
}

/**
 * Compute overlap ratio between two rects, using the "swallowed by the smaller
 * rect" definition: `intersection_area / min(areaA, areaB)`. Returns 0 when
 * rects are disjoint.
 */
export function detectOverlap(a: ZoneRect, b: ZoneRect): number {
  const xLeft = Math.max(a.left, b.left);
  const yTop = Math.max(a.top, b.top);
  const xRight = Math.min(a.left + a.width, b.left + b.width);
  const yBottom = Math.min(a.top + a.height, b.top + b.height);
  const iw = xRight - xLeft;
  const ih = yBottom - yTop;
  if (iw <= 0 || ih <= 0) return 0;
  const inter = iw * ih;
  const areaA = a.width * a.height;
  const areaB = b.width * b.height;
  const minArea = Math.max(1, Math.min(areaA, areaB));
  return Math.min(1, inter / minArea);
}

/** True if both rects sit on the same monitor (multi-monitor DPI safety). */
export function onSameMonitor(a: ZoneRect, b: ZoneRect): boolean {
  const ma = monitorForClientRect(a);
  const mb = monitorForClientRect(b);
  if (!ma || !mb) return true; // cache empty — don't reject
  // MonitorInfo equality via rect_full coordinates (stable for a session).
  return (
    ma.rect_full.x === mb.rect_full.x &&
    ma.rect_full.y === mb.rect_full.y &&
    ma.rect_full.width === mb.rect_full.width &&
    ma.rect_full.height === mb.rect_full.height
  );
}

/**
 * Group ids into connected components by overlap using union-find. Any pair
 * passing `threshold` (default 0.6) and `onSameMonitor` gets unioned.
 *
 * Returns an array of clusters of size >= 2. Free-standing zones are omitted.
 */
export function suggestStack(
  zones: BentoZone[],
  threshold = 0.6,
): string[][] {
  const rects = zones.map(zoneToClientRect);
  const n = rects.length;
  if (n < 2) return [];
  const parent = Array.from({ length: n }, (_, i) => i);
  const find = (i: number): number => {
    let r = i;
    while (parent[r] !== r) r = parent[r];
    let cur = i;
    while (parent[cur] !== r) {
      const next = parent[cur];
      parent[cur] = r;
      cur = next;
    }
    return r;
  };
  const union = (a: number, b: number): void => {
    const ra = find(a);
    const rb = find(b);
    if (ra !== rb) parent[rb] = ra;
  };

  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      if (!onSameMonitor(rects[i], rects[j])) continue;
      const ratio = detectOverlap(rects[i], rects[j]);
      if (ratio >= threshold) union(i, j);
    }
  }

  const groups = new Map<number, string[]>();
  for (let i = 0; i < n; i++) {
    const root = find(i);
    const bucket = groups.get(root);
    if (bucket) bucket.push(rects[i].id);
    else groups.set(root, [rects[i].id]);
  }
  return Array.from(groups.values()).filter((g) => g.length >= 2);
}
