// BentoDesk Settings types — mirrors Rust data model exactly

export type Theme = "Dark" | "Light" | "System";
export type SafetyProfile = "Conservative" | "Balanced" | "Expanded";
export type UpdateCheckFrequency = "Daily" | "Weekly" | "Manual";
export type EncryptionMode = "None" | "Dpapi" | "Passphrase";

export interface UpdatesConfig {
  check_frequency: UpdateCheckFrequency;
  auto_download: boolean;
  skipped_version: string | null;
}

export interface EncryptionConfig {
  mode: EncryptionMode;
}

export interface AppSettings {
  /** Numeric settings schema version — bumped every time the on-disk shape changes. */
  schema_version: number;
  version: string;
  ghost_layer_enabled: boolean;
  expand_delay_ms: number;       // Default: 150
  collapse_delay_ms: number;     // Default: 300
  icon_cache_size: number;       // Default: 500
  auto_group_enabled: boolean;
  theme: Theme;
  accent_color: string;          // Default accent hex
  desktop_path: string;
  watch_paths: string[];
  portable_mode: boolean;
  launch_at_startup: boolean;
  show_in_taskbar: boolean;
  safety_profile: SafetyProfile; // Default: Balanced
  active_theme?: string | null;  // JSON theme ID (e.g. "ocean-blue")
  startup_high_priority: boolean;
  crash_restart_enabled: boolean;
  crash_max_retries: number;
  crash_window_secs: number;
  safe_start_after_hibernation: boolean;
  hibernate_resume_delay_ms: number;
  /** Theme A — auto-update preferences. */
  updates: UpdatesConfig;
  /** Theme A — settings encryption preferences. */
  encryption: EncryptionConfig;
  /** D1: show debug overlay with hit-rect / anchor / state visualization. */
  debug_overlay: boolean;
  /**
   * v1.2.1 — how zones reveal from their capsule state.
   *   "hover"  mouse hover triggers expand (default, v1.x behaviour)
   *   "always" zones mount expanded and never auto-collapse (Fences-style)
   *   "click"  single click expands; mouse-leave still collapses
   */
  zone_display_mode?: "hover" | "always" | "click";
}

export interface SettingsUpdate {
  ghost_layer_enabled?: boolean;
  expand_delay_ms?: number;
  collapse_delay_ms?: number;
  icon_cache_size?: number;
  auto_group_enabled?: boolean;
  theme?: Theme;
  accent_color?: string;
  desktop_path?: string;
  watch_paths?: string[];
  portable_mode?: boolean;
  launch_at_startup?: boolean;
  show_in_taskbar?: boolean;
  safety_profile?: SafetyProfile;
  active_theme?: string | null;
  startup_high_priority?: boolean;
  crash_restart_enabled?: boolean;
  crash_max_retries?: number;
  crash_window_secs?: number;
  safe_start_after_hibernation?: boolean;
  hibernate_resume_delay_ms?: number;
  updates?: Partial<UpdatesConfig>;
  encryption?: Partial<EncryptionConfig>;
  debug_overlay?: boolean;
  zone_display_mode?: "hover" | "always" | "click";
}
