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
import { useTextAbbrGroup } from "../../services/textAbbrGroup";
import { t } from "../../i18n";
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
  const isMissing = () => props.item.file_missing === true;

  const visibleName = () => displayName(props.item.name);
  // v8: name uses the panel-wide FontGroup when available so a column of
  // ItemCards reads as a uniform block. Falls back to per-element sizing
  // when no FontGroupContext provider is mounted.
  const abbr = useTextAbbrGroup(visibleName);

  const handleDoubleClick = () => {
    if (isMissing()) return;
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
    if (e.button !== 0 || isMissing()) return;
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
      class={`item-card item-lift item-enter ${props.item.is_wide ? "item-card--wide" : ""} ${selected() ? "item-card--selected" : ""} ${isDragging() ? "item-card--dragging" : ""} ${isMissing() ? "item-card--missing" : ""}`}
      style={{
        "grid-column": props.item.is_wide ? "span 2" : undefined,
        "animation-delay": animationDelay(),
      }}
      onDblClick={handleDoubleClick}
      onContextMenu={handleContextMenu}
      onMouseDown={handleMouseDown}
      tabIndex={0}
      role="button"
      aria-label={
        isMissing()
          ? `${displayName(props.item.name)} ${t("itemCardMissingBadge")}`
          : `Open ${displayName(props.item.name)}`
      }
      aria-disabled={isMissing() ? "true" : undefined}
    >
      <ItemIcon
        path={props.item.path}
        iconHash={props.item.icon_hash}
        isWide={props.item.is_wide}
      />
      <div class={`item-card__meta ${props.item.is_wide ? "item-card__meta--wide" : ""}`}>
        <Tooltip
          content={visibleName()}
          disabled={abbr.tooltipDisabled()}
        >
          <span
            class={`item-card__name ${props.item.is_wide ? "item-card__name--wide" : ""}`}
            ref={abbr.setRef}
            aria-label={visibleName()}
            style={{ "font-size": `${abbr.fontSize()}px` }}
          >
            {abbr.text()}
          </span>
        </Tooltip>
        <Tooltip content={t("itemCardMissingTooltip")}>
          <span
            class={`item-card__missing-badge ${isMissing() ? "item-card__missing-badge--visible" : ""}`}
            aria-hidden={isMissing() ? undefined : "true"}
          >
            {t("itemCardMissingBadge")}
          </span>
        </Tooltip>
      </div>
    </div>
  );
};

export default ItemCard;
