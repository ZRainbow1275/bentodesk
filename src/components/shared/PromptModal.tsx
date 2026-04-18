/**
 * PromptModal — Portal-mounted text input dialog.
 *
 * Replaces `window.prompt()` which is unreliable under Tauri's overlay
 * passthrough (the native prompt may never take focus or deliver input
 * when setIgnoreCursorEvents is toggled by the hit-test poller).
 *
 * Mounted via <Portal> into document.body so it's never clipped by the
 * containing context menu's transform / overflow contexts.
 */
import {
  Component,
  Show,
  createEffect,
  createSignal,
  onCleanup,
} from "solid-js";
import { Portal } from "solid-js/web";
import "./PromptModal.css";

export interface PromptModalProps {
  open: boolean;
  title: string;
  defaultValue?: string;
  placeholder?: string;
  okLabel?: string;
  cancelLabel?: string;
  onSubmit: (value: string) => void;
  onCancel: () => void;
}

const PromptModal: Component<PromptModalProps> = (props) => {
  const [value, setValue] = createSignal("");
  let inputRef: HTMLInputElement | undefined;

  // Re-seed the input every time the dialog opens. This way re-using the
  // same <PromptModal> across multiple actions doesn't leak state from a
  // previous invocation.
  createEffect(() => {
    if (props.open) {
      setValue(props.defaultValue ?? "");
      // Focus after layout so the portal node exists in the DOM.
      requestAnimationFrame(() => {
        inputRef?.focus();
        inputRef?.select();
      });
    }
  });

  const onKey = (e: KeyboardEvent) => {
    if (!props.open) return;
    if (e.key === "Escape") {
      e.stopPropagation();
      props.onCancel();
    } else if (e.key === "Enter") {
      e.stopPropagation();
      props.onSubmit(value());
    }
  };

  // Bind at window level so Enter/Escape work regardless of focus order.
  createEffect(() => {
    if (props.open) {
      window.addEventListener("keydown", onKey, true);
      onCleanup(() => window.removeEventListener("keydown", onKey, true));
    }
  });

  return (
    <Show when={props.open}>
      <Portal mount={document.body}>
        <div class="prompt-modal__backdrop" onClick={props.onCancel}>
          <div
            class="prompt-modal__panel scale-in"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 class="prompt-modal__title">{props.title}</h3>
            <input
              ref={inputRef}
              class="prompt-modal__input"
              type="text"
              value={value()}
              placeholder={props.placeholder}
              onInput={(e) => setValue(e.currentTarget.value)}
            />
            <div class="prompt-modal__actions">
              <button
                class="settings-btn settings-btn--secondary"
                onClick={props.onCancel}
              >
                {props.cancelLabel ?? "Cancel"}
              </button>
              <button
                class="settings-btn settings-btn--primary"
                onClick={() => props.onSubmit(value())}
              >
                {props.okLabel ?? "OK"}
              </button>
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  );
};

export default PromptModal;
