/**
 * UpdaterCard — Settings panel section for Theme A updater UX.
 *
 * Surfaces:
 *   • Current app version
 *   • "Check for updates" button + status pill
 *   • Progress bar during download
 *   • Release notes body (markdown rendered as plain text for now)
 *   • "Install and restart" / "Skip this version" / "Later" actions
 *   • Check-frequency dropdown + auto-download toggle (writes settings.updates)
 */
import { Component, Show, createMemo, onCleanup, onMount } from "solid-js";
import { t } from "../../i18n";
import {
  getUpdaterStatus,
  getUpdateInfo,
  getProgressPct,
  getProgressBytes,
  getUpdaterError,
  manualCheck,
  startDownload,
  installAndRestart,
  skipCurrentVersion,
  wireUpdaterEvents,
} from "../../stores/updater";
import { getSettings, updateSettings } from "../../stores/settings";
import type { UpdateCheckFrequency } from "../../types/settings";
import "./UpdaterCard.css";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function statusPillLabel(
  status:
    | "idle"
    | "checking"
    | "available"
    | "downloading"
    | "ready"
    | "error"
): string {
  switch (status) {
    case "idle":
      return t("updaterStatusIdle");
    case "checking":
      return t("updaterStatusChecking");
    case "available":
      return t("updaterStatusAvailable");
    case "downloading":
      return t("updaterStatusDownloading");
    case "ready":
      return t("updaterStatusReady");
    case "error":
      return t("updaterStatusError");
  }
}

const UpdaterCard: Component = () => {
  let unwire: (() => void) | null = null;

  onMount(async () => {
    unwire = await wireUpdaterEvents();
  });
  onCleanup(() => unwire?.());

  const status = createMemo(() => getUpdaterStatus());
  const info = createMemo(() => getUpdateInfo());
  const pct = createMemo(() => getProgressPct());

  const frequency = createMemo<UpdateCheckFrequency>(
    () => getSettings().updates?.check_frequency ?? "Weekly"
  );
  const autoDownload = createMemo<boolean>(
    () => getSettings().updates?.auto_download ?? true
  );

  const onFrequencyChange = async (e: Event) => {
    const target = e.target as HTMLSelectElement;
    await updateSettings({
      updates: { check_frequency: target.value as UpdateCheckFrequency },
    });
  };

  const onAutoDownloadChange = async (e: Event) => {
    const target = e.target as HTMLInputElement;
    await updateSettings({ updates: { auto_download: target.checked } });
  };

  return (
    <section class="updater-card settings-card" aria-labelledby="updater-card-title">
      <h3 id="updater-card-title" class="settings-card-title">
        {t("updaterCardTitle")}
      </h3>

      <div class="updater-row">
        <span class="updater-row-label">{t("updaterStatus")}</span>
        <span class={`updater-status-pill updater-status-${status()}`}>
          {statusPillLabel(status())}
        </span>
      </div>

      <Show when={info()}>
        {(i) => (
          <div class="updater-version-block">
            <div class="updater-version-row">
              <span>{t("updaterAvailableVersion")}:</span>
              <strong>{i().version}</strong>
              <span class="updater-version-current">
                ({t("updaterCurrentVersion")}: {i().current_version})
              </span>
            </div>
            <Show when={i().body}>
              <pre class="updater-release-body" aria-label={t("updaterReleaseNotes")}>
                {i().body}
              </pre>
            </Show>
          </div>
        )}
      </Show>

      <Show when={status() === "downloading"}>
        <div class="updater-progress">
          <progress
            class="updater-progress-bar"
            value={pct() ?? undefined}
            max={100}
            aria-label={t("updaterDownloading")}
          />
          <span class="updater-progress-label">
            {pct() !== null
              ? `${pct()}%`
              : formatBytes(getProgressBytes().downloaded)}
          </span>
        </div>
      </Show>

      <Show when={getUpdaterError()}>
        <p class="updater-error" role="alert">
          {getUpdaterError()}
        </p>
      </Show>

      <div class="updater-actions">
        <Show when={status() === "idle" || status() === "error"}>
          <button type="button" onClick={manualCheck}>
            {t("updaterCheckNow")}
          </button>
        </Show>
        <Show when={status() === "available"}>
          <button type="button" onClick={startDownload}>
            {t("updaterDownload")}
          </button>
          <button type="button" class="updater-secondary" onClick={skipCurrentVersion}>
            {t("updaterSkipVersion")}
          </button>
        </Show>
        <Show when={status() === "ready"}>
          <button type="button" onClick={installAndRestart}>
            {t("updaterInstallRestart")}
          </button>
        </Show>
      </div>

      <div class="updater-prefs">
        <label class="updater-pref-row">
          <span>{t("updaterFrequency")}</span>
          <select value={frequency()} onChange={onFrequencyChange}>
            <option value="Daily">{t("updaterFrequencyDaily")}</option>
            <option value="Weekly">{t("updaterFrequencyWeekly")}</option>
            <option value="Manual">{t("updaterFrequencyManual")}</option>
          </select>
        </label>
        <label class="updater-pref-row">
          <input
            type="checkbox"
            checked={autoDownload()}
            onChange={onAutoDownloadChange}
          />
          <span>{t("updaterAutoDownload")}</span>
        </label>
      </div>
    </section>
  );
};

export default UpdaterCard;
