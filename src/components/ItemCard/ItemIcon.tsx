/**
 * ItemIcon — Displays file icon from bentodesk://icon/{encoded_path}.
 * Shows a loading pulse while the icon loads and a fallback emoji on error.
 */
import { Component, createSignal, createEffect, Show } from "solid-js";
import { getIconUrl } from "../../services/ipc";
import "./ItemIcon.css";

interface ItemIconProps {
  path: string;
  iconHash: string;
  isWide: boolean;
}

const ItemIcon: Component<ItemIconProps> = (props) => {
  const [iconSrc, setIconSrc] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal(false);

  createEffect(() => {
    // Track reactive dependencies so the effect re-runs when path/hash change.
    const _path = props.path;
    const _hash = props.iconHash;
    void (async () => {
      setLoading(true);
      setError(false);
      try {
        // Always call getIconUrl which now returns a data: URL with the PNG
        // bytes base64-encoded. This bypasses WebView2 caching issues with
        // the custom bentodesk:// protocol that caused wrong icons to render.
        const url = await getIconUrl(_path);
        setIconSrc(url);
      } catch {
        setError(true);
      } finally {
        setLoading(false);
      }
    })();
  });

  // Icon render size — smaller than the raw 32x32 PNG for visual breathing room.
  // 24px in a 36px container = 6px margin each side — clear, not cropped.
  const renderSize = () => (props.isWide ? 20 : 24);
  // Container provides the breathing room around the icon
  const containerSize = () => (props.isWide ? 28 : 36);

  return (
    <div
      class={`item-icon ${loading() ? "pulse" : ""}`}
      style={{
        width: `${containerSize()}px`,
        height: `${containerSize()}px`,
        "flex-shrink": "0",
      }}
    >
      <Show when={!loading() && !error() && iconSrc()}>
        <img
          class="item-icon__img"
          src={iconSrc()!}
          alt=""
          width={renderSize()}
          height={renderSize()}
          onError={() => setError(true)}
          draggable={false}
        />
      </Show>
      <Show when={error() || (!loading() && !iconSrc())}>
        <span class="item-icon__fallback" style={{ "font-size": `${renderSize() - 4}px` }}>
          {getFallbackEmoji(props.path)}
        </span>
      </Show>
    </div>
  );
};

/** Determine a fallback emoji based on file extension */
function getFallbackEmoji(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    // Documents
    doc: "\u{1F4C4}", docx: "\u{1F4C4}", pdf: "\u{1F4C4}",
    txt: "\u{1F4C3}", md: "\u{1F4C3}", rtf: "\u{1F4C3}",
    xlsx: "\u{1F4CA}", xls: "\u{1F4CA}", csv: "\u{1F4CA}",
    pptx: "\u{1F4CA}", ppt: "\u{1F4CA}",
    // Images
    png: "\u{1F5BC}", jpg: "\u{1F5BC}", jpeg: "\u{1F5BC}",
    gif: "\u{1F5BC}", svg: "\u{1F5BC}", webp: "\u{1F5BC}",
    bmp: "\u{1F5BC}", ico: "\u{1F5BC}",
    // Videos
    mp4: "\u{1F3AC}", avi: "\u{1F3AC}", mkv: "\u{1F3AC}",
    mov: "\u{1F3AC}", wmv: "\u{1F3AC}", webm: "\u{1F3AC}",
    // Audio
    mp3: "\u{1F3B5}", wav: "\u{1F3B5}", flac: "\u{1F3B5}",
    aac: "\u{1F3B5}", ogg: "\u{1F3B5}", m4a: "\u{1F3B5}",
    // Code
    rs: "\u{1F4BB}", js: "\u{1F4BB}", ts: "\u{1F4BB}",
    tsx: "\u{1F4BB}", jsx: "\u{1F4BB}", py: "\u{1F4BB}",
    go: "\u{1F4BB}", java: "\u{1F4BB}", cpp: "\u{1F4BB}",
    c: "\u{1F4BB}", h: "\u{1F4BB}", cs: "\u{1F4BB}",
    html: "\u{1F4BB}", css: "\u{1F4BB}",
    // Archives
    zip: "\u{1F4E6}", rar: "\u{1F4E6}", "7z": "\u{1F4E6}",
    tar: "\u{1F4E6}", gz: "\u{1F4E6}",
    // Executables
    exe: "\u{2699}", msi: "\u{2699}", bat: "\u{2699}",
    cmd: "\u{2699}", ps1: "\u{2699}",
    // Shortcuts
    lnk: "\u{1F517}", url: "\u{1F517}",
  };
  return map[ext] ?? "\u{1F4C1}";
}

export default ItemIcon;
