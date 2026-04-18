/**
 * Auto-layout algorithms for zone arrangement.
 *
 * All algorithms operate in the 0..100 relative-percent space used by
 * `RelativePosition` / `RelativeSize`, so results survive resolution /
 * DPI changes. A single `applyLayout` entrypoint wires the algorithm
 * output to a `bulk_update_zones` IPC for atomic persistence + one
 * timeline checkpoint.
 *
 * Parameter choices (Organic Verlet): `k_r=8000`, `k_e=0.05`, `dt=0.16`,
 * 150 iterations. These were validated against 10–30 zone PoCs and
 * converge in <20 ms in the browser.
 */
import type { LayoutAlgorithm, BulkZoneUpdate } from "./ipc";
import { bulkUpdateZones } from "./ipc";
import type { BentoZone } from "../types/zone";

export type { LayoutAlgorithm } from "./ipc";

interface Point {
  x: number;
  y: number;
}

interface LayoutOpts {
  margin?: number;
  viewportAspect?: number;
}

/**
 * Compute target positions for `n` zones under the given algorithm.
 * Returned as `RelativePosition` style tuples in the 0..100 percent space.
 */
export function computeLayout(
  algo: LayoutAlgorithm,
  zones: BentoZone[],
  opts: LayoutOpts = {}
): Point[] {
  const n = zones.length;
  if (n === 0) return [];
  const margin = opts.margin ?? 5;
  const usable = 100 - margin * 2;
  switch (algo) {
    case "grid":
      return grid(n, margin, usable);
    case "row":
      return row(n, margin, usable);
    case "column":
      return column(n, margin, usable);
    case "spiral":
      return spiral(n);
    case "organic":
      return organic(n, margin);
  }
}

function grid(n: number, margin: number, usable: number): Point[] {
  const cols = Math.ceil(Math.sqrt(n));
  const rows = Math.ceil(n / cols);
  const cellW = usable / cols;
  const cellH = usable / rows;
  return Array.from({ length: n }, (_, i) => ({
    x: margin + (i % cols) * cellW,
    y: margin + Math.floor(i / cols) * cellH,
  }));
}

function row(n: number, margin: number, usable: number): Point[] {
  const cellW = usable / n;
  return Array.from({ length: n }, (_, i) => ({
    x: margin + i * cellW,
    y: margin,
  }));
}

function column(n: number, margin: number, usable: number): Point[] {
  const cellH = usable / n;
  return Array.from({ length: n }, (_, i) => ({
    x: margin,
    y: margin + i * cellH,
  }));
}

function spiral(n: number): Point[] {
  const cx = 50;
  const cy = 50;
  const a = 5;
  // Archimedean spiral: r = a + b*θ. `b` tuned so consecutive points are
  // roughly evenly spaced without leaving the viewport.
  const b = 4 / (2 * Math.PI);
  const step = 4;
  let theta = 0;
  const out: Point[] = [];
  for (let i = 0; i < n; i++) {
    const r = a + b * theta;
    out.push({ x: cx + r * Math.cos(theta), y: cy + r * Math.sin(theta) });
    theta += step / Math.max(r, a);
  }
  return out;
}

/**
 * Verlet-physics organic packing. Repulsion keeps zones apart, a weak
 * edge-pull attractor keeps them centred, and damping (0.85) prevents
 * oscillation.
 */
function organic(n: number, margin: number): Point[] {
  const pos: Point[] = Array.from({ length: n }, (_, i) => {
    const golden = (137.508 * Math.PI) / 180;
    const theta = i * golden;
    const r = 2 * Math.sqrt(i);
    return { x: 50 + r * Math.cos(theta), y: 50 + r * Math.sin(theta) };
  });
  const prev = pos.map((p) => ({ ...p }));
  const kr = 8000;
  const ke = 0.05;
  const dt = 0.16;
  const iterations = 150;
  const min = margin;
  const max = 100 - margin;
  for (let step = 0; step < iterations; step++) {
    for (let i = 0; i < n; i++) {
      let fx = 0;
      let fy = 0;
      for (let j = 0; j < n; j++) {
        if (i === j) continue;
        const dx = pos[i].x - pos[j].x;
        const dy = pos[i].y - pos[j].y;
        const d2 = dx * dx + dy * dy + 1;
        const scale = kr / (d2 * Math.sqrt(d2));
        fx += dx * scale;
        fy += dy * scale;
      }
      // Soft centring force — keeps blobs near viewport centre instead of
      // blowing outward past the clamp.
      fx += (50 - pos[i].x) * ke;
      fy += (50 - pos[i].y) * ke;
      const vx = (pos[i].x - prev[i].x) * 0.85;
      const vy = (pos[i].y - prev[i].y) * 0.85;
      prev[i].x = pos[i].x;
      prev[i].y = pos[i].y;
      pos[i].x = clamp(pos[i].x + vx + fx * dt * dt, min, max);
      pos[i].y = clamp(pos[i].y + vy + fy * dt * dt, min, max);
    }
  }
  return pos;
}

function clamp(n: number, min: number, max: number): number {
  return n < min ? min : n > max ? max : n;
}

/**
 * Apply a layout to the given zones via the bulk IPC so all updates land
 * under a single Rust lock and a single timeline checkpoint.
 */
export async function applyLayout(
  algo: LayoutAlgorithm,
  zones: BentoZone[]
): Promise<void> {
  const pts = computeLayout(algo, zones);
  const updates: BulkZoneUpdate[] = zones.map((z, i) => ({
    id: z.id,
    position: { x_percent: pts[i].x, y_percent: pts[i].y },
  }));
  await bulkUpdateZones(updates);
}
