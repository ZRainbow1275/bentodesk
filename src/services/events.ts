/**
 * Event listener setup/teardown functions for Tauri backend events.
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  FileChangedPayload,
  ResolutionChangedPayload,
  SettingsChangedPayload,
} from "../types/events";

export type EventCleanup = UnlistenFn;

/**
 * Listen for file system change events from the backend watcher.
 */
export async function onFileChanged(
  handler: (payload: FileChangedPayload) => void
): Promise<EventCleanup> {
  return listen<FileChangedPayload>("file_changed", (event) => {
    handler(event.payload);
  });
}

/**
 * Listen for resolution/DPI change events from the backend.
 */
export async function onResolutionChanged(
  handler: (payload: ResolutionChangedPayload) => void
): Promise<EventCleanup> {
  return listen<ResolutionChangedPayload>("resolution_changed", (event) => {
    handler(event.payload);
  });
}

/**
 * Listen for settings change events (e.g. from tray menu actions).
 */
export async function onSettingsChanged(
  handler: (payload: SettingsChangedPayload) => void
): Promise<EventCleanup> {
  return listen<SettingsChangedPayload>("settings_changed", (event) => {
    handler(event.payload);
  });
}

// ─── Tray menu events ───────────────────────────────────────

/**
 * Listen for "New Zone" tray menu action.
 */
export async function onTrayNewZone(
  handler: () => void
): Promise<EventCleanup> {
  return listen("tray_new_zone", () => {
    handler();
  });
}

/**
 * Listen for "Settings" tray menu action.
 */
export async function onTraySettings(
  handler: () => void
): Promise<EventCleanup> {
  return listen("tray_settings", () => {
    handler();
  });
}

/**
 * Listen for "About" tray menu action.
 */
export async function onTrayAbout(
  handler: () => void
): Promise<EventCleanup> {
  return listen("tray_about", () => {
    handler();
  });
}

/**
 * Listen for "Auto-Organize" tray menu action.
 */
export async function onTrayAutoOrganize(
  handler: () => void
): Promise<EventCleanup> {
  return listen("tray_auto_organize", () => {
    handler();
  });
}

/**
 * Helper to combine multiple cleanup functions into one.
 */
export function combineCleanups(...cleanups: EventCleanup[]): EventCleanup {
  return () => {
    for (const cleanup of cleanups) {
      cleanup();
    }
  };
}
