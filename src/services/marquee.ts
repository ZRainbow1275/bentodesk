/**
 * Marquee (drag-to-select) hit testing.
 *
 * Design notes:
 * - The marquee box is overlaid on the zone container while the user drags
 *   from an empty spot. Hit testing is AABB (`box ∩ rect`), not point-in.
 * - The rectangle index is captured **once on mousedown** so reorder /
 *   layout churn during the drag doesn't desync selection vs. what the
 *   user is visually circling. Without this cache a `querySelectorAll`
 *   on every mousemove would jank on 200+ zones.
 * - Marquee only activates when at least one zone is expanded OR a modal
 *   is open — otherwise passthrough routes the click to the desktop so
 *   BentoDesk stays zero-interference.
 */
import { createSignal, createMemo } from "solid-js";
import { isAnyModalOpen, isZoneExpanded } from "../stores/ui";
import { zonesStore } from "../stores/zones";

export interface MarqueeBox {
  /** Pointer coordinates where marquee started. */
  originX: number;
  originY: number;
  /** Current pointer coordinates. */
  currentX: number;
  currentY: number;
}

export interface RectEntry {
  id: string;
  kind: "zone" | "item";
  zoneId: string;
  rect: DOMRect;
}

const [marquee, setMarquee] = createSignal<MarqueeBox | null>(null);
const [rectIndex, setRectIndex] = createSignal<RectEntry[]>([]);

export { marquee, rectIndex };

/**
 * Derived AABB `{left, top, right, bottom}` in viewport coordinates so
 * the overlay <div> can render and hit-test consume the same box.
 */
export const marqueeRect = createMemo(() => {
  const m = marquee();
  if (!m) return null;
  const left = Math.min(m.originX, m.currentX);
  const right = Math.max(m.originX, m.currentX);
  const top = Math.min(m.originY, m.currentY);
  const bottom = Math.max(m.originY, m.currentY);
  return { left, right, top, bottom, width: right - left, height: bottom - top };
});

/**
 * Whether marquee is allowed right now. Kept as a pure function so the
 * zone container can consult it inside its `mousedown` handler.
 */
export function canActivateMarquee(): boolean {
  if (isAnyModalOpen()) return true;
  const zones = zonesStore.zones;
  return zones.some((z) => isZoneExpanded(z.id));
}

export function startMarquee(originX: number, originY: number): void {
  setMarquee({ originX, originY, currentX: originX, currentY: originY });
  setRectIndex(captureRectIndex());
}

export function updateMarquee(x: number, y: number): void {
  setMarquee((prev) => (prev ? { ...prev, currentX: x, currentY: y } : null));
}

/**
 * End the marquee and return the set of ids (items + zones) whose rect
 * intersects the final box. Caller merges this into its selection store.
 */
export function endMarquee(): {
  zoneIds: string[];
  itemIds: Array<{ zoneId: string; itemId: string }>;
} {
  const box = marqueeRect();
  const index = rectIndex();
  const zoneIds: string[] = [];
  const itemIds: Array<{ zoneId: string; itemId: string }> = [];
  if (box) {
    // If the drag is too small to be a deliberate marquee, treat as a
    // cancel — returning empty arrays so the caller just clears selection.
    const tooSmall = box.width < 4 && box.height < 4;
    if (!tooSmall) {
      for (const entry of index) {
        if (intersects(box, entry.rect)) {
          if (entry.kind === "zone") zoneIds.push(entry.id);
          else itemIds.push({ zoneId: entry.zoneId, itemId: entry.id });
        }
      }
    }
  }
  setMarquee(null);
  setRectIndex([]);
  return { zoneIds, itemIds };
}

export function cancelMarquee(): void {
  setMarquee(null);
  setRectIndex([]);
}

function intersects(
  a: { left: number; top: number; right: number; bottom: number },
  b: DOMRect
): boolean {
  return !(
    b.right < a.left ||
    b.left > a.right ||
    b.bottom < a.top ||
    b.top > a.bottom
  );
}

/**
 * Snapshot the bounding boxes of every `.bento-zone` and `.item-card` at
 * the moment marquee starts. Exported so tests can drive hit-testing
 * against a synthetic index.
 */
export function captureRectIndex(): RectEntry[] {
  if (typeof document === "undefined") return [];
  const entries: RectEntry[] = [];
  const zones = document.querySelectorAll<HTMLElement>(".bento-zone");
  zones.forEach((zoneEl) => {
    const id = zoneEl.getAttribute("data-zone-id");
    if (!id) return;
    entries.push({
      id,
      kind: "zone",
      zoneId: id,
      rect: zoneEl.getBoundingClientRect(),
    });
    const items = zoneEl.querySelectorAll<HTMLElement>(".item-card");
    items.forEach((itemEl) => {
      const itemId = itemEl.getAttribute("data-item-id");
      if (!itemId) return;
      entries.push({
        id: itemId,
        kind: "item",
        zoneId: id,
        rect: itemEl.getBoundingClientRect(),
      });
    });
  });
  return entries;
}

/** Test-only: inject a synthetic index (used by `marquee.test.ts`). */
export function __setRectIndexForTest(index: RectEntry[]): void {
  setRectIndex(index);
}

/** Test-only: inject a synthetic marquee box. */
export function __setMarqueeForTest(box: MarqueeBox | null): void {
  setMarquee(box);
}
