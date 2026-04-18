/**
 * Frontend IPC wrapper for the Tauri v2 updater plugin (Theme A — A1).
 *
 * Commands are registered in `src-tauri/src/lib.rs` and forwarded from
 * `src-tauri/src/commands/updater.rs` → `src-tauri/src/updater`. Events are
 * emitted from the Rust module as typed payloads mirrored below.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface UpdateInfo {
  version: string;
  current_version: string;
  date?: string | null;
  body?: string | null;
}

export interface UpdateProgressPayload {
  chunk_len: number;
  total_bytes: number | null;
}

export interface UpdateErrorPayload {
  kind: string;
  message: string;
}

export async function checkForUpdates(): Promise<UpdateInfo | null> {
  return invoke<UpdateInfo | null>("check_for_updates");
}

export async function downloadUpdate(): Promise<void> {
  return invoke<void>("download_update");
}

export async function installUpdateAndRestart(): Promise<void> {
  return invoke<void>("install_update_and_restart");
}

export async function skipUpdateVersion(version: string): Promise<void> {
  return invoke<void>("skip_update_version", { version });
}

/** Subscribe to download progress events. Returns an unlisten function. */
export function onUpdateProgress(
  handler: (payload: UpdateProgressPayload) => void
): Promise<UnlistenFn> {
  return listen<UpdateProgressPayload>("update:progress", (ev) =>
    handler(ev.payload)
  );
}

export function onUpdateAvailable(
  handler: (payload: UpdateInfo) => void
): Promise<UnlistenFn> {
  return listen<UpdateInfo>("update:available", (ev) => handler(ev.payload));
}

export function onUpdateReady(handler: () => void): Promise<UnlistenFn> {
  return listen("update:ready", () => handler());
}

export function onUpdateError(
  handler: (payload: UpdateErrorPayload) => void
): Promise<UnlistenFn> {
  return listen<UpdateErrorPayload>("update:error", (ev) =>
    handler(ev.payload)
  );
}
