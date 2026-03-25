/**
 * ItemGrid — CSS Grid container for ItemCard components.
 * 4-column grid with 12px gap.
 * Renders a ghost card placeholder during internal reorder drags.
 * Listens for reorder-commit events to persist new item order.
 */
import { Component, For, Show, createMemo, onMount, onCleanup } from "solid-js";
import type { BentoZone, BentoItem } from "../../types/zone";
import { internalDrag } from "../../services/drag";
import { reorderItems } from "../../stores/zones";
import { t } from "../../i18n";
import ItemCard from "../ItemCard/ItemCard";
import "./ItemGrid.css";

interface ItemGridProps {
  zone: BentoZone;
  items: BentoItem[];
}

const ItemGrid: Component<ItemGridProps> = (props) => {
  const gridColumns = () => props.zone.grid_columns || 4;

  // Is this zone the current target of an internal reorder drag?
  const dragTargetingThisZone = () => {
    const drag = internalDrag();
    return drag !== null && drag.targetZoneId === props.zone.id;
  };

  // Compute displayed items with ghost card insertion
  const displayItems = createMemo(() => {
    const drag = internalDrag();
    const items = props.items;

    if (!drag || drag.targetZoneId !== props.zone.id) {
      // No active drag targeting this zone — return items as-is
      return items.map((item) => ({ type: "item" as const, item }));
    }

    // Build display list: filter out the dragged item, insert ghost at target index
    const filtered = items.filter((item) => item.id !== drag.itemId);
    const result: Array<
      | { type: "item"; item: BentoItem }
      | { type: "ghost"; key: string }
    > = filtered.map((item) => ({ type: "item" as const, item }));

    // Clamp target index to valid range
    const insertAt = Math.min(drag.targetIndex, result.length);
    result.splice(insertAt, 0, { type: "ghost", key: "drag-ghost" });

    return result;
  });

  // Listen for reorder-commit custom events from drag.ts
  const handleReorderCommit = (e: Event) => {
    const detail = (e as CustomEvent).detail as {
      zoneId: string;
      itemId: string;
      targetIndex: number;
    };
    if (detail.zoneId !== props.zone.id) return;

    // Build the new order: remove item from current position, insert at target
    const currentIds = props.items.map((item) => item.id);
    const filtered = currentIds.filter((id) => id !== detail.itemId);
    const insertAt = Math.min(detail.targetIndex, filtered.length);
    filtered.splice(insertAt, 0, detail.itemId);

    void reorderItems(props.zone.id, filtered);
  };

  onMount(() => {
    document.addEventListener("bentodesk:reorder-commit", handleReorderCommit);
  });

  onCleanup(() => {
    document.removeEventListener(
      "bentodesk:reorder-commit",
      handleReorderCommit
    );
  });

  return (
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
  );
};

export default ItemGrid;
