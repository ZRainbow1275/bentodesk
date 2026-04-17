/**
 * TimelinePanel — R4-C1 time-machine slider.
 *
 * Horizontal draggable timeline of desktop layout checkpoints.
 * Markers: auto captures as small dots, manual pins as larger labeled markers.
 * Dragging the scrubber previews the delta summary; release confirms restore.
 */
import {
  Component,
  Show,
  For,
  createSignal,
  createEffect,
  onMount,
  onCleanup,
  createMemo,
} from "solid-js";
import type { CheckpointMeta, Checkpoint } from "../../types/system";
import type { BentoZone } from "../../types/zone";
import * as ipc from "../../services/ipc";
import { isTimelineOpen, closeTimeline } from "../../stores/ui";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { t } from "../../i18n";
import "./TimelinePanel.css";

const TimelinePanel: Component = () => {
  const [metas, setMetas] = createSignal<CheckpointMeta[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [hoverIndex, setHoverIndex] = createSignal<number | null>(null);
  const [dragIndex, setDragIndex] = createSignal<number | null>(null);
  const [previewCache, setPreviewCache] = createSignal<
    Record<string, Checkpoint>
  >({});

  let timelineUpdatedUnlisten: UnlistenFn | null = null;

  const refresh = async () => {
    setLoading(true);
    try {
      const list = await ipc.listCheckpoints();
      // Newest first for display.
      list.sort((a, b) => b.captured_at.localeCompare(a.captured_at));
      setMetas(list);
    } catch (err) {
      console.error("Failed to list checkpoints:", err);
      setMetas([]);
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    if (isTimelineOpen()) {
      void refresh();
    }
  });

  onMount(async () => {
    // Refresh the panel whenever the backend emits `timeline_updated`.
    timelineUpdatedUnlisten = await listen<string>("timeline_updated", () => {
      if (isTimelineOpen()) {
        void refresh();
      }
    });

    document.addEventListener("keydown", handleKey);
  });

  onCleanup(() => {
    timelineUpdatedUnlisten?.();
    document.removeEventListener("keydown", handleKey);
  });

  const handleKey = (e: KeyboardEvent) => {
    if (!isTimelineOpen()) return;
    if (e.key === "Escape") {
      closeTimeline();
    }
  };

  const reversed = createMemo(() => {
    // For the slider we want oldest on the left, newest on the right.
    return [...metas()].sort((a, b) =>
      a.captured_at.localeCompare(b.captured_at)
    );
  });

  const previewFor = async (id: string): Promise<Checkpoint | null> => {
    const cached = previewCache()[id];
    if (cached) return cached;
    try {
      const cp = await ipc.getCheckpoint(id);
      setPreviewCache((prev) => ({ ...prev, [id]: cp }));
      return cp;
    } catch (err) {
      console.error("Failed to fetch checkpoint preview:", err);
      return null;
    }
  };

  const handlePointerEnter = (idx: number) => {
    setHoverIndex(idx);
    const meta = reversed()[idx];
    if (meta) void previewFor(meta.id);
  };
  const handlePointerLeave = () => setHoverIndex(null);

  const handleSliderInput = (e: InputEvent) => {
    const target = e.currentTarget as HTMLInputElement;
    const idx = Number(target.value);
    setDragIndex(idx);
    const meta = reversed()[idx];
    if (meta) void previewFor(meta.id);
  };

  const handleSliderRelease = async () => {
    const idx = dragIndex();
    setDragIndex(null);
    if (idx === null) return;
    const meta = reversed()[idx];
    if (!meta) return;
    try {
      await ipc.restoreCheckpoint(meta.id);
    } catch (err) {
      console.error("Failed to restore checkpoint:", err);
    }
  };

  const handlePin = async (id: string, e: MouseEvent) => {
    e.stopPropagation();
    try {
      await ipc.saveCheckpointPermanent(id, null);
      await refresh();
    } catch (err) {
      console.error("Failed to pin checkpoint:", err);
    }
  };

  const handleDelete = async (id: string, e: MouseEvent) => {
    e.stopPropagation();
    try {
      await ipc.deleteCheckpoint(id);
      await refresh();
    } catch (err) {
      console.error("Failed to delete checkpoint:", err);
    }
  };

  const handleManualSave = async () => {
    try {
      await ipc.saveCheckpointPermanent(null, "manual save");
      await refresh();
    } catch (err) {
      console.error("Failed to save manual checkpoint:", err);
    }
  };

  const formatTime = (iso: string) => {
    try {
      return new Date(iso).toLocaleString();
    } catch {
      return iso;
    }
  };

  const activeIdx = createMemo(() => {
    const d = dragIndex();
    const h = hoverIndex();
    if (d !== null) return d;
    if (h !== null) return h;
    return reversed().length - 1;
  });

  const activeMeta = createMemo<CheckpointMeta | null>(() => {
    const idx = activeIdx();
    const list = reversed();
    return idx >= 0 && idx < list.length ? list[idx] : null;
  });

  const activePreview = createMemo<Checkpoint | null>(() => {
    const meta = activeMeta();
    if (!meta) return null;
    return previewCache()[meta.id] ?? null;
  });

  return (
    <Show when={isTimelineOpen()}>
      <div class="timeline-overlay" onClick={closeTimeline}>
        <div
          class="timeline-panel scale-in"
          onClick={(e) => e.stopPropagation()}
        >
          <div class="timeline-panel__header">
            <h2 class="timeline-panel__title">
              {t("timelinePanelTitle")}
            </h2>
            <div class="timeline-panel__actions">
              <button
                class="timeline-btn timeline-btn--primary"
                onClick={() => void handleManualSave()}
              >
                {t("timelineSaveNow")}
              </button>
              <button
                class="timeline-panel__close"
                onClick={closeTimeline}
                aria-label={t("timelineCloseAriaLabel")}
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </div>
          </div>

          <div class="timeline-panel__body">
            <Show when={loading()}>
              <div class="timeline-panel__loading pulse">
                {t("timelineLoading")}
              </div>
            </Show>

            <Show when={!loading() && reversed().length === 0}>
              <div class="timeline-panel__empty">
                {t("timelineEmpty")}
              </div>
            </Show>

            <Show when={!loading() && reversed().length > 0}>
              <div class="timeline-slider-wrap">
                <input
                  class="timeline-slider"
                  type="range"
                  min={0}
                  max={Math.max(reversed().length - 1, 0)}
                  value={activeIdx()}
                  onInput={handleSliderInput}
                  onChange={() => void handleSliderRelease()}
                />
                <div class="timeline-markers">
                  <For each={reversed()}>
                    {(meta, idx) => (
                      <button
                        type="button"
                        class={
                          "timeline-marker" +
                          (meta.pinned ? " timeline-marker--pinned" : "") +
                          (idx() === activeIdx()
                            ? " timeline-marker--active"
                            : "")
                        }
                        style={{
                          left:
                            reversed().length > 1
                              ? `${(idx() / (reversed().length - 1)) * 100}%`
                              : "50%",
                        }}
                        onPointerEnter={() => handlePointerEnter(idx())}
                        onPointerLeave={handlePointerLeave}
                        onClick={() => {
                          setDragIndex(idx());
                          void handleSliderRelease();
                        }}
                        title={`${formatTime(meta.captured_at)} · ${meta.delta_summary}`}
                      >
                        <Show when={meta.pinned} fallback={<span class="dot" />}>
                          <span class="pin-label">★</span>
                        </Show>
                      </button>
                    )}
                  </For>
                </div>
              </div>

              <div class="timeline-details">
                <Show when={activeMeta()} fallback={<div class="timeline-hint">{t("timelineHoverHint")}</div>}>
                  <div class="timeline-details__card">
                    <div class="timeline-details__row">
                      <span class="timeline-details__time">
                        {formatTime(activeMeta()!.captured_at)}
                      </span>
                      <Show when={activeMeta()!.pinned}>
                        <span class="timeline-details__pinned">
                          ★ {t("timelinePinned")}
                        </span>
                      </Show>
                    </div>
                    <div class="timeline-details__delta">
                      {activeMeta()!.delta_summary || t("timelineNoChange")}
                    </div>
                    <div class="timeline-details__trigger">
                      {t("timelineTriggerPrefix")} {activeMeta()!.trigger || "—"}
                    </div>

                    <Show when={activePreview()}>
                      <ZonesThumbnail zones={activePreview()!.snapshot.zones} />
                    </Show>

                    <div class="timeline-details__buttons">
                      <button
                        class="timeline-btn"
                        onClick={(e) =>
                          void handlePin(activeMeta()!.id, e)
                        }
                        disabled={activeMeta()!.pinned}
                      >
                        {t("timelinePinButton")}
                      </button>
                      <button
                        class="timeline-btn timeline-btn--danger"
                        onClick={(e) =>
                          void handleDelete(activeMeta()!.id, e)
                        }
                      >
                        {t("timelineDeleteButton")}
                      </button>
                    </div>
                  </div>
                </Show>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
};

// ─── Client-side thumbnail ──────────────────────────────────

/**
 * Render zones into a lightweight miniature. No image roundtrip: we just draw
 * coloured rectangles proportional to each zone's relative position/size.
 */
const ZonesThumbnail: Component<{ zones: BentoZone[] }> = (props) => {
  return (
    <div class="timeline-thumbnail">
      <div class="timeline-thumbnail__canvas">
        <For each={props.zones}>
          {(zone) => (
            <div
              class="timeline-thumbnail__zone"
              style={{
                left: `${zone.position.x_percent}%`,
                top: `${zone.position.y_percent}%`,
                width: `${Math.max(zone.expanded_size.w_percent, 2)}%`,
                height: `${Math.max(zone.expanded_size.h_percent, 2)}%`,
                "background-color": zone.accent_color ?? "var(--color-accent, #6488ff)",
              }}
              title={`${zone.name} — ${zone.items.length} items`}
            />
          )}
        </For>
      </div>
    </div>
  );
};

export default TimelinePanel;
