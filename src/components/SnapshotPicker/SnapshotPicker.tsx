/**
 * SnapshotPicker — Modal dialog for listing, loading, and deleting layout snapshots.
 * Fetches snapshots via IPC on open. Displays name, date, zone count, resolution.
 * Load applies the snapshot. Delete removes it after confirmation.
 */
import {
  Component,
  Show,
  For,
  createSignal,
  createEffect,
  onMount,
  onCleanup,
} from "solid-js";
import {
  isSnapshotPickerOpen,
  closeSnapshotPicker,
  openTimeline,
} from "../../stores/ui";
import * as ipc from "../../services/ipc";
import { t } from "../../i18n";
import type { DesktopSnapshot } from "../../types/system";
import "./SnapshotPicker.css";

const SnapshotPicker: Component = () => {
  const [snapshots, setSnapshots] = createSignal<DesktopSnapshot[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [confirmDeleteId, setConfirmDeleteId] = createSignal<string | null>(
    null
  );

  // Fetch snapshots when dialog opens
  createEffect(() => {
    if (isSnapshotPickerOpen()) {
      setLoading(true);
      setConfirmDeleteId(null);
      ipc
        .listSnapshots()
        .then((result) => {
          setSnapshots(result);
        })
        .catch((err) => {
          console.error("Failed to list snapshots:", err);
          setSnapshots([]);
        })
        .finally(() => {
          setLoading(false);
        });
    }
  });

  // Escape to close
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && isSnapshotPickerOpen()) {
      closeSnapshotPicker();
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
  });

  const handleLoad = async (id: string) => {
    try {
      await ipc.loadSnapshot(id);
      closeSnapshotPicker();
    } catch (err) {
      console.error("Failed to load snapshot:", err);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await ipc.deleteSnapshot(id);
      setSnapshots((prev) => prev.filter((s) => s.id !== id));
      setConfirmDeleteId(null);
    } catch (err) {
      console.error("Failed to delete snapshot:", err);
    }
  };

  const formatDate = (iso: string): string => {
    try {
      return new Date(iso).toLocaleString();
    } catch {
      return iso;
    }
  };

  return (
    <Show when={isSnapshotPickerOpen()}>
      <div class="snapshot-overlay" onClick={() => closeSnapshotPicker()}>
        <div
          class="snapshot-picker scale-in"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header */}
          <div class="snapshot-picker__header">
            <h2 class="snapshot-picker__title">{t("snapshotPickerTitle")}</h2>
            <button
              class="snapshot-btn snapshot-btn--load"
              onClick={() => {
                closeSnapshotPicker();
                openTimeline();
              }}
              title={t("timelinePanelTitle")}
            >
              {t("timelinePanelTitle")}
            </button>
            <button
              class="snapshot-picker__close"
              onClick={() => closeSnapshotPicker()}
              aria-label={t("snapshotPickerCloseAriaLabel")}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>

          {/* Body */}
          <div class="snapshot-picker__body">
            <Show when={loading()}>
              <div class="snapshot-picker__loading pulse">
                {t("snapshotPickerLoading")}
              </div>
            </Show>

            <Show when={!loading() && snapshots().length === 0}>
              <div class="snapshot-picker__empty">
                {t("snapshotPickerEmpty")}
              </div>
            </Show>

            <Show when={!loading() && snapshots().length > 0}>
              <div class="snapshot-list">
                <For each={snapshots()}>
                  {(snapshot) => (
                    <div class="snapshot-item">
                      <div class="snapshot-item__info">
                        <span class="snapshot-item__name">
                          {snapshot.name}
                        </span>
                        <span class="snapshot-item__meta">
                          {snapshot.zones.length} {t("snapshotPickerZones")}
                          {" \u{2022} "}
                          {snapshot.resolution.width}x{snapshot.resolution.height}
                          {" \u{2022} "}
                          {formatDate(snapshot.captured_at)}
                        </span>
                      </div>
                      <div class="snapshot-item__actions">
                        <Show
                          when={confirmDeleteId() === snapshot.id}
                          fallback={
                            <>
                              <button
                                class="snapshot-btn snapshot-btn--load"
                                onClick={() => void handleLoad(snapshot.id)}
                              >
                                {t("snapshotPickerLoad")}
                              </button>
                              <button
                                class="snapshot-btn snapshot-btn--delete"
                                onClick={() =>
                                  setConfirmDeleteId(snapshot.id)
                                }
                              >
                                {t("snapshotPickerDelete")}
                              </button>
                            </>
                          }
                        >
                          <span class="snapshot-item__confirm-text">
                            {t("snapshotPickerConfirmDelete")}
                          </span>
                          <button
                            class="snapshot-btn snapshot-btn--confirm"
                            onClick={() =>
                              void handleDelete(snapshot.id)
                            }
                          >
                            {t("snapshotPickerConfirmYes")}
                          </button>
                          <button
                            class="snapshot-btn snapshot-btn--cancel"
                            onClick={() => setConfirmDeleteId(null)}
                          >
                            {t("snapshotPickerConfirmNo")}
                          </button>
                        </Show>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default SnapshotPicker;
