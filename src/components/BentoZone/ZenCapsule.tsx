/**
 * ZenCapsule — Collapsed state of a BentoZone.
 * Displays: icon, title, item count badge.
 * 160x48px capsule with glassmorphism.
 */
import { Component, createSignal, createMemo, onMount, onCleanup } from "solid-js";
import type { BentoZone } from "../../types/zone";
import ZoneIcon from "../Icons/ZoneIcon";
import Tooltip from "../shared/Tooltip";
import { smartAbbreviate, getFontCtx } from "../../services/textAbbr";
import "./ZenCapsule.css";

interface ZenCapsuleProps {
  zone: BentoZone;
}

const ZenCapsule: Component<ZenCapsuleProps> = (props) => {
  const capsuleClass = () => {
    const shape = props.zone.capsule_shape || "pill";
    const size = props.zone.capsule_size || "medium";
    return `zen-capsule zen-capsule--${shape} zen-capsule--${size}`;
  };

  const fullName = () => props.zone.alias ?? props.zone.name;

  const [titleEl, setTitleEl] = createSignal<HTMLElement | undefined>();
  const [maxPx, setMaxPx] = createSignal(0);
  const [fontCtx, setFontCtx] = createSignal<{ font: string }>({ font: "12px sans-serif" });

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
    const w = maxPx();
    const name = fullName();
    if (w <= 0) return name;
    return smartAbbreviate(name, w, fontCtx());
  });

  const tooltipDisabled = createMemo(() => abbreviated() === fullName());

  return (
    <div class={capsuleClass()}>
      <span class="zen-capsule__icon">
          <ZoneIcon icon={props.zone.icon} size={18} />
        </span>
      <span
        class="zen-capsule__title"
        ref={setTitleEl}
        aria-label={fullName()}
      >
        <Tooltip content={fullName()} disabled={tooltipDisabled()}>
          {abbreviated()}
        </Tooltip>
      </span>
      <span class="zen-capsule__badge">{props.zone.items.length}</span>
    </div>
  );
};

export default ZenCapsule;
