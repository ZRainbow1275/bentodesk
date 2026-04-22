/**
 * ItemIcon — Displays file icon via the `bentodesk://icon/{hash}` protocol.
 *
 * Theme B changes:
 * - Directly uses `bentodesk://icon/{iconHash}` as `<img src>` — no more
 *   base64 data URL round-trip through JS. The WebView2 fetches the PNG
 *   streaming bytes from the custom protocol handler, which consults the
 *   hot-tier LRU (or warm-tier disk) without any JS-side buffering.
 * - IntersectionObserver gates both the first extract IPC and the image
 *   src assignment. When the card is scrolled out of view we never pay
 *   for an extract or a texture upload.
 * - On extract failure (HICON returns transparent, file missing, etc.)
 *   we fall back to the emoji heuristic.
 */
import { Component, createEffect, createSignal, onMount, onCleanup, Show, untrack } from "solid-js";
import { getIconUrl } from "../../services/ipc";
import "./ItemIcon.css";

interface ItemIconProps {
  path: string;
  iconHash: string;
  isWide: boolean;
}

/** Margin outside the viewport at which we start warming up the icon. */
const PRELOAD_ROOT_MARGIN = "200px";

const ItemIcon: Component<ItemIconProps> = (props) => {
  let containerEl: HTMLDivElement | undefined;
  let observer: IntersectionObserver | null = null;
  let cancelled = false;
  let hasRetried = false;

  const [visible, setVisible] = createSignal(false);
  const [primed, setPrimed] = createSignal(false);
  const [error, setError] = createSignal(false);
  const [resolvedSrc, setResolvedSrc] = createSignal<string | null>(null);

  const prime = async (force = false) => {
    if ((!force && primed()) || cancelled || !props.path) return;
    try {
      const nextUrl = await getIconUrl(props.path);
      if (cancelled) return;
      setResolvedSrc(nextUrl);
      setError(false);
    } catch {
      if (!cancelled) setError(true);
      return;
    }
    if (!cancelled) setPrimed(true);
  };

  createEffect(() => {
    props.path;
    props.iconHash;
    hasRetried = false;
    setResolvedSrc(
      props.iconHash
        ? `bentodesk://icon/${encodeURIComponent(props.iconHash)}`
        : null,
    );
    setPrimed(false);
    setError(false);
    if (untrack(visible)) {
      void prime(true);
    }
  });

  onMount(() => {
    if (!containerEl || typeof IntersectionObserver === "undefined") {
      // No IO support (e.g. jsdom in tests) — prime immediately.
      setVisible(true);
      void prime(true);
      return;
    }

    observer = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            setVisible(true);
            observer?.disconnect();
            observer = null;
            void prime(true);
            break;
          }
        }
      },
      { rootMargin: PRELOAD_ROOT_MARGIN, threshold: 0.01 },
    );
    observer.observe(containerEl);
  });

  onCleanup(() => {
    cancelled = true;
    observer?.disconnect();
    observer = null;
  });

  const renderSize = () => (props.isWide ? 20 : 24);
  const containerSize = () => (props.isWide ? 28 : 36);

  return (
    <div
      ref={containerEl}
      class={`item-icon ${!primed() && !error() ? "pulse" : ""}`}
      style={{
        width: `${containerSize()}px`,
        height: `${containerSize()}px`,
        "flex-shrink": "0",
      }}
    >
      <Show when={visible() && primed() && !error() ? resolvedSrc() : null} keyed>
        {(currentSrc) => (
          <img
            class="item-icon__img"
            src={currentSrc}
            alt=""
            width={renderSize()}
            height={renderSize()}
            loading="lazy"
            decoding="async"
            onError={() => {
              if (hasRetried || cancelled) {
                setError(true);
                return;
              }
              hasRetried = true;
              setPrimed(false);
              void prime(true);
            }}
            onLoad={() => setPrimed(true)}
            draggable={false}
          />
        )}
      </Show>
      <Show when={error()}>
        <span class="item-icon__fallback" style={{ "font-size": `${renderSize() - 4}px` }}>
          {getFallbackEmoji(props.path)}
        </span>
      </Show>
    </div>
  );
};

function getFallbackEmoji(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    doc: "\u{1F4C4}", docx: "\u{1F4C4}", pdf: "\u{1F4C4}",
    txt: "\u{1F4C3}", md: "\u{1F4C3}", rtf: "\u{1F4C3}",
    xlsx: "\u{1F4CA}", xls: "\u{1F4CA}", csv: "\u{1F4CA}",
    pptx: "\u{1F4CA}", ppt: "\u{1F4CA}",
    png: "\u{1F5BC}", jpg: "\u{1F5BC}", jpeg: "\u{1F5BC}",
    gif: "\u{1F5BC}", svg: "\u{1F5BC}", webp: "\u{1F5BC}",
    bmp: "\u{1F5BC}", ico: "\u{1F5BC}",
    mp4: "\u{1F3AC}", avi: "\u{1F3AC}", mkv: "\u{1F3AC}",
    mov: "\u{1F3AC}", wmv: "\u{1F3AC}", webm: "\u{1F3AC}",
    mp3: "\u{1F3B5}", wav: "\u{1F3B5}", flac: "\u{1F3B5}",
    aac: "\u{1F3B5}", ogg: "\u{1F3B5}", m4a: "\u{1F3B5}",
    rs: "\u{1F4BB}", js: "\u{1F4BB}", ts: "\u{1F4BB}",
    tsx: "\u{1F4BB}", jsx: "\u{1F4BB}", py: "\u{1F4BB}",
    go: "\u{1F4BB}", java: "\u{1F4BB}", cpp: "\u{1F4BB}",
    c: "\u{1F4BB}", h: "\u{1F4BB}", cs: "\u{1F4BB}",
    html: "\u{1F4BB}", css: "\u{1F4BB}",
    zip: "\u{1F4E6}", rar: "\u{1F4E6}", "7z": "\u{1F4E6}",
    tar: "\u{1F4E6}", gz: "\u{1F4E6}",
    exe: "\u{2699}", msi: "\u{2699}", bat: "\u{2699}",
    cmd: "\u{2699}", ps1: "\u{2699}",
    lnk: "\u{1F517}", url: "\u{1F517}",
  };
  return map[ext] ?? "\u{1F4C1}";
}

export default ItemIcon;
