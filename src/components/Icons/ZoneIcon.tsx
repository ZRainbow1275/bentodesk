/**
 * ZoneIcon — Render a zone icon by name.
 *
 * Supports three namespaces in the `icon` prop:
 *   - `"folder"` (bare name)        → built-in ZONE_ICONS registry
 *   - `"lucide:home"`               → LucideDynamic (lazy-loaded SVG)
 *   - `"custom:{uuid}"`             → `<img src="bentodesk://custom-icon/{uuid}">`
 *   - anything else                 → rendered as emoji / text fallback
 */
import { Component, Show, createMemo } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ZONE_ICONS } from "./ZoneIcons";
import LucideDynamic from "./LucideDynamic";

interface ZoneIconProps {
  icon: string;
  size?: number;
  class?: string;
}

const ZoneIcon: Component<ZoneIconProps> = (props) => {
  const size = () => props.size ?? 20;

  const parsed = createMemo(() => {
    const raw = props.icon ?? "";
    if (raw.startsWith("lucide:")) {
      return { kind: "lucide" as const, name: raw.slice(7) };
    }
    if (raw.startsWith("custom:")) {
      return { kind: "custom" as const, uuid: raw.slice(7) };
    }
    const component = ZONE_ICONS[raw] as Component | undefined;
    if (component) return { kind: "builtin" as const, component };
    return { kind: "text" as const, value: raw };
  });

  const wrapperStyle = () => ({
    display: "inline-flex",
    "align-items": "center",
    "justify-content": "center",
    width: `${size()}px`,
    height: `${size()}px`,
  });

  return (
    <Show
      when={parsed().kind !== "text"}
      fallback={
        <span
          class={props.class}
          style={{
            "font-size": `${size()}px`,
            "line-height": "1",
            ...wrapperStyle(),
          }}
        >
          {(parsed() as { kind: "text"; value: string }).value}
        </span>
      }
    >
      <span class={props.class} style={wrapperStyle()}>
        <Show when={parsed().kind === "builtin"}>
          <Dynamic component={(parsed() as { kind: "builtin"; component: Component }).component} />
        </Show>
        <Show when={parsed().kind === "lucide"}>
          <LucideDynamic name={(parsed() as { kind: "lucide"; name: string }).name} size={size()} />
        </Show>
        <Show when={parsed().kind === "custom"}>
          <img
            src={`bentodesk://custom-icon/${(parsed() as { kind: "custom"; uuid: string }).uuid}`}
            alt=""
            width={size()}
            height={size()}
            style={{ "object-fit": "contain" }}
          />
        </Show>
      </span>
    </Show>
  );
};

export default ZoneIcon;
