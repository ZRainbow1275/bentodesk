/**
 * customIcons — IPC wrappers for uploading / listing / deleting user icons.
 */
import { invoke } from "@tauri-apps/api/core";

export interface CustomIcon {
  uuid: string;
  name: string;
  /** kind: "svg" | "png" | "ico" (ico is converted to png on write). */
  kind: string;
  /** Resolved `bentodesk://custom-icon/{uuid}` URL for use in <img src>. */
  url: string;
  created_at: string;
}

export async function uploadCustomIcon(kind: string, bytes: number[], name: string): Promise<string> {
  return invoke<string>("upload_custom_icon", { kind, bytes, name });
}

export async function listCustomIcons(): Promise<CustomIcon[]> {
  return invoke<CustomIcon[]>("list_custom_icons");
}

export async function deleteCustomIcon(uuid: string): Promise<void> {
  await invoke("delete_custom_icon", { uuid });
}
