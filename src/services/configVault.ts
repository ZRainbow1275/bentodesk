/**
 * Frontend IPC wrapper for settings backup + encryption (Theme A — A2, A3).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface BackupEntry {
  id: string;
  path: string;
  created_at: string;
  size_bytes: number;
}

export type EncryptionMode = "None" | "Dpapi" | "Passphrase";

export type EncryptionModeRequest =
  | { kind: "none" }
  | { kind: "dpapi" }
  | { kind: "passphrase"; passphrase: string };

export async function listBackups(): Promise<BackupEntry[]> {
  return invoke<BackupEntry[]>("list_settings_backups");
}

export async function createBackup(): Promise<string> {
  return invoke<string>("create_settings_backup");
}

export async function restoreBackup(backupId: string): Promise<void> {
  return invoke<void>("restore_settings_backup", { backupId });
}

export async function setEncryptionMode(
  request: EncryptionModeRequest
): Promise<void> {
  return invoke<void>("set_encryption_mode", { request });
}

export async function verifyPassphrase(passphrase: string): Promise<boolean> {
  return invoke<boolean>("verify_passphrase", { passphrase });
}

export function onBackupCreated(
  handler: (payload: { path: string; timestamp: string }) => void
): Promise<UnlistenFn> {
  return listen<{ path: string; timestamp: string }>("backup:created", (ev) =>
    handler(ev.payload)
  );
}
