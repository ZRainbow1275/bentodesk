/**
 * AutoLayoutMenu — popover to pick one of the four auto-layout algorithms.
 *
 * Each entry ships a tiny preview icon so the user can eyeball the shape
 * before committing; the computation happens in `autoLayout.ts` and the
 * result lands via `bulk_update_zones` (single timeline checkpoint).
 */
import { Component, For } from "solid-js";
import type { LayoutAlgorithm } from "../../services/autoLayout";
import { t } from "../../i18n";
import type { TranslationKey } from "../../i18n/locales/zh-CN";

interface Props {
  onPick: (algo: LayoutAlgorithm) => void;
  onClose: () => void;
}

const ENTRIES: Array<{ id: LayoutAlgorithm; labelKey: TranslationKey; preview: string }> = [
  { id: "grid", labelKey: "bulkManagerLayoutGrid", preview: "⊞" },
  { id: "row", labelKey: "bulkManagerLayoutRow", preview: "⋯" },
  { id: "column", labelKey: "bulkManagerLayoutColumn", preview: "⋮" },
  { id: "spiral", labelKey: "bulkManagerLayoutSpiral", preview: "↺" },
  { id: "organic", labelKey: "bulkManagerLayoutOrganic", preview: "❁" },
];

const AutoLayoutMenu: Component<Props> = (props) => {
  return (
    <div class="bulk-manager__layoutmenu" role="menu">
      <header class="bulk-manager__palette-header">
        <span>{t("bulkManagerPickLayout")}</span>
        <button
          class="bulk-manager__close bulk-manager__close--sm"
          onClick={props.onClose}
          aria-label={t("settingsCloseAriaLabel")}
        >
          ×
        </button>
      </header>
      <For each={ENTRIES}>
        {(entry) => (
          <button
            type="button"
            class="bulk-manager__layout-option"
            onClick={() => props.onPick(entry.id)}
          >
            <span class="bulk-manager__layout-preview">{entry.preview}</span>
            <span class="bulk-manager__layout-label">{t(entry.labelKey)}</span>
          </button>
        )}
      </For>
    </div>
  );
};

export default AutoLayoutMenu;
