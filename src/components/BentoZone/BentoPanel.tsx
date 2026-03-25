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
import "./BentoPanel.css";

interface BentoPanelProps {
  zone: BentoZone;
  onHeaderDragStart: (e: MouseEvent) => void;
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
    collapseZone(props.zone.id);
  };

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
      <div class="bento-panel__content content-reveal content-reveal--visible">
        <ItemGrid zone={props.zone} items={filteredItems()} />
      </div>
    </div>
  );
};

export default BentoPanel;
