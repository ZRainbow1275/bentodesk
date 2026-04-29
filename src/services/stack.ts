/**
 * Stack detection service — D2 Zone Stack Mode.
 *
 * Core responsibility: after a zone drag settles, scan pairs of zones on the
 * same monitor for substantial rect overlap. Overlap is measured as
 * `intersection / min(areaA, areaB)` (NOT IoU) so a small zone fully swallowed
 * by a much larger one still triggers at ~100% rather than getting diluted by
 * the big zone's extra area.
 *
 * Threshold: 30% by default — anything more eager misses partial overlaps the
 * user perceives as "stuck behind"; anything less eager swallows freshly
 * adjacent zones the user wanted side-by-side. Adjustable via settings slider
 * (20–80%).
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

/**
 * Convert a `BentoZone`'s percentage position/size to client-space pixels.
 *
 * Optional `vpOverride` lets callers (e.g. `computeAutoSpread`) inject a
 * viewport that has already been hardened against the {0,0} pre-mount
 * state of the ui store. Without it the rect collapses to all-zeroes
 * during the first render, making cluster spread land every zone at
 * (0%, ~0.8%, ~1.6%) — visually still stacked.
 */
export function zoneToClientRect(
  zone: BentoZone,
  vpOverride?: { width: number; height: number },
): ZoneRect {
  let vp = vpOverride ?? getViewportSize();
  if (vp.width <= 0 || vp.height <= 0) {
    if (typeof window !== "undefined") {
      vp = { width: window.innerWidth || 1920, height: window.innerHeight || 1080 };
    } else {
      vp = { width: 1920, height: 1080 };
    }
  }
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
  threshold = 0.3,
): string[][] {
  // Wrap in arrow so Array.map's index arg never bleeds into the optional
  // vpOverride parameter (TS would otherwise complain about number→object).
  const rects = zones.map((z) => zoneToClientRect(z));
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

// ─── Auto-spread layout ─────────────────────────────────────

export interface SpreadEntry {
  id: string;
  x_percent: number;
  y_percent: number;
}

/**
 * Width (px) reserved on the LEFT and RIGHT of the viewport for the desktop
 * icon column when auto-spreading zones. v5 Fix #2: the prior algorithm let
 * `cursor = centerX - totalWidth/2` clamp to 0 whenever the cluster centroid
 * was already near the left edge, which placed the leftmost zone at left=0
 * and stomped the user's Win11 desktop icon column. Reserving 256 px on each
 * side (room for 1–2 standard 96 px icon cells with their text labels) keeps
 * those icons clickable after a spread.
 */
const DESKTOP_ICON_GUTTER_PX = 256;

/**
 * Spread a cluster of overlapping zones horizontally around the centroid of
 * the original group. Members are ordered by `sort_order` (ascending) so the
 * visual ordering after spread is deterministic. Spacing equals each zone's
 * own width plus a 16px gutter so the result is gap-free even with mixed
 * sizes. Coordinates are returned as viewport percentages so the caller can
 * commit through `bulkUpdateZones` without further conversion.
 *
 * `clusterIds` MUST share a monitor (caller's responsibility); the centroid
 * is computed in viewport pixels. The first member's y-coordinate is reused
 * for every spread member — keeping the row aligned matches the user's
 * "Auto-spread along the same shelf" mental model from PRD Q-stack.
 *
 * v5 Fix #2: spread is constrained to the viewport's middle band
 * `[DESKTOP_ICON_GUTTER_PX, vp.width - DESKTOP_ICON_GUTTER_PX]` so the user's
 * desktop icon columns remain accessible. If the cluster's intrinsic
 * `totalWidth` exceeds that band we still place zones inside it (clamped flush
 * left of the gutter) — overlap is preferred over hiding desktop icons.
 */
export function computeAutoSpread(
  zones: BentoZone[],
  clusterIds: string[],
  gutterPx = 16,
): SpreadEntry[] {
  const memberZones = clusterIds
    .map((id) => zones.find((z) => z.id === id))
    .filter((z): z is BentoZone => z !== undefined)
    .sort((a, b) => a.sort_order - b.sort_order);
  if (memberZones.length < 2) return [];

  // Resilient viewport read: ui store is the canonical source but on first
  // render (before installViewportTracker fires) it can return {0, 0}. Fall
  // back to window dimensions, then to a safe 1920x1080 default so spread
  // still produces sane percentages instead of 0% / NaN%.
  let vp = getViewportSize();
  if (vp.width <= 0 || vp.height <= 0) {
    if (typeof window !== "undefined") {
      vp = { width: window.innerWidth || 1920, height: window.innerHeight || 1080 };
    } else {
      vp = { width: 1920, height: 1080 };
    }
  }

  // Pass the hardened vp into zoneToClientRect so each rect is computed
  // against the same fallback. Otherwise zoneToClientRect re-reads the
  // ui-store viewport (which may still be {0,0}) and every rect collapses
  // to zero, defeating the spread math entirely.
  const rects = memberZones.map((z) => zoneToClientRect(z, vp));
  const centerX =
    rects.reduce((sum, r) => sum + r.left + r.width / 2, 0) / rects.length;
  // Anchor every spread member to the cluster's average top so the row is
  // visually flush; using min keeps them on-screen even if some zones were
  // dragged downward into the overlap.
  const topY = Math.min(...rects.map((r) => r.top));

  const totalWidth =
    rects.reduce((sum, r) => sum + r.width, 0) +
    gutterPx * (rects.length - 1);
  // v5 Fix #2: usable region excludes left + right desktop icon gutters so
  // spread never overlays the user's Win11 icon column. When the gutter
  // would leave less than `totalWidth` of usable width we honour the gutter
  // floor on the left and accept the rightmost zone hugging the right
  // gutter edge — preferred over re-overlaying icons just to fit.
  const leftBound = DESKTOP_ICON_GUTTER_PX;
  const rightBound = Math.max(leftBound, vp.width - DESKTOP_ICON_GUTTER_PX);
  let cursor = centerX - totalWidth / 2;
  // Clamp the start so the leftmost zone never lands inside the desktop
  // icon gutter on the left.
  cursor = Math.max(leftBound, cursor);
  // Clamp the end so the rightmost zone fits inside the right-side desktop
  // icon gutter; if even that's not enough the cluster must accept overlap
  // with the left gutter floor (totalWidth > usableWidth case).
  if (cursor + totalWidth > rightBound) {
    cursor = Math.max(leftBound, rightBound - totalWidth);
  }
  const yPercent = Math.max(0, (topY / vp.height) * 100);

  const out: SpreadEntry[] = [];
  for (let i = 0; i < memberZones.length; i++) {
    const rect = rects[i];
    const xPercent = (cursor / vp.width) * 100;
    out.push({
      id: memberZones[i].id,
      x_percent: xPercent,
      y_percent: yPercent,
    });
    cursor += rect.width + gutterPx;
  }
  return out;
}
