/**
 * BackupCard — Settings panel section for Theme A — A2 migration protection.
 *
 * Lists rotated `settings.backup.*.json` entries with restore + create-now
 * actions. Restoring reloads the live AppState so the user does not need to
 * relaunch to see their reverted preferences.
 */
import { Component, For, Show, createSignal, onMount } from "solid-js";
import { t } from "../../i18n";
import {
  listBackups,
  createBackup,
  restoreBackup,
  onBackupCreated,
  type BackupEntry,
} from "../../services/configVault";
import { loadSettings } from "../../stores/settings";
import "./BackupCard.css";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  return `${(bytes / 1024).toFixed(1)} KB`;
}

function formatTimestamp(raw: string): string {
  // raw: "20260418T150000.000Z" → "2026-04-18 15:00:00 UTC"
  const m = raw.match(/^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})/);
  if (!m) return raw;
  return `${m[1]}-${m[2]}-${m[3]} ${m[4]}:${m[5]}:${m[6]} UTC`;
}

const BackupCard: Component = () => {
  const [entries, setEntries] = createSignal<BackupEntry[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [lastActionMessage, setLastActionMessage] = createSignal<string | null>(
    null
  );

  const refresh = async () => {
    try {
      setEntries(await listBackups());
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  onMount(async () => {
    await refresh();
    const unlisten = await onBackupCreated((payload) => {
      setLastActionMessage(`${t("backupCreatedAt")} ${payload.path}`);
      void refresh();
    });
    return () => unlisten();
  });

  const onCreate = async () => {
    setBusy(true);
    setError(null);
    try {
      const path = await createBackup();
      setLastActionMessage(`${t("backupCreatedAt")} ${path}`);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const onRestore = async (entry: BackupEntry) => {
    const confirmMsg = `${t("backupRestoreConfirm")} (${formatTimestamp(
      entry.created_at
    )})`;
    if (!confirm(confirmMsg)) return;
    setBusy(true);
    setError(null);
    try {
      await restoreBackup(entry.id);
      await loadSettings();
      setLastActionMessage(
        `${t("backupRestored")} (${formatTimestamp(entry.created_at)})`
      );
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section class="backup-card settings-card" aria-labelledby="backup-card-title">
      <h3 id="backup-card-title" class="settings-card-title">
        {t("backupCardTitle")}
      </h3>
      <p class="backup-card-description">{t("backupCardDescription")}</p>

      <div class="backup-actions">
        <button type="button" onClick={onCreate} disabled={busy()}>
          {t("backupCreateNow")}
        </button>
      </div>

      <Show when={error()}>
        <p class="backup-error" role="alert">
          {error()}
        </p>
      </Show>
      <Show when={lastActionMessage()}>
        <p class="backup-info" role="status">
          {lastActionMessage()}
        </p>
      </Show>

      <ul class="backup-list" aria-label={t("backupList")}>
        <Show
          when={entries().length > 0}
          fallback={<li class="backup-empty">{t("backupEmpty")}</li>}
        >
          <For each={entries()}>
            {(entry) => (
              <li class="backup-entry">
                <div class="backup-entry-info">
                  <span class="backup-entry-timestamp">
                    {formatTimestamp(entry.created_at)}
                  </span>
                  <span class="backup-entry-size">
                    {formatSize(entry.size_bytes)}
                  </span>
                </div>
                <button
                  type="button"
                  class="backup-restore"
                  disabled={busy()}
                  onClick={() => onRestore(entry)}
                >
                  {t("backupRestore")}
                </button>
              </li>
            )}
          </For>
        </Show>
      </ul>
    </section>
  );
};

export default BackupCard;
