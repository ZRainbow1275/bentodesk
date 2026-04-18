/**
 * Keybindings store — default accelerator map + persistence + registration.
 *
 * Defaults match `spec-C-hotkeys-bulk.md § C1`. Persistence goes through
 * `localStorage.bentodesk.keybindings` so existing Theme-A settings migration
 * paths don't need to know about this (the backend settings struct stays
 * untouched at v1.2.0; a later cut can promote overrides into settings.json).
 */
import { createSignal } from "solid-js";
import {
  applyBindings,
  rebindAction,
  isReservedAccelerator,
  type ConflictError,
} from "../services/globalHotkeys";

export const DEFAULT_KEYBINDINGS: Record<string, string> = {
  "app.toggle": "Control+Space",
  "zone.new": "Control+Shift+N",
  "zone.duplicate": "Control+Shift+D",
  "zone.lock-toggle": "Control+Shift+L",
  "zone.hide-all": "Control+Shift+H",
  "layout.auto-organize": "Control+Shift+O",
  "layout.reflow": "Control+Shift+R",
  "bulk.open-manager": "Control+Shift+M",
  "zone.focus.next": "Control+]",
  "zone.focus.prev": "Control+[",
};

const STORAGE_KEY = "bentodesk.keybindings.v1";

function loadOverrides(): Record<string, string> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function saveOverrides(overrides: Record<string, string>): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(overrides));
  } catch {
    // quota exceeded → silently drop
  }
}

export interface KeybindingsState {
  /** Resolved current map: `{...defaults, ...overrides}` */
  current: Record<string, string>;
  /** Per-action override (omits defaults; what we persist). */
  overrides: Record<string, string>;
  /** Last conflicts seen on apply, per action. */
  conflicts: Record<string, ConflictError>;
}

const [state, setState] = createSignal<KeybindingsState>({
  current: { ...DEFAULT_KEYBINDINGS, ...loadOverrides() },
  overrides: loadOverrides(),
  conflicts: {},
});

export { state as keybindingsState };

export function getBinding(action: string): string | undefined {
  return state().current[action];
}

/**
 * Register every action with the OS. `handlerFor(action)` returns the
 * frontend callback to run when the accelerator fires. Conflicts are
 * captured into the store so `KeybindingsSection.tsx` can render them
 * inline instead of blocking app startup.
 */
export async function initKeybindings(
  handlerFor: (action: string) => (() => void) | undefined
): Promise<void> {
  const map = state().current;
  const conflicts = await applyBindings(map, handlerFor);
  const byAction: Record<string, ConflictError> = {};
  for (const c of conflicts) byAction[c.action] = c;
  setState((prev) => ({ ...prev, conflicts: byAction }));
}

/**
 * Rebind an action to a new accelerator. Rejects reserved combinations
 * and reflects OS-level conflicts in the store.
 */
export async function setBinding(
  action: string,
  accelerator: string
): Promise<ConflictError | null> {
  if (isReservedAccelerator(accelerator)) {
    const err: ConflictError = {
      action,
      accelerator,
      reason: "reserved",
      message: `Accelerator ${accelerator} is reserved by Windows`,
    };
    setState((prev) => ({
      ...prev,
      conflicts: { ...prev.conflicts, [action]: err },
    }));
    return err;
  }
  const err = await rebindAction(action, accelerator);
  if (err) {
    setState((prev) => ({
      ...prev,
      conflicts: { ...prev.conflicts, [action]: err },
    }));
    return err;
  }
  setState((prev) => {
    const nextOverrides = { ...prev.overrides, [action]: accelerator };
    const nextCurrent = { ...prev.current, [action]: accelerator };
    const nextConflicts = { ...prev.conflicts };
    delete nextConflicts[action];
    saveOverrides(nextOverrides);
    return {
      current: nextCurrent,
      overrides: nextOverrides,
      conflicts: nextConflicts,
    };
  });
  return null;
}

export async function resetBinding(
  action: string,
  handlerFor: (action: string) => (() => void) | undefined
): Promise<ConflictError | null> {
  const fallback = DEFAULT_KEYBINDINGS[action];
  if (!fallback) return null;
  const err = await setBinding(action, fallback);
  if (err) return err;
  // Drop from overrides so future default changes flow through.
  setState((prev) => {
    const { [action]: _drop, ...rest } = prev.overrides;
    saveOverrides(rest);
    return { ...prev, overrides: rest };
  });
  // Re-register handler in case the plugin lost it.
  const handler = handlerFor(action);
  if (handler) {
    void rebindAction(action, fallback);
  }
  return null;
}

/**
 * Format a raw `KeyboardEvent` into a Tauri accelerator string for the
 * Record UI. Keeps modifier order consistent with the plugin's parser.
 */
export function formatAccelerator(event: KeyboardEvent): string | null {
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Control");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");
  const key = event.key;
  if (!key || key === "Control" || key === "Alt" || key === "Shift" || key === "Meta") {
    return null;
  }
  const keyPart =
    key.length === 1
      ? key.toUpperCase()
      : key.startsWith("Arrow")
        ? key.slice(5)
        : key;
  parts.push(keyPart);
  return parts.join("+");
}
