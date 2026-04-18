/**
 * BulkManagerPanel — Theme C bulk operations UI.
 *
 * Opens via `Ctrl+Shift+M` or ContextMenu "Manage all zones". Shows a
 * sortable table of every zone with checkbox selection, inline summary,
 * and a toolbar for palette / auto-layout / delete operations. All
 * mutations funnel through `bulk_update_zones` so Ctrl+Z restores the
 * entire bulk change in one step.
 */
import {
  Component,
  Show,
  For,
  createMemo,
  createSignal,
  onMount,
  onCleanup,
} from "solid-js";
import { isBulkManagerOpen, closeBulkManager } from "../../stores/ui";
import { zonesStore } from "../../stores/zones";
import {
  bulkUpdateZones,
  bulkDeleteZones,
  type BulkZoneUpdate,
} from "../../services/ipc";
import { applyLayout, type LayoutAlgorithm } from "../../services/autoLayout";
import PalettePicker from "./PalettePicker";
import AutoLayoutMenu from "./AutoLayoutMenu";
import { t } from "../../i18n";
import "./BulkManagerPanel.css";

type SortKey = "name" | "items" | "accent" | "size";

const BulkManagerPanel: Component = () => {
  const [selected, setSelected] = createSignal<Set<string>>(new Set<string>());
  const [sortKey, setSortKey] = createSignal<SortKey>("name");
  const [sortAsc, setSortAsc] = createSignal(true);
  const [search, setSearch] = createSignal("");
  const [paletteOpen, setPaletteOpen] = createSignal(false);
  const [layoutOpen, setLayoutOpen] = createSignal(false);
  const [applying, setApplying] = createSignal(false);

  const sortedZones = createMemo(() => {
    const list = [...zonesStore.zones];
    const key = sortKey();
    const asc = sortAsc();
    list.sort((a, b) => {
      let cmp = 0;
      switch (key) {
        case "name":
          cmp = a.name.localeCompare(b.name);
          break;
        case "items":
          cmp = a.items.length - b.items.length;
          break;
        case "accent":
          cmp = (a.accent_color ?? "").localeCompare(b.accent_color ?? "");
          break;
        case "size":
          cmp =
            a.expanded_size.w_percent * a.expanded_size.h_percent -
            b.expanded_size.w_percent * b.expanded_size.h_percent;
          break;
      }
      return asc ? cmp : -cmp;
    });
    const term = search().trim().toLowerCase();
    if (!term) return list;
    return list.filter((z) => z.name.toLowerCase().includes(term));
  });

  const allChecked = createMemo(() => {
    const visible = sortedZones();
    if (visible.length === 0) return false;
    const sel = selected();
    return visible.every((z) => sel.has(z.id));
  });

  function toggle(id: string): void {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleAll(): void {
    if (allChecked()) {
      setSelected(new Set<string>());
    } else {
      setSelected(new Set<string>(sortedZones().map((z) => z.id)));
    }
  }

  function invert(): void {
    setSelected((prev) => {
      const next = new Set<string>();
      for (const z of sortedZones()) {
        if (!prev.has(z.id)) next.add(z.id);
      }
      return next;
    });
  }

  function setSort(key: SortKey): void {
    if (sortKey() === key) setSortAsc((p) => !p);
    else {
      setSortKey(key);
      setSortAsc(true);
    }
  }

  async function applyAccent(color: string): Promise<void> {
    const ids = [...selected()];
    if (ids.length === 0) return;
    setApplying(true);
    try {
      const updates: BulkZoneUpdate[] = ids.map((id) => ({
        id,
        accent_color: color,
      }));
      await bulkUpdateZones(updates);
      setPaletteOpen(false);
    } finally {
      setApplying(false);
    }
  }

  async function applyAutoLayout(algo: LayoutAlgorithm): Promise<void> {
    const ids = [...selected()];
    const targets =
      ids.length > 0
        ? sortedZones().filter((z) => ids.includes(z.id))
        : sortedZones();
    if (targets.length === 0) return;
    setApplying(true);
    try {
      await applyLayout(algo, targets);
      setLayoutOpen(false);
    } finally {
      setApplying(false);
    }
  }

  async function deleteSelected(): Promise<void> {
    const ids = [...selected()];
    if (ids.length === 0) return;
    setApplying(true);
    try {
      await bulkDeleteZones(ids);
      setSelected(new Set<string>());
    } finally {
      setApplying(false);
    }
  }

  function handleKey(e: KeyboardEvent): void {
    if (!isBulkManagerOpen()) return;
    if (e.key === "Escape") closeBulkManager();
  }

  onMount(() => document.addEventListener("keydown", handleKey));
  onCleanup(() => document.removeEventListener("keydown", handleKey));

  return (
    <Show when={isBulkManagerOpen()}>
      <div
        class="bulk-manager__scrim"
        role="dialog"
        aria-modal="true"
        aria-label={t("bulkManagerTitle")}
      >
        <div class="bulk-manager__panel">
          <header class="bulk-manager__header">
            <h2>{t("bulkManagerTitle")}</h2>
            <input
              class="bulk-manager__search"
              type="search"
              placeholder={t("bulkManagerSearchPlaceholder")}
              value={search()}
              onInput={(e) => setSearch(e.currentTarget.value)}
            />
            <button
              class="bulk-manager__close"
              onClick={closeBulkManager}
              aria-label={t("settingsCloseAriaLabel")}
            >
              ×
            </button>
          </header>

          <div class="bulk-manager__toolbar">
            <button
              class="bulk-manager__btn"
              onClick={toggleAll}
              aria-pressed={allChecked()}
            >
              {allChecked()
                ? t("bulkManagerDeselectAll")
                : t("bulkManagerSelectAll")}
            </button>
            <button class="bulk-manager__btn" onClick={invert}>
              {t("bulkManagerInvert")}
            </button>
            <div class="bulk-manager__divider" />
            <button
              class="bulk-manager__btn bulk-manager__btn--primary"
              disabled={selected().size === 0 || applying()}
              onClick={() => setPaletteOpen((p) => !p)}
            >
              {t("bulkManagerApplyPalette")}
            </button>
            <button
              class="bulk-manager__btn bulk-manager__btn--primary"
              disabled={applying()}
              onClick={() => setLayoutOpen((p) => !p)}
            >
              {t("bulkManagerAutoLayout")}
            </button>
            <button
              class="bulk-manager__btn bulk-manager__btn--danger"
              disabled={selected().size === 0 || applying()}
              onClick={deleteSelected}
            >
              {t("bulkManagerDeleteSelected")}
            </button>
            <div class="bulk-manager__counter">
              {t("bulkManagerSelectedLabel")} {selected().size}/
              {sortedZones().length}
            </div>
          </div>

          <Show when={paletteOpen()}>
            <PalettePicker
              onPick={applyAccent}
              onClose={() => setPaletteOpen(false)}
            />
          </Show>
          <Show when={layoutOpen()}>
            <AutoLayoutMenu
              onPick={applyAutoLayout}
              onClose={() => setLayoutOpen(false)}
            />
          </Show>

          <div class="bulk-manager__tablewrap">
            <table class="bulk-manager__table">
              <thead>
                <tr>
                  <th />
                  <th
                    class={`bulk-manager__sort ${
                      sortKey() === "name" ? "is-active" : ""
                    }`}
                    onClick={() => setSort("name")}
                  >
                    {t("bulkManagerColName")}
                  </th>
                  <th
                    class={`bulk-manager__sort ${
                      sortKey() === "items" ? "is-active" : ""
                    }`}
                    onClick={() => setSort("items")}
                  >
                    {t("bulkManagerColItems")}
                  </th>
                  <th
                    class={`bulk-manager__sort ${
                      sortKey() === "accent" ? "is-active" : ""
                    }`}
                    onClick={() => setSort("accent")}
                  >
                    {t("bulkManagerColAccent")}
                  </th>
                  <th
                    class={`bulk-manager__sort ${
                      sortKey() === "size" ? "is-active" : ""
                    }`}
                    onClick={() => setSort("size")}
                  >
                    {t("bulkManagerColSize")}
                  </th>
                  <th>{t("bulkManagerColPosition")}</th>
                </tr>
              </thead>
              <tbody>
                <For each={sortedZones()}>
                  {(z) => (
                    <tr class={selected().has(z.id) ? "is-selected" : ""}>
                      <td>
                        <input
                          type="checkbox"
                          checked={selected().has(z.id)}
                          onChange={() => toggle(z.id)}
                        />
                      </td>
                      <td class="bulk-manager__name">{z.name}</td>
                      <td>{z.items.length}</td>
                      <td>
                        <span
                          class="bulk-manager__swatch"
                          style={{
                            "background-color":
                              z.accent_color ?? "transparent",
                          }}
                        />
                        <span class="bulk-manager__swatchlabel">
                          {z.accent_color ?? "—"}
                        </span>
                      </td>
                      <td>
                        {Math.round(z.expanded_size.w_percent)}×
                        {Math.round(z.expanded_size.h_percent)}%
                      </td>
                      <td>
                        {Math.round(z.position.x_percent)},
                        {Math.round(z.position.y_percent)}
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default BulkManagerPanel;
