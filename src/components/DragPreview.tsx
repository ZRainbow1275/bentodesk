/**
 * DragPreview -- Fixed-position overlay that follows the cursor during
 * internal item drags. Shows a glassmorphism card with the item's icon
 * and display name, offset from the cursor so the pointer remains visible.
 *
 * Reads from the `internalDrag()` signal to decide visibility and position.
 * Rendered once in App.tsx as a global overlay.
 */
import { Component, Show, createSignal, createEffect } from "solid-js";
import { internalDrag } from "../services/drag";
import { getIconUrl } from "../services/ipc";
import "./DragPreview.css";

/** Strip .lnk / .url extensions from display names for shortcut files. */
function displayName(name: string): string {
  return name.replace(/\.(lnk|url)$/i, "");
}

const DragPreview: Component = () => {
  const [iconSrc, setIconSrc] = createSignal<string | null>(null);

  // Load icon whenever the dragged item changes
  createEffect(() => {
    const drag = internalDrag();
    if (drag && drag.filePath) {
      void (async () => {
        try {
          const url = await getIconUrl(drag.filePath);
          setIconSrc(url);
        } catch {
          setIconSrc(null);
        }
      })();
    } else {
      setIconSrc(null);
    }
  });

  const previewStyle = () => {
    const drag = internalDrag();
    if (!drag) return {};
    return {
      position: "fixed" as const,
      left: `${drag.cursorX + 12}px`,
      top: `${drag.cursorY + 12}px`,
      "pointer-events": "none" as const,
      "z-index": "9999",
    };
  };

  return (
    <Show when={internalDrag()}>
      <div class="drag-preview" style={previewStyle()}>
        <Show when={iconSrc()}>
          <img
            class="drag-preview__icon"
            src={iconSrc()!}
            alt=""
            width={24}
            height={24}
            draggable={false}
          />
        </Show>
        <span class="drag-preview__name">
          {displayName(internalDrag()?.itemName ?? "")}
        </span>
      </div>
    </Show>
  );
};

export default DragPreview;
