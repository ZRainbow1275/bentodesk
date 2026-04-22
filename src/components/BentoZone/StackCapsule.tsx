import {
  Component,
  For,
  Show,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import type { BentoZone } from "../../types/zone";
import { t } from "../../i18n";
import ZoneIcon from "../Icons/ZoneIcon";
import Tooltip from "../shared/Tooltip";
import { getFontCtx, smartAbbreviate } from "../../services/textAbbr";

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

  const [titleEl, setTitleEl] = createSignal<HTMLElement | undefined>();
  const [maxPx, setMaxPx] = createSignal(0);
  const [fontCtx, setFontCtx] = createSignal<{ font: string }>({
    font: "12px sans-serif",
  });

  onMount(() => {
    const el = titleEl();
    if (!el) return;
    setFontCtx(getFontCtx(el));
    const measure = () => setMaxPx(el.clientWidth);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    onCleanup(() => ro.disconnect());
  });

  const abbreviated = createMemo(() => {
    const width = maxPx();
    const name = fullName();
    if (width <= 0) return name;
    return smartAbbreviate(name, width, fontCtx());
  });

  const tooltipDisabled = createMemo(() => abbreviated() === fullName());

  return (
    <button
      type="button"
      class={`stack-capsule ${props.open ? "is-open" : ""} ${props.hasPreview ? "has-preview" : ""} ${props.locked ? "is-locked" : ""}`}
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
        ref={setTitleEl}
        aria-label={fullName()}
      >
        <Tooltip content={fullName()} disabled={tooltipDisabled()}>
          {abbreviated()}
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
