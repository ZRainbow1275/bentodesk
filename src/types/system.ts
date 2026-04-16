// BentoDesk System types — mirrors Rust data model exactly

import type { BentoZone } from "./zone";

export interface Resolution {
  width: number;
  height: number;
}

export interface MemoryInfo {
  working_set_bytes: number;
  peak_working_set_bytes: number;
}

export type DesktopSourceKind = "user" | "public" | "onedrive" | "custom";

export interface DesktopSourceInfo {
  path: string;
  kind: DesktopSourceKind;
  watched: boolean;
}

export interface SystemInfo {
  os_version: string;
  resolution: Resolution;
  dpi: number;
  desktop_path: string;
  desktop_sources: DesktopSourceInfo[];
  webview2_version: string | null;
  memory_usage: MemoryInfo;
}

export interface DesktopSnapshot {
  id: string;        // UUID v4
  name: string;
  resolution: Resolution;
  dpi: number;
  zones: BentoZone[];
  captured_at: string; // ISO 8601
}
