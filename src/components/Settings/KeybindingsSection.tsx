/**
 * KeybindingsSection — rebind UI for Theme C.
 *
 * Displayed as a modal panel. Each row shows `{label}  {chip}  [Record] [Reset]`.
 * Record mode captures the first non-modifier key release and applies the
 * rebind via `setBinding`. Reserved accelerators and OS-level conflicts are
 * surfaced inline on the row so the user isn't dumped to a separate dialog.
 */
import {
  Component,
  Show,
  For,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import {
  keybindingsState,
  setBinding,
  resetBinding,
  formatAccelerator,
  DEFAULT_KEYBINDINGS,
} from "../../stores/keybindings";
import {
  isKeybindingsPanelOpen,
  closeKeybindingsPanel,
} from "../../stores/ui";
import { t } from "../../i18n";
import type { TranslationKey } from "../../i18n/locales/zh-CN";
import "./KeybindingsSection.css";

const LABEL_MAP: Record<string, TranslationKey> = {
  "app.toggle": "keybindingsActionAppToggle",
  "zone.new": "keybindingsActionZoneNew",
  "zone.duplicate": "keybindingsActionZoneDuplicate",
  "zone.lock-toggle": "keybindingsActionZoneLockToggle",
  "zone.hide-all": "keybindingsActionZoneHideAll",
  "layout.auto-organize": "keybindingsActionLayoutAutoOrganize",
  "layout.reflow": "keybindingsActionLayoutReflow",
  "bulk.open-manager": "keybindingsActionBulkOpen",
  "zone.focus.next": "keybindingsActionZoneFocusNext",
  "zone.focus.prev": "keybindingsActionZoneFocusPrev",
};

interface HandlerProvider {
  (action: string): (() => void) | undefined;
}

interface Props {
  handlerFor: HandlerProvider;
}

const KeybindingsSection: Component<Props> = (props) => {
  const [recording, setRecording] = createSignal<string | null>(null);

  function handleKeydown(e: KeyboardEvent): void {
    const action = recording();
    if (!action) return;
    if (e.key === "Escape") {
      setRecording(null);
      e.preventDefault();
      return;
    }
    const accel = formatAccelerator(e);
    if (!accel) return;
    e.preventDefault();
    e.stopPropagation();
    void setBinding(action, accel).finally(() => setRecording(null));
  }

  onMount(() => window.addEventListener("keydown", handleKeydown, true));
  onCleanup(() => window.removeEventListener("keydown", handleKeydown, true));

  return (
    <Show when={isKeybindingsPanelOpen()}>
      <div class="keybindings-panel__scrim" role="dialog" aria-modal="true">
        <div class="keybindings-panel__card">
          <header class="keybindings-panel__header">
            <h2>{t("keybindingsTitle")}</h2>
            <button
              class="keybindings-panel__close"
              onClick={closeKeybindingsPanel}
              aria-label={t("settingsCloseAriaLabel")}
            >
              ×
            </button>
          </header>
          <ul class="keybindings-panel__list">
            <For each={Object.keys(DEFAULT_KEYBINDINGS)}>
              {(action) => {
                const current = () => keybindingsState().current[action] ?? "";
                const conflict = () => keybindingsState().conflicts[action];
                const labelKey = LABEL_MAP[action];
                return (
                  <li class="keybindings-panel__row">
                    <span class="keybindings-panel__label">
                      {labelKey ? t(labelKey) : action}
                    </span>
                    <span class="keybindings-panel__chip">
                      {recording() === action
                        ? t("keybindingsRecording")
                        : current()}
                    </span>
                    <div class="keybindings-panel__actions">
                      <button
                        class="keybindings-panel__btn"
                        onClick={() => setRecording(action)}
                        disabled={recording() !== null && recording() !== action}
                      >
                        {t("keybindingsRecord")}
                      </button>
                      <button
                        class="keybindings-panel__btn"
                        onClick={() =>
                          void resetBinding(action, props.handlerFor)
                        }
                      >
                        {t("keybindingsReset")}
                      </button>
                    </div>
                    <Show when={conflict()}>
                      <div class="keybindings-panel__conflict">
                        {conflict()!.reason === "reserved"
                          ? t("keybindingsConflictReserved")
                          : t("keybindingsConflictTaken")}
                      </div>
                    </Show>
                  </li>
                );
              }}
            </For>
          </ul>
        </div>
      </div>
    </Show>
  );
};

export default KeybindingsSection;
