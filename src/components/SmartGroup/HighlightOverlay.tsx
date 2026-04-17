/**
 * HighlightOverlay — renders pulsing circles over desktop icon positions.
 *
 * The ghost-layer webview covers the primary monitor's work area at logical
 * scale, so physical screen coordinates from the backend are translated into
 * viewport space via:
 *
 *   css_x = (physical_x - primary_work.x) / primary_dpi_scale
 *
 * Targets that fall outside the primary monitor are dropped — painting on
 * secondary displays would require additional per-monitor overlay windows,
 * tracked as a follow-up.
 */
import {
  Component,
  For,
  Show,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { HighlightPayload, HighlightTarget } from "../../services/ipc";
import {
  cachedMonitors,
  refreshMonitors,
} from "../../services/geometry";
import type { MonitorInfo } from "../../types/monitor";
import "../../styles/highlight.css";

interface RenderedCircle {
  key: string;
  cssX: number;
  cssY: number;
}

const HighlightOverlay: Component = () => {
  const [circles, setCircles] = createSignal<RenderedCircle[]>([]);
  let clearTimer: number | null = null;
  let unlistenHighlight: UnlistenFn | null = null;
  let unlistenClear: UnlistenFn | null = null;

  const cancelClearTimer = () => {
    if (clearTimer !== null) {
      window.clearTimeout(clearTimer);
      clearTimer = null;
    }
  };

  const primaryMonitor = (): MonitorInfo | null => {
    const list = cachedMonitors();
    if (!list || list.length === 0) return null;
    return list.find((m) => m.is_primary) ?? list[0];
  };

  const toCircle = (
    target: HighlightTarget,
    index: number
  ): RenderedCircle | null => {
    const primary = primaryMonitor();
    // Fallback geometry: whole viewport at 1.0 DPI if we could not query
    // monitors. This keeps highlights visible on single-monitor setups.
    const work = primary?.rect_work ?? {
      x: 0,
      y: 0,
      width: window.innerWidth,
      height: window.innerHeight,
    };
    const dpi = primary?.dpi_scale ?? 1;

    const cssX = (target.x - work.x) / dpi;
    const cssY = (target.y - work.y) / dpi;

    // Drop targets outside the primary work area — those live on other
    // monitors and would need their own overlay window.
    const cssW = work.width / dpi;
    const cssH = work.height / dpi;
    if (cssX < 0 || cssY < 0 || cssX > cssW || cssY > cssH) {
      return null;
    }

    return {
      key: `${target.name}-${index}-${target.x}-${target.y}`,
      cssX,
      cssY,
    };
  };

  onMount(async () => {
    if (!cachedMonitors()) {
      try {
        await refreshMonitors();
      } catch (err) {
        console.warn("HighlightOverlay: refreshMonitors failed", err);
      }
    }

    unlistenHighlight = await listen<HighlightPayload>(
      "highlight_desktop_files",
      (event) => {
        cancelClearTimer();

        const rendered = event.payload.targets
          .map((t, i) => toCircle(t, i))
          .filter((c): c is RenderedCircle => c !== null);

        setCircles(rendered);

        const duration = Math.max(0, event.payload.duration_ms | 0);
        if (duration > 0) {
          clearTimer = window.setTimeout(() => {
            setCircles([]);
            clearTimer = null;
          }, duration);
        }
      }
    );

    unlistenClear = await listen("clear_desktop_highlights", () => {
      cancelClearTimer();
      setCircles([]);
    });
  });

  onCleanup(() => {
    cancelClearTimer();
    unlistenHighlight?.();
    unlistenClear?.();
  });

  return (
    <Show when={circles().length > 0}>
      <div class="desktop-highlight-layer" aria-hidden="true">
        <For each={circles()}>
          {(c) => (
            <div
              class="desktop-highlight-circle"
              style={{ left: `${c.cssX}px`, top: `${c.cssY}px` }}
            />
          )}
        </For>
      </div>
    </Show>
  );
};

export default HighlightOverlay;
