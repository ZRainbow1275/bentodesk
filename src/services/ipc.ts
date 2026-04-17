/**
 * Type-safe wrappers around all Tauri IPC invoke commands.
 * Every command from the data model spec is covered here.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  BentoZone,
  BentoItem,
  RelativePosition,
  RelativeSize,
  ZoneUpdate,
  AutoGroupRule,
  FileInfo,
  SuggestedGroup,
} from "../types/zone";
import type { AppSettings, SettingsUpdate } from "../types/settings";
import type {
  SystemInfo,
  MemoryInfo,
  DesktopSnapshot,
  DesktopSourceInfo,
  Checkpoint,
  CheckpointMeta,
} from "../types/system";
import type { JsonTheme } from "../themes/types";
import type { InstalledPlugin } from "../types/plugins";

// ─── Zone Management ─────────────────────────────────────────

export async function createZone(
  name: string,
  icon: string,
  position: RelativePosition,
  expandedSize: RelativeSize
): Promise<BentoZone> {
  return invoke<BentoZone>("create_zone", {
    name,
    icon,
    position,
    expandedSize,
  });
}

export async function updateZone(
  id: string,
  updates: ZoneUpdate
): Promise<BentoZone> {
  return invoke<BentoZone>("update_zone", { id, updates });
}

export async function deleteZone(id: string): Promise<void> {
  return invoke<void>("delete_zone", { id });
}

export async function listZones(): Promise<BentoZone[]> {
  return invoke<BentoZone[]>("list_zones");
}

export async function reorderZones(zoneIds: string[]): Promise<void> {
  return invoke<void>("reorder_zones", { zoneIds });
}

// ─── Item Management ─────────────────────────────────────────

export async function addItem(
  zoneId: string,
  path: string
): Promise<BentoItem> {
  return invoke<BentoItem>("add_item", { zoneId, path });
}

export async function removeItem(
  zoneId: string,
  itemId: string
): Promise<void> {
  return invoke<void>("remove_item", { zoneId, itemId });
}

export async function moveItem(
  fromZoneId: string,
  toZoneId: string,
  itemId: string
): Promise<void> {
  return invoke<void>("move_item", { fromZoneId, toZoneId, itemId });
}

export async function reorderItems(
  zoneId: string,
  itemIds: string[]
): Promise<void> {
  return invoke<void>("reorder_items", { zoneId, itemIds });
}

export async function toggleItemWide(
  zoneId: string,
  itemId: string
): Promise<BentoItem> {
  return invoke<BentoItem>("toggle_item_wide", { zoneId, itemId });
}

// ─── File Operations ─────────────────────────────────────────

export async function openFile(path: string): Promise<void> {
  return invoke<void>("open_file", { path });
}

export async function revealInExplorer(path: string): Promise<void> {
  return invoke<void>("reveal_in_explorer", { path });
}

export async function getFileInfo(path: string): Promise<FileInfo> {
  return invoke<FileInfo>("get_file_info", { path });
}

// ─── Icon Management ─────────────────────────────────────────

export async function getIconUrl(path: string): Promise<string> {
  return invoke<string>("get_icon_url", { path });
}

export async function preloadIcons(paths: string[]): Promise<void> {
  return invoke<void>("preload_icons", { paths });
}

export async function clearIconCache(): Promise<void> {
  return invoke<void>("clear_icon_cache");
}

// ─── Layout / Snapshot Management ────────────────────────────

export async function saveSnapshot(name: string): Promise<DesktopSnapshot> {
  return invoke<DesktopSnapshot>("save_snapshot", { name });
}

export async function loadSnapshot(id: string): Promise<void> {
  return invoke<void>("load_snapshot", { id });
}

export async function listSnapshots(): Promise<DesktopSnapshot[]> {
  return invoke<DesktopSnapshot[]>("list_snapshots");
}

export async function deleteSnapshot(id: string): Promise<void> {
  return invoke<void>("delete_snapshot", { id });
}

// ─── Timeline (Time-machine, R4-C1) ──────────────────────────

export async function listCheckpoints(): Promise<CheckpointMeta[]> {
  return invoke<CheckpointMeta[]>("list_checkpoints");
}

export async function getCheckpoint(id: string): Promise<Checkpoint> {
  return invoke<Checkpoint>("get_checkpoint", { id });
}

export async function restoreCheckpoint(id: string): Promise<void> {
  return invoke<void>("restore_checkpoint", { id });
}

export async function undoCheckpoint(): Promise<string | null> {
  return invoke<string | null>("undo_checkpoint");
}

export async function redoCheckpoint(): Promise<string | null> {
  return invoke<string | null>("redo_checkpoint");
}

export async function deleteCheckpoint(id: string): Promise<void> {
  return invoke<void>("delete_checkpoint", { id });
}

export async function saveCheckpointPermanent(
  id: string | null,
  label: string | null
): Promise<CheckpointMeta> {
  return invoke<CheckpointMeta>("save_checkpoint_permanent", { id, label });
}

// ─── Smart Grouping ──────────────────────────────────────────

export async function scanDesktop(): Promise<FileInfo[]> {
  return invoke<FileInfo[]>("scan_desktop");
}

export async function suggestGroups(
  files: string[]
): Promise<SuggestedGroup[]> {
  return invoke<SuggestedGroup[]>("suggest_groups", { files });
}

export async function applyAutoGroup(
  zoneId: string,
  rule: AutoGroupRule,
  selectedPaths?: string[]
): Promise<BentoItem[]> {
  return invoke<BentoItem[]>("apply_auto_group", {
    zoneId,
    rule,
    selectedPaths: selectedPaths ?? null,
  });
}

// ─── Rule Preview (R4-C2) ───────────────────────────────────

/**
 * A single desktop-icon highlight target resolved from a file path.
 * Mirrors the Rust `HighlightTarget` in
 * `src-tauri/src/ghost_layer/highlight_overlay.rs`.
 */
export interface HighlightTarget {
  name: string;
  x: number;
  y: number;
  monitor_index: number | null;
}

export interface HighlightPayload {
  targets: HighlightTarget[];
  duration_ms: number;
}

/**
 * Ask the backend to pulse-highlight the given desktop files. Returns the
 * number of targets actually resolved (paths without a known icon position
 * are silently skipped).
 */
export async function highlightDesktopFiles(
  paths: string[],
  durationMs?: number
): Promise<number> {
  return invoke<number>("highlight_desktop_files", {
    paths,
    durationMs: durationMs ?? null,
  });
}

/** Clear any live highlight overlays immediately. */
export async function clearDesktopHighlights(): Promise<void> {
  return invoke<void>("clear_desktop_highlights");
}

/**
 * Automatically add a new desktop file to zones whose auto_group rule matches.
 * Called when the file watcher detects a new file on the desktop.
 * Returns (zoneId, item) pairs for each zone the file was added to.
 */
export async function autoGroupNewFile(
  filePath: string
): Promise<[string, BentoItem][]> {
  return invoke<[string, BentoItem][]>("auto_group_new_file", { filePath });
}

// ─── Settings ────────────────────────────────────────────────

export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export async function updateSettings(
  updates: SettingsUpdate
): Promise<AppSettings> {
  return invoke<AppSettings>("update_settings", { updates });
}

// ─── System ──────────────────────────────────────────────────

export async function getSystemInfo(): Promise<SystemInfo> {
  return invoke<SystemInfo>("get_system_info");
}

export async function getDesktopSources(): Promise<DesktopSourceInfo[]> {
  return invoke<DesktopSourceInfo[]>("get_desktop_sources");
}

export async function startDrag(filePaths: string[]): Promise<string> {
  return invoke<string>("start_drag", { filePaths });
}

export async function getMemoryUsage(): Promise<MemoryInfo> {
  return invoke<MemoryInfo>("get_memory_usage");
}

// ─── JSON Theme Plugin ──────────────────────────────────────

export async function listThemes(): Promise<JsonTheme[]> {
  return invoke<JsonTheme[]>("list_themes");
}

export async function getTheme(id: string): Promise<JsonTheme> {
  return invoke<JsonTheme>("get_theme", { id });
}

export async function getActiveTheme(): Promise<JsonTheme> {
  return invoke<JsonTheme>("get_active_theme");
}

export async function setActiveTheme(id: string): Promise<JsonTheme> {
  return invoke<JsonTheme>("set_active_theme", { id });
}

// ─── Plugin Management ──────────────────────────────────────

export async function listPlugins(): Promise<InstalledPlugin[]> {
  return invoke<InstalledPlugin[]>("list_plugins");
}

export async function installPlugin(path: string): Promise<InstalledPlugin> {
  return invoke<InstalledPlugin>("install_plugin", { path });
}

export async function uninstallPlugin(id: string): Promise<void> {
  return invoke<void>("uninstall_plugin", { id });
}

export async function togglePlugin(id: string, enabled: boolean): Promise<InstalledPlugin> {
  return invoke<InstalledPlugin>("toggle_plugin", { id, enabled });
}

// ─── Stealth / Dotfolder Visibility (R3) ────────────────────

export interface StealthStatus {
  applied: boolean;
  last_error: string | null;
  retry_count: number;
  schema_version: string;
  mirror_healthy: boolean;
}

export interface OneDriveExclusionCheck {
  needed: boolean;
  desktop_path: string | null;
  exclusion_hint: string;
  guide_url: string;
}

export async function getStealthStatus(): Promise<StealthStatus> {
  return invoke<StealthStatus>("get_stealth_status");
}

export async function reapplyStealth(): Promise<StealthStatus> {
  return invoke<StealthStatus>("reapply_stealth");
}

export async function checkOneDriveExclusionNeeded(): Promise<OneDriveExclusionCheck> {
  return invoke<OneDriveExclusionCheck>("check_onedrive_exclusion_needed");
}
