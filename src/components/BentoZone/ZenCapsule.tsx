/**
 * ZenCapsule — Collapsed state of a BentoZone.
 * Displays: icon, title, item count badge.
 * 160x48px capsule with glassmorphism.
 */
import { Component } from "solid-js";
import type { BentoZone } from "../../types/zone";
import ZoneIcon from "../Icons/ZoneIcon";
import Tooltip from "../shared/Tooltip";
import { useTextAbbr } from "../../services/textAbbr";
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

  const abbr = useTextAbbr(fullName);

  return (
    <div class={capsuleClass()}>
      <span class="zen-capsule__icon">
          <ZoneIcon icon={props.zone.icon} size={18} />
        </span>
      <span
        class="zen-capsule__title"
        ref={abbr.setRef}
        aria-label={fullName()}
        style={{ "font-size": `${abbr.fontSize()}px` }}
      >
        <Tooltip content={fullName()} disabled={abbr.tooltipDisabled()}>
          {abbr.text()}
        </Tooltip>
      </span>
      <span class="zen-capsule__badge">{props.zone.items.length}</span>
    </div>
  );
};

export default ZenCapsule;
