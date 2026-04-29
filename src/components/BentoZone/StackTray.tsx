import { Component, For, Show, createMemo } from "solid-js";
import type { BentoZone } from "../../types/zone";
import { t } from "../../i18n";
import ZoneIcon from "../Icons/ZoneIcon";
import Tooltip from "../shared/Tooltip";
import {
  FontGroupContext,
  createFontGroup,
  useTextAbbrGroup,
} from "../../services/textAbbrGroup";

/** stack-tray__member-name's CSS-declared default size. */
const STACK_TRAY_MEMBER_DEFAULT_FONT_PX = 13;

interface StackTrayProps {
  zones: BentoZone[];
  previewZoneId: string | null;
  onSelectPreview: (zoneId: string) => void;
  onDetach: (zoneId: string) => void;
  onDissolve: () => void;
}

interface StackTrayRowProps {
  zone: BentoZone;
  selected: boolean;
  onSelectPreview: (zoneId: string) => void;
  onDetach: (zoneId: string) => void;
}

/**
 * Single tray row. Lifted to its own component so each row owns an isolated
 * `useTextAbbr` instance — the hook installs a ResizeObserver and the lifecycle
 * must live in a per-row component, not inside the parent's `For` callback.
 */
const StackTrayRow: Component<StackTrayRowProps> = (props) => {
  const zoneName = () => props.zone.alias ?? props.zone.name;
  // v8: row participates in the StackTray-wide FontGroup so member names in
  // a single tray render at a uniform size. Falls back to per-row sizing
  // if a row is somehow rendered outside a FontGroupContext provider.
  const abbr = useTextAbbrGroup(zoneName);

  return (
    <div class={`stack-tray__row ${props.selected ? "is-selected" : ""}`}>
      <button
        type="button"
        class="stack-tray__member"
        onClick={() => props.onSelectPreview(props.zone.id)}
        aria-pressed={props.selected}
      >
        <span class="stack-tray__member-icon">
          <ZoneIcon icon={props.zone.icon} size={16} />
        </span>
        <span class="stack-tray__member-text">
          <Tooltip
            content={zoneName()}
            disabled={abbr.tooltipDisabled()}
          >
            <span
              class="stack-tray__member-name"
              ref={abbr.setRef}
              aria-label={zoneName()}
              style={{ "font-size": `${abbr.fontSize()}px` }}
            >
              {abbr.text()}
            </span>
          </Tooltip>
          <span class="stack-tray__member-meta">
            {props.zone.items.length} {t("bulkManagerColItems")}
          </span>
        </span>
      </button>
      <div class="stack-tray__actions">
        <button
          type="button"
          class="stack-tray__action-btn"
          onClick={() => props.onSelectPreview(props.zone.id)}
        >
          <Show
            when={props.selected}
            fallback={t("stackShowPreview")}
          >
            {t("stackHidePreview")}
          </Show>
        </button>
        <button
          type="button"
          class="stack-tray__action-btn"
          onClick={() => props.onDetach(props.zone.id)}
        >
          {t("stackDetachMember")}
        </button>
      </div>
    </div>
  );
};

const StackTray: Component<StackTrayProps> = (props) => {
  const orderedZones = createMemo(() => [...props.zones].reverse());

  // v8 font-uniformity: every member row registers with the same group so a
  // tray with mixed-length zone names renders one consistent font-size
  // instead of a ragged stack. Default 13px matches the CSS-declared size
  // for `.stack-tray__member-name`.
  const fontGroup = createFontGroup(STACK_TRAY_MEMBER_DEFAULT_FONT_PX);

  return (
    <div class="stack-tray spring-emerge" onMouseDown={(e) => e.stopPropagation()}>
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
      <FontGroupContext.Provider value={fontGroup}>
        <div class="stack-tray__list">
          <For each={orderedZones()}>
            {(zone) => (
              <StackTrayRow
                zone={zone}
                selected={props.previewZoneId === zone.id}
                onSelectPreview={props.onSelectPreview}
                onDetach={props.onDetach}
              />
            )}
          </For>
        </div>
      </FontGroupContext.Provider>
    </div>
  );
};

export default StackTray;
