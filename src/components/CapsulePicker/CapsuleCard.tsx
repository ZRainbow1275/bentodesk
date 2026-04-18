/**
 * CapsuleCard — Single capsule row inside CapsulePicker.
 */
import { Component, For, Show, createSignal } from "solid-js";
import ZoneIcon from "../Icons/ZoneIcon";
import type { ContextCapsule } from "./CapsulePicker";
import { t } from "../../i18n";

interface Props {
  capsule: ContextCapsule;
  onRestore: () => void;
  onDelete: () => void;
  busy: boolean;
}

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

const CapsuleCard: Component<Props> = (props) => {
  const [showDetail, setShowDetail] = createSignal(false);

  return (
    <div class="capsule-card">
      <button
        class="capsule-card__main"
        onClick={() => setShowDetail(!showDetail())}
      >
        <span class="capsule-card__icon">
          <ZoneIcon icon={props.capsule.icon} size={20} />
        </span>
        <span class="capsule-card__info">
          <span class="capsule-card__name">{props.capsule.name}</span>
          <span class="capsule-card__meta">
            {props.capsule.windows.length}{" "}
            {t("capsulePickerWindows") || "windows"} · {formatDate(props.capsule.captured_at)}
          </span>
        </span>
      </button>
      <div class="capsule-card__actions">
        <button
          class="capsule-card__btn"
          onClick={props.onRestore}
          disabled={props.busy}
          title={t("capsulePickerRestoreTitle") || "Restore windows"}
        >
          {t("capsulePickerRestore") || "Restore"}
        </button>
        <button
          class="capsule-card__btn capsule-card__btn--danger"
          onClick={props.onDelete}
          title={t("capsulePickerDelete") || "Delete"}
        >
          {"\u{1F5D1}"}
        </button>
      </div>
      <Show when={showDetail()}>
        <ul class="capsule-card__windows">
          <For each={props.capsule.windows}>
            {(w) => (
              <li class="capsule-card__window">
                <span class="capsule-card__window-title">{w.title}</span>
                <span class="capsule-card__window-proc">{w.process_name}</span>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
};

export default CapsuleCard;
