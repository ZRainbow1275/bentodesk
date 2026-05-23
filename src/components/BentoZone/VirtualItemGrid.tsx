/**
 * VirtualItemGrid — windowed variant of ItemGrid for large zones.
 *
 * Theme B — only used when `items.length > VIRTUAL_THRESHOLD` (50).
 * Small zones use the plain `ItemGrid` because the virtualizer's
 * per-row bookkeeping costs more than rendering a handful of cards.
 *
 * Layout model: we virtualise rows (vertical only). Each row has
 * `gridColumns` lanes rendered via CSS grid so spacing and gap match
 * the non-virtual path pixel-for-pixel.
 */
import { Component, For, createMemo, onCleanup, onMount } from "solid-js";
import { createVirtualizer } from "@tanstack/solid-virtual";
import type { BentoZone, BentoItem } from "../../types/zone";
import ItemCard from "../ItemCard/ItemCard";
import "./ItemGrid.css";

interface VirtualItemGridProps {
  zone: BentoZone;
  items: BentoItem[];
}

/** Rough card height + gap — tuned against ItemCard.css. */
const ROW_HEIGHT = 80;
/** Rows to render above/below the viewport for smooth fast-scroll. */
const OVERSCAN_ROWS = 3;

const VirtualItemGrid: Component<VirtualItemGridProps> = (props) => {
  let scrollEl: HTMLDivElement | undefined;

  const gridColumns = () => props.zone.grid_columns || 4;

  const rowCount = createMemo(() => {
    const cols = gridColumns();
    return Math.ceil(props.items.length / Math.max(1, cols));
  });

  const rowVirtualizer = createVirtualizer({
    get count() {
      return rowCount();
    },
    getScrollElement: () => scrollEl ?? null,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN_ROWS,
  });

  // Resize-aware re-measurement: when the panel grows (resize handle)
  // the row height might change if the card CSS breaks to a taller
  // layout at different breakpoints. Measure once on mount and on
  // window resize.
  let resizeObserver: ResizeObserver | null = null;
  onMount(() => {
    if (!scrollEl || typeof ResizeObserver === "undefined") return;
    resizeObserver = new ResizeObserver(() => {
      rowVirtualizer.measure();
    });
    resizeObserver.observe(scrollEl);
  });
  onCleanup(() => {
    resizeObserver?.disconnect();
    resizeObserver = null;
  });

  return (
    <div ref={scrollEl} class="item-grid item-grid--virtual">
      <div
        class="item-grid__virtual-spacer"
        style={{ "--virtual-grid-height": `${rowVirtualizer.getTotalSize()}px` }}
      >
        <For each={rowVirtualizer.getVirtualItems()}>
          {(virtualRow) => {
            const cols = gridColumns();
            const rowStart = virtualRow.index * cols;
            const rowItems = props.items.slice(rowStart, rowStart + cols);
            return (
              <div
                class="item-grid__virtual-row"
                style={{
                  "--virtual-row-y": `${virtualRow.start}px`,
                  "--virtual-grid-columns": `repeat(${cols}, 1fr)`,
                }}
              >
                <For each={rowItems}>
                  {(item, colIdx) => (
                    <ItemCard
                      item={item}
                      zoneId={props.zone.id}
                      index={rowStart + colIdx()}
                    />
                  )}
                </For>
              </div>
            );
          }}
        </For>
      </div>
    </div>
  );
};

export default VirtualItemGrid;
