/**
 * Theme B — populate_300_items.ts
 *
 * Runs inside the BentoDesk WebView (paste into DevTools console, or
 * import from an in-app bench panel). Creates 10 zones × 30 items so
 * the 300-item performance target can be measured end-to-end without
 * hand-seeding the UI.
 *
 * The script expects `BENTODESK_BENCH_` shortcuts on the desktop
 * (produced by `scripts/bench/stress.ps1`). If they are missing the
 * script logs a warning and exits — it will NOT fabricate fake paths
 * because the icon pipeline rejects non-existent paths by design.
 *
 * Metrics captured:
 *   • `Rust working set` — `getMemoryUsage`
 *   • `WebView2 process-group working set` — `getWebView2Memory`
 *   • `JS heap` — performance.memory.usedJSHeapSize (Chromium-only)
 *   • `Icon cache stats` — hot_hits / warm_hits / misses / hit_rate
 *
 * Output is a single JSON blob logged to the console so a caller can
 * `copy(<blob>)` and diff against baseline.
 */

import {
  addItem,
  createZone,
  getIconCacheStats,
  getMemoryUsage,
  getWebView2Memory,
  preloadIcons,
  scanDesktop,
} from "../../src/services/ipc";

const ZONE_COUNT = 10;
const ITEMS_PER_ZONE = 30;
const BENCH_PREFIX = "BENTODESK_BENCH_";

interface BenchSnapshot {
  label: string;
  timestamp: number;
  rust_working_set_mb: number;
  process_group_working_set_mb: number | null;
  js_heap_mb: number | null;
  icon_cache: Awaited<ReturnType<typeof getIconCacheStats>>;
}

async function snapshot(label: string): Promise<BenchSnapshot> {
  const rust = await getMemoryUsage();
  let pg: number | null = null;
  try {
    const wv = await getWebView2Memory();
    pg = wv.total_working_set_bytes;
  } catch {
    pg = null;
  }
  const heap =
    typeof performance !== "undefined" &&
    (performance as Performance & { memory?: { usedJSHeapSize?: number } }).memory
      ? (performance as Performance & { memory?: { usedJSHeapSize?: number } }).memory?.usedJSHeapSize ?? null
      : null;
  const stats = await getIconCacheStats();
  return {
    label,
    timestamp: Date.now(),
    rust_working_set_mb: rust.working_set_bytes / (1024 * 1024),
    process_group_working_set_mb: pg === null ? null : pg / (1024 * 1024),
    js_heap_mb: heap === null ? null : heap / (1024 * 1024),
    icon_cache: stats,
  };
}

export async function runBench(): Promise<BenchSnapshot[]> {
  const snapshots: BenchSnapshot[] = [];

  snapshots.push(await snapshot("baseline"));

  const desktop = await scanDesktop();
  const benchFiles = desktop.filter((f) => f.name.startsWith(BENCH_PREFIX));
  if (benchFiles.length < ZONE_COUNT * ITEMS_PER_ZONE) {
    console.warn(
      `[bench] expected ${ZONE_COUNT * ITEMS_PER_ZONE} stress shortcuts, found ${benchFiles.length}. Run scripts/bench/stress.ps1 first.`,
    );
  }

  for (let z = 0; z < ZONE_COUNT; z++) {
    const zone = await createZone(
      `Bench Zone ${z + 1}`,
      "lucide:folder",
      { x_percent: 5 + (z % 5) * 18, y_percent: 5 + Math.floor(z / 5) * 45 },
      { w_percent: 15, h_percent: 35 },
    );
    const slice = benchFiles.slice(z * ITEMS_PER_ZONE, (z + 1) * ITEMS_PER_ZONE);
    for (const f of slice) {
      await addItem(zone.id, f.path);
    }
    // Warm the icon cache for this zone's items.
    await preloadIcons(slice.map((f) => f.path));
  }

  snapshots.push(await snapshot("after_populate"));

  // Let the JS engine settle, then sample again.
  await new Promise((resolve) => setTimeout(resolve, 5000));
  snapshots.push(await snapshot("settled_5s"));

  console.log(JSON.stringify(snapshots, null, 2));
  return snapshots;
}

if (typeof window !== "undefined") {
  (window as unknown as { runBentoBench: typeof runBench }).runBentoBench = runBench;
}
