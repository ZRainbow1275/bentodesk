/**
 * ZenCapsule — Collapsed state of a BentoZone.
 * Displays: icon, title, item count badge.
 * 160x48px capsule with glassmorphism.
 */
import { Component } from "solid-js";
import type { BentoZone } from "../../types/zone";
import ZoneIcon from "../Icons/ZoneIcon";
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

  return (
    <div class={capsuleClass()}>
      <span class="zen-capsule__icon">
          <ZoneIcon icon={props.zone.icon} size={18} />
        </span>
      <span class="zen-capsule__title">{props.zone.name}</span>
      <span class="zen-capsule__badge">{props.zone.items.length}</span>
    </div>
  );
};

export default ZenCapsule;
