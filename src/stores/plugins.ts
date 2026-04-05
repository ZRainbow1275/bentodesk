/**
 * Solid.js store for plugin management.
 * IPC-first mutation pattern: all state changes go through backend first.
 */
import { createSignal } from "solid-js";
import type { InstalledPlugin } from "../types/plugins";
import {
  listPlugins,
  installPlugin as ipcInstallPlugin,
  uninstallPlugin as ipcUninstallPlugin,
  togglePlugin as ipcTogglePlugin,
} from "../services/ipc";

const [plugins, setPlugins] = createSignal<InstalledPlugin[]>([]);
const [pluginsError, setPluginsError] = createSignal<string | null>(null);
const [pluginsLoading, setPluginsLoading] = createSignal(false);

// ─── Data loading ────────────────────────────────────────────

export async function loadPlugins(): Promise<void> {
  setPluginsLoading(true);
  setPluginsError(null);
  try {
    const loaded = await listPlugins();
    setPlugins(loaded);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setPluginsError(message);
  } finally {
    setPluginsLoading(false);
  }
}

// ─── Mutations ──────────────────────────────────────────────

export async function installPluginAction(path: string): Promise<InstalledPlugin | null> {
  try {
    const installed = await ipcInstallPlugin(path);
    setPlugins((prev) => [...prev, installed]);
    return installed;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setPluginsError(message);
    return null;
  }
}

export async function uninstallPluginAction(id: string): Promise<boolean> {
  try {
    await ipcUninstallPlugin(id);
    setPlugins((prev) => prev.filter((p) => p.id !== id));
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setPluginsError(message);
    return false;
  }
}

export async function togglePluginAction(id: string, enabled: boolean): Promise<boolean> {
  try {
    const updated = await ipcTogglePlugin(id, enabled);
    setPlugins((prev) => prev.map((p) => (p.id === id ? updated : p)));
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setPluginsError(message);
    return false;
  }
}

export { plugins, pluginsError, pluginsLoading };
