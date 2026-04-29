/**
 * SelectionFloatingBar — bottom-of-screen action bar shown whenever the user
 * has selected two or more zones via Shift/Ctrl+click or marquee.
 *
 * Fix-FE-8: prior to v1.2.4 multi-selection was tracked but had no surface;
 * users could not act on the selection without right-clicking each zone.
 * The bar exposes the four operations the v1.2.4 PRD calls out — bulk
 * rename, delete, align, and grid arrange — and routes through the existing
 * Bulk Manager panel for everything else.
 */
import { Component, Show } from "solid-js";
import {
  selectedZoneIds,
  clearMultiSelection,
} from "../../stores/selection";
import { deleteZone, loadZones, zonesStore } from "../../stores/zones";
import { bulkUpdateZones } from "../../services/ipc";
import { computeAutoSpread } from "../../services/stack";
import { openBulkManager, showFlashToast } from "../../stores/ui";
import { t } from "../../i18n";
import "./SelectionFloatingBar.css";

const SelectionFloatingBar: Component = () => {
  const count = () => selectedZoneIds().size;

  async function handleAlign(): Promise<void> {
    const ids = Array.from(selectedZoneIds());
    if (ids.length < 2) return;
    const spread = computeAutoSpread(zonesStore.zones, ids);
    if (spread.length === 0) return;
    try {
      await bulkUpdateZones(
        spread.map((entry) => ({
          id: entry.id,
          position: { x_percent: entry.x_percent, y_percent: entry.y_percent },
        })),
      );
      await loadZones();
    } catch (e) {
      showFlashToast(`${t("bulkBarAlignError") || "Align failed"}: ${String(e)}`, "error");
    }
  }

  async function handleGrid(): Promise<void> {
    const ids = Array.from(selectedZoneIds());
    if (ids.length < 2) return;
    const zones = ids
      .map((id) => zonesStore.zones.find((z) => z.id === id))
      .filter((z): z is NonNullable<typeof z> => z !== undefined)
      .sort((a, b) => a.sort_order - b.sort_order);
    if (zones.length < 2) return;
    const cols = Math.ceil(Math.sqrt(zones.length));
    const colStep = 100 / Math.max(cols + 1, 2);
    const rowStep = 12;
    const updates = zones.map((zone, idx) => {
      const col = idx % cols;
      const row = Math.floor(idx / cols);
      return {
        id: zone.id,
        position: {
          x_percent: colStep * (col + 1) - colStep / 2,
          y_percent: 8 + row * rowStep,
        },
      };
    });
    try {
      await bulkUpdateZones(updates);
      await loadZones();
    } catch (e) {
      showFlashToast(`${t("bulkBarGridError") || "Grid failed"}: ${String(e)}`, "error");
    }
  }

  async function handleDelete(): Promise<void> {
    const ids = Array.from(selectedZoneIds());
    if (ids.length === 0) return;
    if (typeof window !== "undefined" && typeof window.confirm === "function") {
      const message = (t("bulkBarConfirmDelete") || "Delete {count} zones?").replace(
        "{count}",
        String(ids.length),
      );
      if (!window.confirm(message)) return;
    }
    let failures = 0;
    for (const id of ids) {
      const ok = await deleteZone(id);
      if (!ok) failures += 1;
    }
    clearMultiSelection();
    if (failures > 0) {
      showFlashToast(
        `${t("bulkBarDeleteError") || "Some zones could not be deleted"} (${failures})`,
        "error",
      );
    }
  }

  function handleBulkManager(): void {
    openBulkManager();
  }

  return (
    <Show when={count() >= 2}>
      <div
        class="selection-floating-bar"
        role="toolbar"
        aria-label={t("bulkBarAriaLabel") || "Bulk zone actions"}
      >
        <span class="selection-floating-bar__count">
          {(t("bulkBarSelectedCount") || "{n} selected").replace(
            "{n}",
            String(count()),
          )}
        </span>
        <button
          class="selection-floating-bar__btn"
          type="button"
          onClick={handleBulkManager}
        >
          {t("bulkBarRename") || "Bulk Manager"}
        </button>
        <button
          class="selection-floating-bar__btn"
          type="button"
          onClick={() => void handleAlign()}
        >
          {t("bulkBarAlign") || "Align"}
        </button>
        <button
          class="selection-floating-bar__btn"
          type="button"
          onClick={() => void handleGrid()}
        >
          {t("bulkBarGrid") || "Grid"}
        </button>
        <button
          class="selection-floating-bar__btn selection-floating-bar__btn--danger"
          type="button"
          onClick={() => void handleDelete()}
        >
          {t("bulkBarDelete") || "Delete"}
        </button>
        <button
          class="selection-floating-bar__btn selection-floating-bar__btn--ghost"
          type="button"
          onClick={() => clearMultiSelection()}
        >
          {t("bulkBarClear") || "Clear"}
        </button>
      </div>
    </Show>
  );
};

export default SelectionFloatingBar;
