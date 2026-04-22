import { Component } from "solid-js";
import type { BentoZone } from "../../types/zone";
import BentoPanel from "./BentoPanel";

interface FocusedZonePreviewProps {
  zone: BentoZone;
  horizontal: "left" | "right";
  vertical: "top" | "bottom";
  onClose: () => void;
}

const FocusedZonePreview: Component<FocusedZonePreviewProps> = (props) => {
  const viewportWidth = () =>
    typeof window !== "undefined" ? window.innerWidth : 1440;
  const viewportHeight = () =>
    typeof window !== "undefined" ? window.innerHeight : 900;

  const width = () => {
    const value = props.zone.expanded_size.w_percent;
    return value > 0 ? `${(value / 100) * viewportWidth()}px` : "360px";
  };

  const height = () => {
    const value = props.zone.expanded_size.h_percent;
    return value > 0 ? `${(value / 100) * viewportHeight()}px` : "420px";
  };

  return (
    <div
      class={`stack-focused-preview stack-focused-preview--${props.horizontal} stack-focused-preview--${props.vertical}`}
      style={{
        width: width(),
        height: height(),
      }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <BentoPanel
        zone={props.zone}
        onHeaderDragStart={() => {}}
        onClose={props.onClose}
      />
    </div>
  );
};

export default FocusedZonePreview;
