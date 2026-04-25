/**
 * #9 SettingsPanel display-mode picker — store-side wiring.
 *
 * The picker UI itself is a radio group inside SettingsPanel.tsx. What we
 * test here is the **end-to-end state flow** that the picker depends on:
 *
 *  1. The settings store exposes `zone_display_mode` and a typed getter.
 *  2. Calling `applySettings(...)` (which is the path the picker takes
 *     when it persists via IPC) flips `getZoneDisplayMode()` immediately,
 *     so every BentoZone reading via that getter reacts the same frame.
 *  3. Default is "hover" — keeps v1.x behaviour for upgrading installs.
 *
 * If any of these break, the picker UI is decorative (changes don't take
 * effect on the canvas), which is the v1.2.2 regression.
 */
import { describe, it, expect, beforeEach, vi } from "vitest";

const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

import {
  applySettings,
  getSettings,
  getZoneDisplayMode,
  updateSettings,
} from "../settings";
import type { AppSettings } from "../../types/settings";

const baseSettings = (): AppSettings => ({
  schema_version: 3,
  version: "1.2.3",
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
  updates: { check_frequency: "Weekly", auto_download: true, skipped_version: null },
  encryption: { mode: "None" },
  debug_overlay: false,
  zone_display_mode: "hover",
});

describe("settings store — zone_display_mode picker", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    applySettings(baseSettings());
  });

  it("default zone_display_mode is hover", () => {
    expect(getZoneDisplayMode()).toBe("hover");
  });

  it("applySettings switches mode synchronously for any subscriber", () => {
    expect(getZoneDisplayMode()).toBe("hover");
    applySettings({ ...baseSettings(), zone_display_mode: "always" });
    expect(getZoneDisplayMode()).toBe("always");
    applySettings({ ...baseSettings(), zone_display_mode: "click" });
    expect(getZoneDisplayMode()).toBe("click");
  });

  it("falls back to hover when payload omits zone_display_mode", () => {
    const partial = baseSettings();
    delete (partial as Partial<AppSettings>).zone_display_mode;
    applySettings(partial as AppSettings);
    expect(getZoneDisplayMode()).toBe("hover");
  });

  it("updateSettings forwards zone_display_mode to backend and applies result", async () => {
    const persisted: AppSettings = {
      ...baseSettings(),
      zone_display_mode: "click",
    };
    mockInvoke.mockResolvedValueOnce(persisted);

    const result = await updateSettings({ zone_display_mode: "click" });
    expect(result).not.toBeNull();
    expect(mockInvoke).toHaveBeenCalledWith("update_settings", {
      updates: { zone_display_mode: "click" },
    });
    expect(getSettings().zone_display_mode).toBe("click");
    expect(getZoneDisplayMode()).toBe("click");
  });
});
