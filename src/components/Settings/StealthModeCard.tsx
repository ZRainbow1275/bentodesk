/**
 * StealthModeCard — Settings panel section for R3 desktop stealth mode.
 *
 * Renders:
 *  • Applied / retry / failed status pill
 *  • Schema version + manifest mirror health
 *  • "Re-apply" button (forces AttrGuard::sweep_root)
 *  • OneDrive redirection warning with link to Microsoft's exclusion docs
 *  • Developer education tooltip explaining the "I can still see .bentodesk"
 *    case when Windows "Hide protected OS files" is disabled.
 *
 * Backed by the `get_stealth_status`, `reapply_stealth`, and
 * `check_onedrive_exclusion_needed` IPC commands.
 */
import { Component, Show, createSignal, onMount } from "solid-js";
import { t } from "../../i18n";
import {
  getStealthStatus,
  reapplyStealth,
  checkOneDriveExclusionNeeded,
  type StealthStatus,
  type OneDriveExclusionCheck,
} from "../../services/ipc";

type StatusLevel = "applied" | "pending" | "failed";

function deriveLevel(s: StealthStatus): StatusLevel {
  if (s.last_error && !s.applied) return "failed";
  if (s.retry_count > 0) return "pending";
  return "applied";
}

const StealthModeCard: Component = () => {
  const [status, setStatus] = createSignal<StealthStatus | null>(null);
  const [oneDrive, setOneDrive] = createSignal<OneDriveExclusionCheck | null>(
    null
  );
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const refresh = async (): Promise<void> => {
    try {
      const [s, od] = await Promise.all([
        getStealthStatus(),
        checkOneDriveExclusionNeeded(),
      ]);
      setStatus(s);
      setOneDrive(od);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleReapply = async (): Promise<void> => {
    setBusy(true);
    try {
      const s = await reapplyStealth();
      setStatus(s);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleOpenGuide = (url: string) => {
    // No shell plugin registered in this build; copy the URL so the user
    // can paste it into their browser. Falls back to a no-op if the
    // clipboard API is unavailable (e.g. inside a non-secure context).
    try {
      void navigator.clipboard?.writeText(url);
    } catch (e) {
      console.warn("clipboard copy failed:", e);
    }
  };

  onMount(() => {
    void refresh();
  });

  const statusLabel = (s: StealthStatus): string => {
    const level = deriveLevel(s);
    if (level === "applied") return t("stealthStatusApplied");
    if (level === "pending") return t("stealthStatusPending");
    return t("stealthStatusFailed");
  };

  return (
    <section class="settings-group">
      <h3 class="settings-group__title">{t("stealthGroupTitle")}</h3>

      <Show
        when={status() !== null}
        fallback={
          <div class="settings-row__desc">…</div>
        }
      >
        {(() => {
          const s = status()!;
          const level = deriveLevel(s);
          return (
            <>
              <div class="settings-row">
                <span class="settings-row__label">
                  {t("stealthStatusLabel")}
                </span>
                <span
                  class={`stealth-status-pill stealth-status-pill--${level}`}
                  aria-live="polite"
                  title={t("stealthDevTooltip")}
                >
                  {statusLabel(s)}
                </span>
              </div>

              <div class="settings-row">
                <span class="settings-row__label">
                  {t("stealthSchemaVersion")}
                </span>
                <span class="settings-row__value">{s.schema_version}</span>
              </div>

              <div class="settings-row">
                <span class="settings-row__label">
                  {t("stealthMirrorHealthy")}
                </span>
                <span class="settings-row__value">
                  {s.mirror_healthy
                    ? t("stealthMirrorHealthyYes")
                    : t("stealthMirrorHealthyNo")}
                </span>
              </div>

              <Show when={s.retry_count > 0}>
                <div class="settings-row">
                  <span class="settings-row__label">
                    {t("stealthRetryCount")}
                  </span>
                  <span class="settings-row__value">{s.retry_count}</span>
                </div>
              </Show>

              <Show when={s.last_error}>
                <div class="settings-row settings-row--column">
                  <span class="settings-row__label">
                    {t("stealthLastError")}
                  </span>
                  <code class="settings-row__desc">{s.last_error}</code>
                </div>
              </Show>

              <div class="settings-row">
                <button
                  class="settings-btn settings-btn--secondary"
                  onClick={() => void refresh()}
                  disabled={busy()}
                >
                  {t("stealthRefreshBtn")}
                </button>
                <button
                  class="settings-btn settings-btn--primary"
                  onClick={() => void handleReapply()}
                  disabled={busy()}
                  title={t("stealthReapplyTooltip")}
                >
                  {busy() ? "…" : t("stealthReapplyBtn")}
                </button>
              </div>
            </>
          );
        })()}
      </Show>

      <Show when={oneDrive()?.needed}>
        <div class="stealth-onedrive-warning" role="alert">
          <div class="settings-row__desc">{t("stealthOneDriveWarning")}</div>
          <div class="settings-row">
            <button
              class="settings-btn settings-btn--secondary"
              onClick={() => handleOpenGuide(oneDrive()!.guide_url)}
              title={oneDrive()!.guide_url}
            >
              {t("stealthOneDriveGuideBtn")}
            </button>
            <code class="settings-row__value">{oneDrive()!.guide_url}</code>
          </div>
        </div>
      </Show>

      <Show when={error()}>
        <div class="settings-row__desc settings-dev-section__feedback settings-dev-section__feedback--error">
          {error()}
        </div>
      </Show>
    </section>
  );
};

export default StealthModeCard;
