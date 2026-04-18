/**
 * PalettePicker — swatch grid for picking an accent color to apply in bulk.
 *
 * Renders inside BulkManagerPanel as a popover above the table; chosen
 * color is relayed via `onPick` to a `bulk_update_zones` IPC.
 */
import { Component, For } from "solid-js";
import { ACCENT_PRESETS } from "../../themes/accentPresets";
import { t } from "../../i18n";

interface Props {
  onPick: (color: string) => void;
  onClose: () => void;
}

const PalettePicker: Component<Props> = (props) => {
  return (
    <div class="bulk-manager__palette" role="menu">
      <header class="bulk-manager__palette-header">
        <span>{t("bulkManagerPickColor")}</span>
        <button
          class="bulk-manager__close bulk-manager__close--sm"
          onClick={props.onClose}
          aria-label={t("settingsCloseAriaLabel")}
        >
          ×
        </button>
      </header>
      <For each={ACCENT_PRESETS}>
        {(preset) => (
          <div class="bulk-manager__palette-row">
            <span class="bulk-manager__palette-name">{t(preset.nameKey)}</span>
            <div class="bulk-manager__palette-swatches">
              <For each={preset.colors}>
                {(color) => (
                  <button
                    type="button"
                    class="bulk-manager__palette-swatch"
                    style={{ "background-color": color }}
                    title={color}
                    onClick={() => props.onPick(color)}
                    aria-label={color}
                  />
                )}
              </For>
            </div>
          </div>
        )}
      </For>
    </div>
  );
};

export default PalettePicker;
