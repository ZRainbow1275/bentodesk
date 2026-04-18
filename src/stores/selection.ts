/**
 * Multi-selection store for zones + items.
 *
 * Supersedes the single-item `selectedItem` signal in `ui.ts` (which is
 * kept for backwards compatibility with keyboard navigation + the existing
 * ItemCard `isItemSelected` consumers). Selection here is a Set of ids.
 *
 * Selection helpers support the four gestures from spec C2:
 * - click → replace selection
 * - Shift+click → range (last anchor → target) for items within same zone
 * - Ctrl+click → additive toggle
 * - marquee → union with hit-tested ids
 */
import { createSignal, createMemo } from "solid-js";

const [selectedZoneIds, setSelectedZoneIds] = createSignal<Set<string>>(new Set());
const [selectedItemIds, setSelectedItemIds] = createSignal<Set<string>>(new Set());

/** Last anchor per-zone → enables shift-click range. */
const itemAnchorByZone = new Map<string, string>();
let lastZoneAnchor: string | null = null;

export { selectedZoneIds, selectedItemIds };

export const selectedZoneCount = createMemo(() => selectedZoneIds().size);
export const selectedItemCount = createMemo(() => selectedItemIds().size);

export function isZoneMultiSelected(zoneId: string): boolean {
  return selectedZoneIds().has(zoneId);
}

export function isItemMultiSelected(itemId: string): boolean {
  return selectedItemIds().has(itemId);
}

export function clearMultiSelection(): void {
  setSelectedZoneIds(new Set<string>());
  setSelectedItemIds(new Set<string>());
  itemAnchorByZone.clear();
  lastZoneAnchor = null;
}

export function setZoneSelection(ids: Iterable<string>): void {
  const next = new Set<string>(ids);
  setSelectedZoneIds(next);
}

export function setItemSelection(ids: Iterable<string>): void {
  const next = new Set<string>(ids);
  setSelectedItemIds(next);
}

/**
 * Toggle a zone id in the selection. If `shift` is true, this sets the
 * selection to the contiguous range between the last anchor and `zoneId`
 * in the caller-provided ordering. `replace=true` clears first (plain
 * click behaviour).
 */
export function selectZone(
  zoneId: string,
  opts: {
    shift?: boolean;
    ctrl?: boolean;
    orderedIds?: string[];
  } = {}
): void {
  const { shift, ctrl, orderedIds } = opts;
  if (shift && lastZoneAnchor && orderedIds) {
    const a = orderedIds.indexOf(lastZoneAnchor);
    const b = orderedIds.indexOf(zoneId);
    if (a >= 0 && b >= 0) {
      const [from, to] = a <= b ? [a, b] : [b, a];
      setSelectedZoneIds(new Set(orderedIds.slice(from, to + 1)));
      return;
    }
  }
  if (ctrl) {
    setSelectedZoneIds((prev) => {
      const next = new Set(prev);
      if (next.has(zoneId)) next.delete(zoneId);
      else next.add(zoneId);
      return next;
    });
    lastZoneAnchor = zoneId;
    return;
  }
  setSelectedZoneIds(new Set([zoneId]));
  lastZoneAnchor = zoneId;
}

export function selectItem(
  zoneId: string,
  itemId: string,
  opts: {
    shift?: boolean;
    ctrl?: boolean;
    orderedItemIds?: string[];
  } = {}
): void {
  const { shift, ctrl, orderedItemIds } = opts;
  const anchor = itemAnchorByZone.get(zoneId);
  if (shift && anchor && orderedItemIds) {
    const a = orderedItemIds.indexOf(anchor);
    const b = orderedItemIds.indexOf(itemId);
    if (a >= 0 && b >= 0) {
      const [from, to] = a <= b ? [a, b] : [b, a];
      setSelectedItemIds(new Set(orderedItemIds.slice(from, to + 1)));
      return;
    }
  }
  if (ctrl) {
    setSelectedItemIds((prev) => {
      const next = new Set(prev);
      if (next.has(itemId)) next.delete(itemId);
      else next.add(itemId);
      return next;
    });
    itemAnchorByZone.set(zoneId, itemId);
    return;
  }
  setSelectedItemIds(new Set([itemId]));
  itemAnchorByZone.set(zoneId, itemId);
}

/**
 * Merge marquee hits into the current selection. Behaves additively so
 * shift+marquee keeps the prior selection live.
 */
export function unionMarqueeSelection(
  zoneIds: Iterable<string>,
  itemIds: Iterable<string>
): void {
  setSelectedZoneIds((prev) => {
    const next = new Set(prev);
    for (const id of zoneIds) next.add(id);
    return next;
  });
  setSelectedItemIds((prev) => {
    const next = new Set(prev);
    for (const id of itemIds) next.add(id);
    return next;
  });
}

/** Replace selection from marquee; used when user drags on empty space without a modifier. */
export function replaceWithMarqueeSelection(
  zoneIds: Iterable<string>,
  itemIds: Iterable<string>
): void {
  setSelectedZoneIds(new Set<string>(zoneIds));
  setSelectedItemIds(new Set<string>(itemIds));
}
