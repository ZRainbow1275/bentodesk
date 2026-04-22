/**
 * BulkManagerPanel - Bulk operations UI backed by the shared selection store.
 *
 * The panel no longer owns a second "selected zones" state. Canvas selection
 * and table selection both read/write `stores/selection.ts`, and every bulk
 * mutation reloads zones afterwards so the UI reflects persisted truth.
 */
import {
  Component,
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { isBulkManagerOpen, closeBulkManager } from "../../stores/ui";
import { loadZones, zonesStore } from "../../stores/zones";
import {
  bulkDeleteZones,
  bulkUpdateZones,
  type BulkZoneUpdate,
} from "../../services/ipc";
import { applyLayout, type LayoutAlgorithm } from "../../services/autoLayout";
import {
  selectedZoneIds,
  setZoneSelection,
} from "../../stores/selection";
import type { CapsuleSize, ZoneDisplayMode } from "../../types/zone";
import PalettePicker from "./PalettePicker";
import AutoLayoutMenu from "./AutoLayoutMenu";
import { t } from "../../i18n";
import "./BulkManagerPanel.css";

type SortKey = "name" | "items" | "accent" | "size";
type LockedBulkValue = "unchanged" | "locked" | "unlocked";

const BulkManagerPanel: Component = () => {
  const [sortKey, setSortKey] = createSignal<SortKey>("name");
  const [sortAsc, setSortAsc] = createSignal(true);
  const [search, setSearch] = createSignal("");
  const [paletteOpen, setPaletteOpen] = createSignal(false);
  const [layoutOpen, setLayoutOpen] = createSignal(false);
  const [applying, setApplying] = createSignal(false);

  const [aliasValue, setAliasValue] = createSignal("");
  const [aliasDirty, setAliasDirty] = createSignal(false);
  const [capsuleSizeValue, setCapsuleSizeValue] = createSignal<"" | CapsuleSize>("");
  const [displayModeValue, setDisplayModeValue] = createSignal<"" | ZoneDisplayMode>("");
  const [lockedValue, setLockedValue] = createSignal<LockedBulkValue>("unchanged");

  const resetBulkFields = (): void => {
    setAliasValue("");
    setAliasDirty(false);
    setCapsuleSizeValue("");
    setDisplayModeValue("");
    setLockedValue("unchanged");
  };

  const sortedZones = createMemo(() => {
    const list = [...zonesStore.zones];
    const key = sortKey();
    const asc = sortAsc();
    list.sort((a, b) => {
      let cmp = 0;
      switch (key) {
        case "name":
          cmp = (a.alias ?? a.name).localeCompare(b.alias ?? b.name);
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
    return list.filter((zone) =>
      `${zone.alias ?? ""} ${zone.name}`.toLowerCase().includes(term),
    );
  });

  const selectedCount = createMemo(() => selectedZoneIds().size);

  const allChecked = createMemo(() => {
    const visible = sortedZones();
    if (visible.length === 0) return false;
    const selected = selectedZoneIds();
    return visible.every((zone) => selected.has(zone.id));
  });

  const hasBulkFieldChanges = createMemo(() => {
    return (
      aliasDirty() ||
      capsuleSizeValue() !== "" ||
      displayModeValue() !== "" ||
      lockedValue() !== "unchanged"
    );
  });

  function toggle(id: string): void {
    const next = new Set(selectedZoneIds());
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setZoneSelection(next);
  }

  function toggleAll(): void {
    const next = new Set(selectedZoneIds());
    const visibleIds = sortedZones().map((zone) => zone.id);
    if (allChecked()) {
      for (const id of visibleIds) next.delete(id);
    } else {
      for (const id of visibleIds) next.add(id);
    }
    setZoneSelection(next);
  }

  function invert(): void {
    const next = new Set(selectedZoneIds());
    for (const zone of sortedZones()) {
      if (next.has(zone.id)) next.delete(zone.id);
      else next.add(zone.id);
    }
    setZoneSelection(next);
  }

  function setSort(key: SortKey): void {
    if (sortKey() === key) setSortAsc((prev) => !prev);
    else {
      setSortKey(key);
      setSortAsc(true);
    }
  }

  async function applyAccent(color: string): Promise<void> {
    const ids = [...selectedZoneIds()];
    if (ids.length === 0) return;
    setApplying(true);
    try {
      const updates: BulkZoneUpdate[] = ids.map((id) => ({
        id,
        accent_color: color,
      }));
      await bulkUpdateZones(updates);
      await loadZones();
      setPaletteOpen(false);
    } finally {
      setApplying(false);
    }
  }

  async function applyAutoLayout(algo: LayoutAlgorithm): Promise<void> {
    const ids = [...selectedZoneIds()];
    const targets =
      ids.length > 0
        ? sortedZones().filter((zone) => ids.includes(zone.id))
        : sortedZones();
    if (targets.length === 0) return;
    setApplying(true);
    try {
      await applyLayout(algo, targets);
      await loadZones();
      setLayoutOpen(false);
    } finally {
      setApplying(false);
    }
  }

  async function applyBulkFields(): Promise<void> {
    const ids = [...selectedZoneIds()];
    if (ids.length === 0) return;

    const updates = ids
      .map((id) => {
        const update: BulkZoneUpdate = { id };
        let changed = false;

        if (aliasDirty()) {
          update.alias = aliasValue().trim();
          changed = true;
        }

        const capsuleSize = capsuleSizeValue();
        if (capsuleSize !== "") {
          update.capsule_size = capsuleSize;
          changed = true;
        }

        const displayMode = displayModeValue();
        if (displayMode !== "") {
          update.display_mode = displayMode;
          changed = true;
        }

        if (lockedValue() !== "unchanged") {
          update.locked = lockedValue() === "locked";
          changed = true;
        }

        return changed ? update : null;
      })
      .filter((update): update is BulkZoneUpdate => update !== null);

    if (updates.length === 0) return;

    setApplying(true);
    try {
      await bulkUpdateZones(updates);
      await loadZones();
      resetBulkFields();
    } finally {
      setApplying(false);
    }
  }

  async function deleteSelected(): Promise<void> {
    const ids = [...selectedZoneIds()];
    if (ids.length === 0) return;
    setApplying(true);
    try {
      await bulkDeleteZones(ids);
      setZoneSelection([]);
      await loadZones();
    } finally {
      setApplying(false);
    }
  }

  function handleKey(e: KeyboardEvent): void {
    if (!isBulkManagerOpen()) return;
    if (e.key === "Escape") closeBulkManager();
  }

  createEffect(() => {
    if (isBulkManagerOpen()) return;
    setPaletteOpen(false);
    setLayoutOpen(false);
    setSearch("");
    resetBulkFields();
  });

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
              x
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
              disabled={selectedCount() === 0 || applying()}
              onClick={() => setPaletteOpen((prev) => !prev)}
            >
              {t("bulkManagerApplyPalette")}
            </button>
            <button
              class="bulk-manager__btn bulk-manager__btn--primary"
              disabled={applying()}
              onClick={() => setLayoutOpen((prev) => !prev)}
            >
              {t("bulkManagerAutoLayout")}
            </button>
            <button
              class="bulk-manager__btn bulk-manager__btn--danger"
              disabled={selectedCount() === 0 || applying()}
              onClick={() => void deleteSelected()}
            >
              {t("bulkManagerDeleteSelected")}
            </button>
            <div class="bulk-manager__counter">
              {t("bulkManagerSelectedLabel")} {selectedCount()}/
              {sortedZones().length}
            </div>
          </div>

          <div class="bulk-manager__bulk-form">
            <label class="bulk-manager__field">
              <span class="bulk-manager__field-label">
                {t("bulkManagerAliasLabel")}
              </span>
              <input
                class="bulk-manager__input"
                type="text"
                value={aliasValue()}
                placeholder={t("bulkManagerAliasPlaceholder")}
                onInput={(e) => {
                  setAliasValue(e.currentTarget.value);
                  setAliasDirty(true);
                }}
              />
            </label>

            <label class="bulk-manager__field">
              <span class="bulk-manager__field-label">
                {t("bulkManagerCapsuleSizeLabel")}
              </span>
              <select
                class="bulk-manager__select"
                value={capsuleSizeValue()}
                onChange={(e) =>
                  setCapsuleSizeValue(e.currentTarget.value as "" | CapsuleSize)
                }
              >
                <option value="">{t("bulkManagerKeepCurrent")}</option>
                <option value="small">{t("zoneEditorCapsuleSizeSmall")}</option>
                <option value="medium">{t("zoneEditorCapsuleSizeMedium")}</option>
                <option value="large">{t("zoneEditorCapsuleSizeLarge")}</option>
              </select>
            </label>

            <label class="bulk-manager__field">
              <span class="bulk-manager__field-label">
                {t("bulkManagerDisplayModeLabel")}
              </span>
              <select
                class="bulk-manager__select"
                value={displayModeValue()}
                onChange={(e) =>
                  setDisplayModeValue(
                    e.currentTarget.value as "" | ZoneDisplayMode,
                  )
                }
              >
                <option value="">{t("bulkManagerKeepCurrent")}</option>
                <option value="hover">{t("settingsDisplayModeHover")}</option>
                <option value="always">{t("settingsDisplayModeAlways")}</option>
                <option value="click">{t("settingsDisplayModeClick")}</option>
              </select>
            </label>

            <label class="bulk-manager__field">
              <span class="bulk-manager__field-label">
                {t("bulkManagerLockedLabel")}
              </span>
              <select
                class="bulk-manager__select"
                value={lockedValue()}
                onChange={(e) =>
                  setLockedValue(e.currentTarget.value as LockedBulkValue)
                }
              >
                <option value="unchanged">{t("bulkManagerKeepCurrent")}</option>
                <option value="locked">{t("bulkManagerLockedOn")}</option>
                <option value="unlocked">{t("bulkManagerLockedOff")}</option>
              </select>
            </label>

            <button
              class="bulk-manager__btn bulk-manager__btn--primary bulk-manager__apply-btn"
              disabled={
                selectedCount() === 0 ||
                !hasBulkFieldChanges() ||
                applying()
              }
              onClick={() => void applyBulkFields()}
            >
              {t("bulkManagerApplyFields")}
            </button>
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
                  {(zone) => (
                    <tr class={selectedZoneIds().has(zone.id) ? "is-selected" : ""}>
                      <td>
                        <input
                          type="checkbox"
                          checked={selectedZoneIds().has(zone.id)}
                          onChange={() => toggle(zone.id)}
                        />
                      </td>
                      <td class="bulk-manager__name">{zone.alias ?? zone.name}</td>
                      <td>{zone.items.length}</td>
                      <td>
                        <span
                          class="bulk-manager__swatch"
                          style={{
                            "background-color":
                              zone.accent_color ?? "transparent",
                          }}
                        />
                        <span class="bulk-manager__swatchlabel">
                          {zone.accent_color ?? "-"}
                        </span>
                      </td>
                      <td>
                        {Math.round(zone.expanded_size.w_percent)} x{" "}
                        {Math.round(zone.expanded_size.h_percent)}%
                      </td>
                      <td>
                        {Math.round(zone.position.x_percent)},
                        {Math.round(zone.position.y_percent)}
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
