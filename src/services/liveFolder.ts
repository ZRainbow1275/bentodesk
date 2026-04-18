/**
 * liveFolder — IPC wrappers for binding a zone to a filesystem folder.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface LiveFolderEntry {
  index: number;
  name: string;
  path: string;
  is_directory: boolean;
  size: number;
  modified_at: string;
}

export async function bindZoneToFolder(zoneId: string, folderPath: string): Promise<void> {
  await invoke("bind_zone_to_folder", { zoneId, folderPath });
}

export async function unbindZoneFolder(zoneId: string): Promise<void> {
  await invoke("unbind_zone_folder", { zoneId });
}

export async function scanLiveFolder(path: string): Promise<LiveFolderEntry[]> {
  return invoke<LiveFolderEntry[]>("scan_live_folder", { path });
}

export async function onZoneLiveRefresh(
  handler: (zoneId: string) => void,
): Promise<UnlistenFn> {
  return listen<string>("zone_live_refresh", (ev) => {
    handler(ev.payload);
  });
}
