/**
 * CapsulePicker — Modal for browsing, capturing, and restoring Context Capsules.
 */
import { Component, createSignal, For, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import ZoneIcon from "../Icons/ZoneIcon";
import CapsuleCard from "./CapsuleCard";
import { t } from "../../i18n";
import "./CapsulePicker.css";

export interface ContextCapsule {
  id: string;
  name: string;
  icon: string;
  captured_at: string;
  windows: CapturedWindow[];
}

export interface CapturedWindow {
  title: string;
  class_name: string;
  process_name: string;
  rect: [number, number, number, number];
  is_maximized: boolean;
}

export interface RestoreResult {
  restored: string[];
  pending: string[];
  errors: string[];
}

interface CapsulePickerProps {
  open: boolean;
  onClose: () => void;
}

const CapsulePicker: Component<CapsulePickerProps> = (props) => {
  const [capsules, setCapsules] = createSignal<ContextCapsule[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [newName, setNewName] = createSignal("");
  const [lastResult, setLastResult] = createSignal<RestoreResult | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  async function refresh() {
    try {
      const list = await invoke<ContextCapsule[]>("list_contexts");
      setCapsules(list);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  onMount(() => {
    void refresh();
  });

  async function handleCapture() {
    const nm = newName().trim() || `Capsule ${new Date().toLocaleString()}`;
    setBusy(true);
    setError(null);
    try {
      await invoke<ContextCapsule>("capture_context", { name: nm, icon: null });
      setNewName("");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleRestore(id: string) {
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<RestoreResult>("restore_context", { id });
      setLastResult(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete(id: string) {
    try {
      await invoke("delete_context", { id });
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <Show when={props.open}>
      <div class="capsule-picker__backdrop" onClick={props.onClose}>
        <div class="capsule-picker" onClick={(e) => e.stopPropagation()}>
          <div class="capsule-picker__header">
            <span class="capsule-picker__title">
              <ZoneIcon icon="briefcase" size={18} />
              {t("capsulePickerTitle") || "Context Capsules"}
            </span>
            <button
              class="capsule-picker__close"
              onClick={props.onClose}
              aria-label="Close"
            >
              ×
            </button>
          </div>

          <div class="capsule-picker__capture-row">
            <input
              class="capsule-picker__input"
              placeholder={t("capsulePickerNamePlaceholder") || "Name (e.g. Coding Mode)"}
              value={newName()}
              onInput={(e) => setNewName(e.currentTarget.value)}
              disabled={busy()}
            />
            <button
              class="capsule-picker__capture-btn"
              onClick={handleCapture}
              disabled={busy()}
            >
              {busy()
                ? t("capsulePickerCapturing") || "Capturing…"
                : t("capsulePickerCaptureCurrent") || "Capture current"}
            </button>
          </div>

          <Show when={error()}>
            <div class="capsule-picker__error">{error()}</div>
          </Show>

          <Show when={lastResult()}>
            <div class="capsule-picker__result">
              <div>
                <strong>{t("capsulePickerRestored") || "Restored"}</strong>:{" "}
                {lastResult()!.restored.length}
              </div>
              <Show when={lastResult()!.pending.length > 0}>
                <div>
                  <strong>{t("capsulePickerPending") || "Pending"}</strong>:{" "}
                  {lastResult()!.pending.join(", ")}
                </div>
              </Show>
              <Show when={lastResult()!.errors.length > 0}>
                <div class="capsule-picker__result-errors">
                  {lastResult()!.errors.join("; ")}
                </div>
              </Show>
            </div>
          </Show>

          <div class="capsule-picker__list">
            <Show
              when={capsules().length > 0}
              fallback={
                <div class="capsule-picker__empty">
                  {t("capsulePickerEmpty") || "No capsules yet. Capture your current windows above."}
                </div>
              }
            >
              <For each={capsules()}>
                {(cap) => (
                  <CapsuleCard
                    capsule={cap}
                    onRestore={() => handleRestore(cap.id)}
                    onDelete={() => handleDelete(cap.id)}
                    busy={busy()}
                  />
                )}
              </For>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default CapsulePicker;
