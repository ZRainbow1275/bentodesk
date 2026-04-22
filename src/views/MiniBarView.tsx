/**
 * MiniBarView — Lightweight UI rendered when `?minibar={zone_id}` is present
 * in the window URL. Replaces the full overlay in App.tsx.
 *
 * Interaction model:
 *   - Click an item icon → emit `minibar-launch-item`; the main window listens
 *     and calls `open_file` on the resolved path.
 *   - Drag on the window border snaps to the nearest screen edge on mouseup.
 *   - The close "×" button invokes `unpin_minibar`.
 */
import { Component, createEffect, createSignal, For, onMount, onCleanup, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow, PhysicalPosition, currentMonitor } from "@tauri-apps/api/window";
import ZoneIcon from "../components/Icons/ZoneIcon";
import type { BentoZone, BentoItem } from "../types/zone";
import { getIconUrl, listZones, openFile } from "../services/ipc";
import "./MiniBarView.css";

const SNAP_THRESHOLD = 24;

interface MiniBarViewProps {
  zoneId: string;
}

const MiniBarItemIcon: Component<{ item: BentoItem }> = (props) => {
  const [src, setSrc] = createSignal<string | null>(null);

  createEffect(() => {
    const path = props.item.path;
    props.item.icon_hash;
    let active = true;
    setSrc(null);
    void getIconUrl(path)
      .then((nextSrc) => {
        if (active) {
          setSrc(nextSrc);
        }
      })
      .catch(() => {
        if (active) {
          setSrc(null);
        }
      });
    onCleanup(() => {
      active = false;
    });
  });

  return (
    <Show when={src()} keyed fallback={<span aria-hidden="true">{getMiniBarFallback(props.item)}</span>}>
      {(iconSrc) => (
        <img
          src={iconSrc}
          alt=""
          width={24}
          height={24}
          onError={() => setSrc(null)}
        />
      )}
    </Show>
  );
};

const MiniBarView: Component<MiniBarViewProps> = (props) => {
  const [zone, setZone] = createSignal<BentoZone | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  async function loadZone() {
    try {
      const all = await listZones();
      const z = all.find((zone) => zone.id === props.zoneId);
      if (!z) {
        setZone(null);
        setError("Zone not found");
        return;
      }
      setError(null);
      setZone(z);
    } catch (e) {
      setZone(null);
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  onMount(() => {
    void loadZone();
    setupSnap();
    setupRefreshListener();
  });

  let unlistenRefresh: (() => void) | null = null;
  async function setupRefreshListener() {
    unlistenRefresh = await listen<string>("zone_live_refresh", (ev) => {
      if (ev.payload === props.zoneId) {
        void loadZone();
      }
    });
  }

  function setupSnap() {
    const onMouseUp = async () => {
      try {
        const win = getCurrentWindow();
        const pos = await win.outerPosition();
        const monitor = await currentMonitor();
        if (!monitor) return;

        const { width: mw, height: mh } = monitor.size;
        const { width: sw, height: sh } = await win.outerSize();

        let nx = pos.x;
        let ny = pos.y;
        if (pos.x < SNAP_THRESHOLD) nx = 0;
        if (pos.x + sw > mw - SNAP_THRESHOLD) nx = mw - sw;
        if (pos.y < SNAP_THRESHOLD) ny = 0;
        if (pos.y + sh > mh - SNAP_THRESHOLD) ny = mh - sh;
        if (nx !== pos.x || ny !== pos.y) {
          await win.setPosition(new PhysicalPosition(nx, ny));
        }
      } catch (err) {
        console.warn("Snap failed:", err);
      }
    };

    window.addEventListener("mouseup", onMouseUp);
    onCleanup(() => window.removeEventListener("mouseup", onMouseUp));
  }

  onCleanup(() => {
    unlistenRefresh?.();
  });

  async function handleItemClick(item: BentoItem) {
    try {
      await emit("minibar-launch-item", {
        zone_id: props.zoneId,
        item_id: item.id,
        path: item.path,
      });
    } catch (err) {
      console.warn("emit failed, falling back to direct open:", err);
      try {
        await openFile(item.path);
      } catch (e) {
        console.error("open_file failed:", e);
      }
    }
  }

  async function handleClose() {
    try {
      const label = getCurrentWindow().label;
      await invoke("unpin_minibar", { windowLabel: label });
      await getCurrentWindow().close();
    } catch (err) {
      console.error("Failed to close minibar:", err);
    }
  }

  // Update window title to match zone name for accessibility / taskbar tools.
  createEffect(() => {
    const z = zone();
    if (z) {
      void getCurrentWindow().setTitle(`BentoDesk — ${z.name}`);
    }
  });

  return (
    <div class="minibar" data-tauri-drag-region>
      <div class="minibar__grab" data-tauri-drag-region>
        <Show when={zone()} keyed>
          {(currentZone) => <ZoneIcon icon={currentZone.icon} size={14} />}
        </Show>
      </div>
      <div class="minibar__items">
        <Show when={zone()} keyed fallback={<span class="minibar__loading">…</span>}>
          {(currentZone) => (
            <>
              <For each={currentZone.items.slice(0, 16)}>
                {(item) => (
                  <button
                    class="minibar__item"
                    title={item.name}
                    onClick={() => handleItemClick(item)}
                  >
                    <MiniBarItemIcon item={item} />
                  </button>
                )}
              </For>
              <Show when={currentZone.items.length === 0}>
                <span class="minibar__empty">Empty zone</span>
              </Show>
            </>
          )}
        </Show>
      </div>
      <button class="minibar__close" onClick={handleClose} aria-label="Close">
        ×
      </button>
      <Show when={error()}>
        <div class="minibar__error">{error()}</div>
      </Show>
    </div>
  );
};

function getMiniBarFallback(item: BentoItem): string {
  switch (item.item_type) {
    case "Folder":
      return "\u{1F4C1}";
    case "Shortcut":
      return "\u{1F517}";
    case "Application":
      return "\u{2699}";
    default:
      return "\u{1F4C4}";
  }
}

export default MiniBarView;
