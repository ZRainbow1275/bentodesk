/**
 * ZoneIcon — Wrapper component that renders an SVG zone icon by name,
 * or falls back to rendering the raw string as emoji text.
 *
 * Usage:
 *   <ZoneIcon icon="folder" />          renders the SVG IconFolder
 *   <ZoneIcon icon="folder" size={24} /> renders at 24x24
 *   <ZoneIcon icon="\u{1F4C1}" />        renders the emoji character as fallback
 */
import { Component, Show, createMemo } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ZONE_ICONS } from "./ZoneIcons";

interface ZoneIconProps {
  icon: string;
  size?: number;
  class?: string;
}

const ZoneIcon: Component<ZoneIconProps> = (props) => {
  const iconComponent = createMemo(() => ZONE_ICONS[props.icon] ?? null);
  const size = () => props.size ?? 20;

  return (
    <Show
      when={iconComponent()}
      fallback={
        <span
          class={props.class}
          style={{
            "font-size": `${size()}px`,
            "line-height": "1",
            display: "inline-flex",
            "align-items": "center",
            "justify-content": "center",
            width: `${size()}px`,
            height: `${size()}px`,
          }}
        >
          {props.icon}
        </span>
      }
    >
      <span
        class={props.class}
        style={{
          display: "inline-flex",
          "align-items": "center",
          "justify-content": "center",
          width: `${size()}px`,
          height: `${size()}px`,
        }}
      >
        <Dynamic component={iconComponent()!} />
      </span>
    </Show>
  );
};

export default ZoneIcon;
