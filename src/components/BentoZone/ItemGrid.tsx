/**
 * ItemGrid — CSS Grid container for ItemCard components.
 * 4-column grid with 12px gap.
 * Renders a ghost card placeholder during internal reorder drags.
 * Listens for reorder-commit events to persist new item order.
 *
 * Theme B — when `items.length > VIRTUAL_THRESHOLD` the grid delegates
 * to VirtualItemGrid (windowed rendering). The threshold is deliberately
 * high (50) because the virtualizer's bookkeeping costs more than a plain
 * For for small zones. Virtualisation is also suspended while a cross-
 * zone reorder drag targets this zone — the ghost-card insertion relies
 * on the full item list being in the DOM.
 */
import { Component, For, Show, createMemo } from "solid-js";
import type { BentoZone, BentoItem } from "../../types/zone";
import { internalDrag } from "../../services/drag";
import { t } from "../../i18n";
import ItemCard from "../ItemCard/ItemCard";
import VirtualItemGrid from "./VirtualItemGrid";
import "./ItemGrid.css";

interface ItemGridProps {
  zone: BentoZone;
  items: BentoItem[];
}

const VIRTUAL_THRESHOLD = 50;

const ItemGrid: Component<ItemGridProps> = (props) => {
  const gridColumns = () => props.zone.grid_columns || 4;

  const dragTargetingThisZone = () => {
    const drag = internalDrag();
    return drag !== null && drag.targetZoneId === props.zone.id;
  };

  const shouldVirtualize = () =>
    props.items.length > VIRTUAL_THRESHOLD && !dragTargetingThisZone();

  const displayItems = createMemo(() => {
    const drag = internalDrag();
    const items = props.items;

    if (!drag || drag.targetZoneId !== props.zone.id) {
      return items.map((item) => ({ type: "item" as const, item }));
    }

    const filtered = items.filter((item) => item.id !== drag.itemId);
    const result: Array<
      | { type: "item"; item: BentoItem }
      | { type: "ghost"; key: string }
    > = filtered.map((item) => ({ type: "item" as const, item }));

    const insertAt = Math.min(drag.targetIndex, result.length);
    result.splice(insertAt, 0, { type: "ghost", key: "drag-ghost" });

    return result;
  });

  return (
    <Show
      when={!shouldVirtualize()}
      fallback={<VirtualItemGrid zone={props.zone} items={props.items} />}
    >
      <div
        class="item-grid"
        style={{
          "grid-template-columns": `repeat(${gridColumns()}, 1fr)`,
        }}
      >
        <Show
          when={props.items.length > 0 || dragTargetingThisZone()}
          fallback={
            <div class="item-grid__empty">
              <span class="item-grid__empty-text">{t("itemGridEmptyDropHere")}</span>
            </div>
          }
        >
          <For each={displayItems()}>
            {(entry, index) => (
              <Show
                when={entry.type === "item"}
                fallback={
                  <div class="bento-zone__ghost-card" />
                }
              >
                {(() => {
                  const itemEntry = entry as { type: "item"; item: BentoItem };
                  return (
                    <ItemCard
                      item={itemEntry.item}
                      zoneId={props.zone.id}
                      index={index()}
                    />
                  );
                })()}
              </Show>
            )}
          </For>
        </Show>
      </div>
    </Show>
  );
};

export default ItemGrid;
