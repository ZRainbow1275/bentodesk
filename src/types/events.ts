// BentoDesk Event payload types — mirrors Rust event structs exactly

import type { Resolution } from "./system";
import type { AppSettings } from "./settings";

export interface FileChangedPayload {
  event_type: "create" | "modify" | "delete";
  path: string;
  old_path: string | null;
}

export interface ResolutionChangedPayload {
  old_resolution: Resolution;
  new_resolution: Resolution;
  old_dpi: number;
  new_dpi: number;
}

/**
 * Emitted by update_settings command after persisting changes.
 * Payload is the full updated AppSettings object.
 */
export type SettingsChangedPayload = AppSettings;
