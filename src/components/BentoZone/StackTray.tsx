import { Component, For, Show, createMemo } from "solid-js";
import type { BentoZone } from "../../types/zone";
import { t } from "../../i18n";
import ZoneIcon from "../Icons/ZoneIcon";
import Tooltip from "../shared/Tooltip";

interface StackTrayProps {
  zones: BentoZone[];
  previewZoneId: string | null;
  onSelectPreview: (zoneId: string) => void;
  onDetach: (zoneId: string) => void;
  onDissolve: () => void;
}

const StackTray: Component<StackTrayProps> = (props) => {
  const orderedZones = createMemo(() => [...props.zones].reverse());

  return (
    <div class="stack-tray" onMouseDown={(e) => e.stopPropagation()}>
      <div class="stack-tray__header">
        <div class="stack-tray__meta">
          <span class="stack-tray__title">{t("stackMembersLabel")}</span>
          <span class="stack-tray__count">{props.zones.length}</span>
        </div>
        <button
          type="button"
          class="stack-tray__toolbar-btn stack-tray__toolbar-btn--danger"
          onClick={props.onDissolve}
        >
          {t("stackDissolve")}
        </button>
      </div>
      <div class="stack-tray__list">
        <For each={orderedZones()}>
          {(zone) => {
            const selected = () => props.previewZoneId === zone.id;
            const zoneName = () => zone.alias ?? zone.name;
            return (
              <div class={`stack-tray__row ${selected() ? "is-selected" : ""}`}>
                <button
                  type="button"
                  class="stack-tray__member"
                  onClick={() => props.onSelectPreview(zone.id)}
                  aria-pressed={selected()}
                >
                  <span class="stack-tray__member-icon">
                    <ZoneIcon icon={zone.icon} size={16} />
                  </span>
                  <span class="stack-tray__member-text">
                    <Tooltip content={zoneName()}>
                      <span class="stack-tray__member-name">{zoneName()}</span>
                    </Tooltip>
                    <span class="stack-tray__member-meta">
                      {zone.items.length} {t("bulkManagerColItems")}
                    </span>
                  </span>
                </button>
                <div class="stack-tray__actions">
                  <button
                    type="button"
                    class="stack-tray__action-btn"
                    onClick={() => props.onSelectPreview(zone.id)}
                  >
                    <Show
                      when={selected()}
                      fallback={t("stackShowPreview")}
                    >
                      {t("stackHidePreview")}
                    </Show>
                  </button>
                  <button
                    type="button"
                    class="stack-tray__action-btn"
                    onClick={() => props.onDetach(zone.id)}
                  >
                    {t("stackDetachMember")}
                  </button>
                </div>
              </div>
            );
          }}
        </For>
      </div>
    </div>
  );
};

export default StackTray;
