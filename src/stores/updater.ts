/**
 * Solid.js store for the updater lifecycle (Theme A — A1).
 *
 * Single source of truth for the Settings panel + tray bubble. Progress
 * percent is derived, so components do not have to track total/chunk
 * themselves.
 */
import { createMemo, createSignal } from "solid-js";
import * as updater from "../services/updater";
import type { UpdateInfo } from "../services/updater";

export type UpdaterStatus =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "error";

const [status, setStatus] = createSignal<UpdaterStatus>("idle");
const [info, setInfo] = createSignal<UpdateInfo | null>(null);
const [downloaded, setDownloaded] = createSignal(0);
const [total, setTotal] = createSignal<number | null>(null);
const [errorMessage, setErrorMessage] = createSignal<string | null>(null);

const progressPct = createMemo(() => {
  const t = total();
  if (!t || t === 0) return null;
  return Math.min(100, Math.round((downloaded() / t) * 100));
});

export function getUpdaterStatus() {
  return status();
}

export function getUpdateInfo() {
  return info();
}

export function getProgressBytes() {
  return { downloaded: downloaded(), total: total() };
}

export function getProgressPct() {
  return progressPct();
}

export function getUpdaterError() {
  return errorMessage();
}

export async function wireUpdaterEvents(): Promise<() => void> {
  const unlisteners = await Promise.all([
    updater.onUpdateAvailable((payload) => {
      setInfo(payload);
      setStatus("available");
    }),
    updater.onUpdateProgress((payload) => {
      setDownloaded((prev) => prev + payload.chunk_len);
      setTotal(payload.total_bytes ?? null);
      setStatus("downloading");
    }),
    updater.onUpdateReady(() => {
      setStatus("ready");
    }),
    updater.onUpdateError((payload) => {
      setErrorMessage(`${payload.kind}: ${payload.message}`);
      setStatus("error");
    }),
  ]);

  return () => unlisteners.forEach((u) => u());
}

export async function manualCheck(): Promise<void> {
  setStatus("checking");
  setErrorMessage(null);
  try {
    const result = await updater.checkForUpdates();
    if (result) {
      setInfo(result);
      setStatus("available");
    } else {
      setInfo(null);
      setStatus("idle");
    }
  } catch (err) {
    setErrorMessage(err instanceof Error ? err.message : String(err));
    setStatus("error");
  }
}

export async function startDownload(): Promise<void> {
  setStatus("downloading");
  setDownloaded(0);
  setTotal(null);
  setErrorMessage(null);
  try {
    await updater.downloadUpdate();
    setStatus("ready");
  } catch (err) {
    setErrorMessage(err instanceof Error ? err.message : String(err));
    setStatus("error");
  }
}

export async function installAndRestart(): Promise<void> {
  try {
    await updater.installUpdateAndRestart();
  } catch (err) {
    setErrorMessage(err instanceof Error ? err.message : String(err));
    setStatus("error");
  }
}

export async function skipCurrentVersion(): Promise<void> {
  const current = info();
  if (!current) return;
  try {
    await updater.skipUpdateVersion(current.version);
    setStatus("idle");
    setInfo(null);
  } catch (err) {
    setErrorMessage(err instanceof Error ? err.message : String(err));
  }
}

export function resetUpdaterState(): void {
  setStatus("idle");
  setInfo(null);
  setDownloaded(0);
  setTotal(null);
  setErrorMessage(null);
}
