import { Component, For, Show, createMemo } from "solid-js";
import type { BentoZone } from "../../types/zone";
import { t } from "../../i18n";
import ZoneIcon from "../Icons/ZoneIcon";
import Tooltip from "../shared/Tooltip";
import { useTextAbbr } from "../../services/textAbbr";

interface StackCapsuleProps {
  zones: BentoZone[];
  open: boolean;
  hasPreview: boolean;
  locked: boolean;
  onMouseDown: (e: MouseEvent) => void;
  onClick: (e: MouseEvent) => void;
  onContextMenu: (e: MouseEvent) => void;
}

const StackCapsule: Component<StackCapsuleProps> = (props) => {
  const topZone = createMemo(() => props.zones[props.zones.length - 1]);
  const fullName = () => topZone()?.alias ?? topZone()?.name ?? "";
  const memberPeek = createMemo(() => props.zones.slice(-3));

  const abbr = useTextAbbr(fullName);

  return (
    <button
      type="button"
      class={`stack-capsule spring-emerge ${props.open ? "is-open" : ""} ${props.hasPreview ? "has-preview" : ""} ${props.locked ? "is-locked" : ""}`}
      onMouseDown={props.onMouseDown}
      onClick={props.onClick}
      onContextMenu={props.onContextMenu}
      aria-expanded={props.open}
      aria-disabled={props.locked}
      aria-label={t("stackAriaLabel").replace("{n}", String(props.zones.length))}
    >
      <span class="stack-capsule__peek" aria-hidden="true">
        <For each={memberPeek()}>
          {(zone) => (
            <span class="stack-capsule__peek-icon">
              <ZoneIcon icon={zone.icon} size={12} />
            </span>
          )}
        </For>
      </span>
      <span class="stack-capsule__icon">
        <ZoneIcon icon={topZone()?.icon ?? "folder"} size={18} />
      </span>
      <span
        class="stack-capsule__title"
        ref={abbr.setRef}
        aria-label={fullName()}
      >
        <Tooltip content={fullName()} disabled={abbr.tooltipDisabled()}>
          {abbr.text()}
        </Tooltip>
      </span>
      <Show when={props.hasPreview}>
        <span class="stack-capsule__preview-indicator">
          {t("stackPreviewActive")}
        </span>
      </Show>
      <span class="stack-capsule__badge">{props.zones.length}</span>
    </button>
  );
};

export default StackCapsule;
