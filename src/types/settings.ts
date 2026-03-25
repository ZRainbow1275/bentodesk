// BentoDesk Settings types — mirrors Rust data model exactly

export type Theme = "Dark" | "Light" | "System";

export interface AppSettings {
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
}
