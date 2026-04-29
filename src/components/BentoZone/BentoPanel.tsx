/**
 * BentoPanel — Expanded state of a BentoZone.
 * Contains: header, optional search bar, item grid.
 */
import { Component, Show, createMemo } from "solid-js";
import type { BentoZone } from "../../types/zone";
import {
  isSearchActive,
  getSearchQuery,
  collapseZone,
} from "../../stores/ui";
import PanelHeader from "./PanelHeader";
import SearchBar from "../SearchBar/SearchBar";
import ItemGrid from "./ItemGrid";
import {
  FontGroupContext,
  createFontGroup,
} from "../../services/textAbbrGroup";
import "./BentoPanel.css";

/** ItemCard's CSS-declared default name size — see ItemCard.css. */
const ITEM_CARD_DEFAULT_FONT_PX = 11;

interface BentoPanelProps {
  zone: BentoZone;
  onHeaderDragStart: (e: MouseEvent) => void;
  onClose?: () => void;
}

const BentoPanel: Component<BentoPanelProps> = (props) => {
  const searchActive = () => isSearchActive(props.zone.id);
  const query = () => (searchActive() ? getSearchQuery() : "");

  const filteredItems = createMemo(() => {
    const q = query().toLowerCase().trim();
    if (!q) return props.zone.items;
    return props.zone.items.filter((item) =>
      item.name.toLowerCase().includes(q)
    );
  });

  const handleClose = () => {
    if (props.onClose) {
      props.onClose();
      return;
    }
    collapseZone(props.zone.id);
  };

  // v8 font-uniformity: every ItemCard inside this panel registers with the
  // same group, so the rendered name size is min(needed) — preventing the
  // ragged column where each name shrunk independently. The group is created
  // once per panel instance; SolidJS owners hold it for the panel's lifetime.
  // PanelHeader deliberately uses plain useTextAbbr (it is standalone, not
  // part of the column), so we wrap only the content area.
  const fontGroup = createFontGroup(ITEM_CARD_DEFAULT_FONT_PX);

  return (
    <div class="bento-panel">
      <PanelHeader
        zone={props.zone}
        onDragStart={props.onHeaderDragStart}
        onClose={handleClose}
      />
      <Show when={searchActive()}>
        <SearchBar zoneId={props.zone.id} />
      </Show>
      <FontGroupContext.Provider value={fontGroup}>
        <div class="bento-panel__content content-reveal content-reveal--visible">
          <ItemGrid zone={props.zone} items={filteredItems()} />
        </div>
      </FontGroupContext.Provider>
    </div>
  );
};

export default BentoPanel;
