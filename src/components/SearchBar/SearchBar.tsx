/**
 * SearchBar — Inline search in zone header.
 * Filters items in real-time (client-side).
 * Escape clears and closes, Enter opens first match.
 */
import { Component, Show, onMount } from "solid-js";
import {
  getSearchQuery,
  setSearchQuery,
  closeSearch,
} from "../../stores/ui";
import { zonesStore } from "../../stores/zones";
import { openFile } from "../../services/ipc";
import { t } from "../../i18n";
import "./SearchBar.css";

interface SearchBarProps {
  zoneId: string;
}

const SearchBar: Component<SearchBarProps> = (props) => {
  let inputRef: HTMLInputElement | undefined;

  onMount(() => {
    inputRef?.focus();
  });

  const handleInput = (e: InputEvent) => {
    const target = e.currentTarget as HTMLInputElement;
    setSearchQuery(target.value);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      closeSearch();
    } else if (e.key === "Enter") {
      e.preventDefault();
      // Open first matching item
      const query = getSearchQuery().toLowerCase().trim();
      if (!query) return;

      const zone = zonesStore.zones.find((z) => z.id === props.zoneId);
      if (!zone) return;

      const match = zone.items.find((item) =>
        item.name.toLowerCase().includes(query)
      );
      if (match) {
        void openFile(match.path);
      }
    }
  };

  const handleClear = () => {
    setSearchQuery("");
    inputRef?.focus();
  };

  return (
    <div class="search-bar">
      <svg
        class="search-bar__icon"
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <circle cx="11" cy="11" r="8" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
      <input
        ref={inputRef}
        class="search-bar__input"
        type="text"
        placeholder={t("searchBarPlaceholder")}
        value={getSearchQuery()}
        onInput={handleInput}
        onKeyDown={handleKeyDown}
      />
      <Show when={getSearchQuery().length > 0}>
        <button
          class="search-bar__clear"
          onClick={handleClear}
          aria-label={t("searchBarClearAriaLabel")}
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </Show>
    </div>
  );
};

export default SearchBar;
