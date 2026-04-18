/**
 * LiveFolderBadge — Small glyph rendered next to the zone title when a zone
 * is bound to a live folder. Clicking opens the folder in Explorer; hovering
 * surfaces the bound path.
 */
import { Component, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

interface LiveFolderBadgeProps {
  path: string;
  size?: number;
}

const LiveFolderBadge: Component<LiveFolderBadgeProps> = (props) => {
  const size = () => props.size ?? 14;

  const handleClick = async (ev: MouseEvent) => {
    ev.stopPropagation();
    try {
      await invoke("reveal_in_explorer", { path: props.path });
    } catch (err) {
      console.warn("reveal_in_explorer failed:", err);
    }
  };

  return (
    <Show when={props.path}>
      <span
        class="live-folder-badge"
        title={props.path}
        onClick={handleClick}
        style={{
          display: "inline-flex",
          "align-items": "center",
          "justify-content": "center",
          width: `${size()}px`,
          height: `${size()}px`,
          color: "var(--accent, #60a5fa)",
          cursor: "pointer",
        }}
      >
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          width="100%"
          height="100%"
        >
          <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
          <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
        </svg>
      </span>
    </Show>
  );
};

export default LiveFolderBadge;
