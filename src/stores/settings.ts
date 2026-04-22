/**
 * Solid.js store for application settings.
 */
import { createSignal } from "solid-js";
import type { AppSettings, SettingsUpdate } from "../types/settings";
import * as ipc from "../services/ipc";

// Default settings used before backend responds
const DEFAULT_SETTINGS: AppSettings = {
  schema_version: 2,
  version: "1.0.0",
  ghost_layer_enabled: true,
  expand_delay_ms: 150,
  collapse_delay_ms: 400,
  icon_cache_size: 500,
  auto_group_enabled: true,
  theme: "Dark",
  accent_color: "#3b82f6",
  desktop_path: "",
  watch_paths: [],
  portable_mode: false,
  launch_at_startup: false,
  show_in_taskbar: false,
  safety_profile: "Balanced",
  startup_high_priority: false,
  crash_restart_enabled: false,
  crash_max_retries: 3,
  crash_window_secs: 10,
  safe_start_after_hibernation: true,
  hibernate_resume_delay_ms: 2000,
  updates: {
    check_frequency: "Weekly",
    auto_download: true,
    skipped_version: null,
  },
  encryption: { mode: "None" },
  debug_overlay: false,
  zone_display_mode: "hover",
};

const [settings, setSettings] = createSignal<AppSettings>(DEFAULT_SETTINGS);
const [settingsLoading, setSettingsLoading] = createSignal(false);
const [settingsError, setSettingsError] = createSignal<string | null>(null);

// ─── Read-only accessors ─────────────────────────────────────

export function getSettings(): AppSettings {
  return settings();
}

export function isSettingsLoading(): boolean {
  return settingsLoading();
}

export function getSettingsError(): string | null {
  return settingsError();
}

export function clearSettingsError(): void {
  setSettingsError(null);
}

// ─── Data loading ────────────────────────────────────────────

export async function loadSettings(): Promise<void> {
  setSettingsLoading(true);
  setSettingsError(null);
  try {
    const loaded = await ipc.getSettings();
    setSettings(loaded);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setSettingsError(message);
  } finally {
    setSettingsLoading(false);
  }
}

// ─── Mutation ────────────────────────────────────────────────

export async function updateSettings(
  updates: SettingsUpdate
): Promise<AppSettings | null> {
  try {
    const updated = await ipc.updateSettings(updates);
    setSettings(updated);
    setSettingsError(null);
    return updated;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setSettingsError(message);
    return null;
  }
}

/**
 * Apply settings directly from an event payload without re-fetching via IPC.
 */
export function applySettings(incoming: AppSettings): void {
  setSettings(incoming);
  setSettingsError(null);
}

// ─── Convenience getters ─────────────────────────────────────

export function getExpandDelay(): number {
  return settings().expand_delay_ms;
}

export function getCollapseDelay(): number {
  return settings().collapse_delay_ms;
}

export function getAccentColor(): string {
  return settings().accent_color;
}

export function getDebugOverlayEnabled(): boolean {
  return import.meta.env.DEV && settings().debug_overlay;
}

/**
 * v1.2.1 — How zones wake from their collapsed capsule state.
 * - "hover"  → mouse-over expands (default, v1.x behaviour)
 * - "always" → zones mount expanded, never auto-collapse (Fences-style)
 * - "click"  → single click expands; mouse-leave still collapses
 */
export function getZoneDisplayMode(): "hover" | "always" | "click" {
  return settings().zone_display_mode ?? "hover";
}
