/**
 * Global hotkey wrapper on top of `@tauri-apps/plugin-global-shortcut`.
 *
 * Why a wrapper:
 * - The plugin API is imperative (`register`, `unregister`), we want a
 *   declarative `applyBindings(map)` / `clearAll()` surface.
 * - Conflict detection must not explode: the plugin rejects `register`
 *   with `ERROR_HOTKEY_ALREADY_REGISTERED` (1409) when another process
 *   has claimed the accelerator. We surface those as `ConflictError`
 *   entries so the UI can show them inline.
 * - A blacklist of Windows-reserved combinations is filtered **before**
 *   hitting the OS, because otherwise `Win+L` / `Ctrl+Alt+Del` etc.
 *   fail silently and leave the user wondering why the binding didn't
 *   take.
 */
import {
  register as pluginRegister,
  unregister as pluginUnregister,
  unregisterAll as pluginUnregisterAll,
  isRegistered as pluginIsRegistered,
} from "@tauri-apps/plugin-global-shortcut";

/** Action id → handler. Kept module-local so re-binding re-wires the same handler. */
const handlers = new Map<string, () => void>();
/** Action id → accelerator currently registered with the OS. */
const activeBindings = new Map<string, string>();

/**
 * Windows-level reserved chord set. Registering these is either impossible
 * (Win+L locks the workstation before the plugin can see the event) or
 * outright disallowed (Ctrl+Alt+Del is handled by the secure attention
 * sequence). Keeping them client-side lets us reject at the UI level
 * before the plugin layer, which gives a crisp error message.
 */
export const SYSTEM_RESERVED_ACCELERATORS: ReadonlySet<string> = new Set([
  "Super+L",
  "Super+D",
  "Super+Tab",
  "Alt+Tab",
  "Alt+F4",
  "Control+Alt+Delete",
  // Commonly-typed aliases — treat as equivalent for rejection purposes.
  "Win+L",
  "Win+D",
  "Win+Tab",
  "Ctrl+Alt+Del",
  "Ctrl+Alt+Delete",
]);

export interface ConflictError {
  action: string;
  accelerator: string;
  reason: "reserved" | "taken" | "plugin_error";
  message: string;
}

/** Normalize "Win+..." → "Super+..." so reservation check matches. */
export function normalizeAccelerator(acc: string): string {
  return acc
    .split("+")
    .map((part) => {
      const p = part.trim();
      if (/^(Win|Meta)$/i.test(p)) return "Super";
      if (/^Ctrl$/i.test(p)) return "Control";
      if (/^Cmd$/i.test(p)) return "Super";
      if (/^CmdOrCtrl$/i.test(p)) return "Control";
      return p;
    })
    .join("+");
}

export function isReservedAccelerator(acc: string): boolean {
  const norm = normalizeAccelerator(acc);
  if (SYSTEM_RESERVED_ACCELERATORS.has(norm)) return true;
  // Accept a few common user-typed variants directly.
  return SYSTEM_RESERVED_ACCELERATORS.has(acc);
}

/**
 * Register a single (action, accelerator) pair. Returns `null` on success,
 * or a `ConflictError` when the accelerator is either reserved or already
 * owned by another process / app action.
 */
export async function registerBinding(
  action: string,
  accelerator: string,
  handler: () => void
): Promise<ConflictError | null> {
  if (isReservedAccelerator(accelerator)) {
    return {
      action,
      accelerator,
      reason: "reserved",
      message: `Accelerator ${accelerator} is reserved by Windows`,
    };
  }
  try {
    await pluginRegister(accelerator, (event) => {
      if (event.state === "Pressed") {
        handler();
      }
    });
    handlers.set(action, handler);
    activeBindings.set(action, accelerator);
    return null;
  } catch (err) {
    const msg = String(err);
    const taken =
      msg.includes("already") || msg.toLowerCase().includes("registered");
    return {
      action,
      accelerator,
      reason: taken ? "taken" : "plugin_error",
      message: msg,
    };
  }
}

export async function unregisterBinding(action: string): Promise<void> {
  const acc = activeBindings.get(action);
  if (!acc) return;
  try {
    await pluginUnregister(acc);
  } catch (err) {
    console.warn(`Failed to unregister ${acc}:`, err);
  }
  handlers.delete(action);
  activeBindings.delete(action);
}

/**
 * Swap the accelerator for an existing action. Returns the conflict info
 * if the new accelerator can't be taken; the old one stays active in that
 * case so the user is never left without a working binding.
 */
export async function rebindAction(
  action: string,
  newAccelerator: string
): Promise<ConflictError | null> {
  const handler = handlers.get(action);
  if (!handler) {
    return {
      action,
      accelerator: newAccelerator,
      reason: "plugin_error",
      message: `No handler registered for action ${action}`,
    };
  }
  const oldAccelerator = activeBindings.get(action);
  if (oldAccelerator) {
    try {
      await pluginUnregister(oldAccelerator);
    } catch {
      // The plugin silently ignores unknown accelerators, safe to proceed.
    }
    activeBindings.delete(action);
  }
  const result = await registerBinding(action, newAccelerator, handler);
  if (result && oldAccelerator) {
    // Restore the old accelerator so the user isn't left stranded.
    await registerBinding(action, oldAccelerator, handler);
  }
  return result;
}

/**
 * Bulk apply: unregister all current bindings, then register the new map.
 * Returns all conflicts encountered so the UI can render a single toast.
 */
export async function applyBindings(
  map: Record<string, string>,
  handlerFor: (action: string) => (() => void) | undefined
): Promise<ConflictError[]> {
  await clearAll();
  const conflicts: ConflictError[] = [];
  for (const [action, accelerator] of Object.entries(map)) {
    const handler = handlerFor(action);
    if (!handler) continue;
    const err = await registerBinding(action, accelerator, handler);
    if (err) conflicts.push(err);
  }
  return conflicts;
}

export async function clearAll(): Promise<void> {
  try {
    await pluginUnregisterAll();
  } catch (err) {
    console.warn("Failed to clear all global shortcuts:", err);
  }
  handlers.clear();
  activeBindings.clear();
}

/** Currently-active accelerator for an action, if any. */
export function getActiveAccelerator(action: string): string | undefined {
  return activeBindings.get(action);
}

/** Check whether an accelerator is already registered *at the OS level*. */
export async function isAcceleratorRegistered(
  accelerator: string
): Promise<boolean> {
  try {
    return await pluginIsRegistered(accelerator);
  } catch {
    return false;
  }
}
