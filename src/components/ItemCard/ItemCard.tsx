/**
 * ItemCard — Displays a single file/folder item.
 * Standard: icon (28px) + name (11px, 2-line clamp), vertical layout.
 * Wide: horizontal layout, spans 2 columns.
 * Interactions: hover lift, active scale, double-click open, right-click context menu.
 */
import { Component } from "solid-js";
import type { BentoItem } from "../../types/zone";
import { showContextMenu, selectItem, isItemSelected } from "../../stores/ui";
import { openFile } from "../../services/ipc";
import { beginDragTracking, internalDrag } from "../../services/drag";
import ItemIcon from "./ItemIcon";
import Tooltip from "../shared/Tooltip";
import "./ItemCard.css";

/** Strip .lnk / .url extensions from display names for shortcut files. */
function displayName(name: string): string {
  return name.replace(/\.(lnk|url)$/i, "");
}

interface ItemCardProps {
  item: BentoItem;
  zoneId: string;
  index: number;
}

const ItemCard: Component<ItemCardProps> = (props) => {
  const selected = () => isItemSelected(props.zoneId, props.item.id);

  const handleDoubleClick = () => {
    void openFile(props.item.path);
  };

  const handleContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    showContextMenu(e.clientX, e.clientY, {
      type: "item",
      zoneId: props.zoneId,
      itemId: props.item.id,
    });
  };

  const handleMouseDown = (e: MouseEvent) => {
    // Only track drag on left-click
    if (e.button !== 0) return;
    selectItem(props.zoneId, props.item.id);
    beginDragTracking(
      [props.item.path],
      e.clientX,
      e.clientY,
      props.item.id,
      props.zoneId,
      props.item.name
    );
  };

  const isDragging = () => {
    const drag = internalDrag();
    return drag !== null && drag.itemId === props.item.id;
  };

  const animationDelay = () => `${props.index * 0.03}s`;

  return (
    <div
      class={`item-card item-lift item-enter ${props.item.is_wide ? "item-card--wide" : ""} ${selected() ? "item-card--selected" : ""} ${isDragging() ? "item-card--dragging" : ""}`}
      style={{
        "grid-column": props.item.is_wide ? "span 2" : undefined,
        "animation-delay": animationDelay(),
      }}
      onDblClick={handleDoubleClick}
      onContextMenu={handleContextMenu}
      onMouseDown={handleMouseDown}
      tabIndex={0}
      role="button"
      aria-label={`Open ${displayName(props.item.name)}`}
    >
      <ItemIcon
        path={props.item.path}
        iconHash={props.item.icon_hash}
        isWide={props.item.is_wide}
      />
      <Tooltip content={displayName(props.item.name)}>
        <span
          class={`item-card__name ${props.item.is_wide ? "item-card__name--wide" : ""}`}
        >
          {displayName(props.item.name)}
        </span>
      </Tooltip>
    </div>
  );
};

export default ItemCard;
