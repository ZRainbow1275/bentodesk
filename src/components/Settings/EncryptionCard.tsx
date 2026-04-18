/**
 * EncryptionCard — Settings panel section for Theme A — A3 encryption.
 *
 * Lets the user choose between:
 *   • None        — plaintext settings.json (default, backwards compatible)
 *   • DPAPI       — transparent per-user encryption, no passphrase required
 *   • Passphrase  — AES-256-GCM + Argon2id, survives machine migrations
 *
 * Mode switches validate via a probe encrypt/decrypt roundtrip before
 * writing to persisted state, so a mistyped passphrase never locks the user
 * out of their own data.
 */
import { Component, Show, createMemo, createSignal, onMount } from "solid-js";
import { t } from "../../i18n";
import {
  setEncryptionMode,
  verifyPassphrase,
  type EncryptionMode,
} from "../../services/configVault";
import { getSettings, loadSettings } from "../../stores/settings";
import "./EncryptionCard.css";

const EncryptionCard: Component = () => {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [info, setInfo] = createSignal<string | null>(null);
  const [passphrase, setPassphrase] = createSignal("");

  const currentMode = createMemo<EncryptionMode>(() => {
    const mode = getSettings().encryption?.mode;
    return (mode as EncryptionMode | undefined) ?? "None";
  });

  const modeLabel = (mode: EncryptionMode): string => {
    switch (mode) {
      case "None":
        return t("encryptionModeNone");
      case "Dpapi":
        return t("encryptionModeDpapi");
      case "Passphrase":
        return t("encryptionModePassphrase");
    }
  };

  onMount(() => {
    void loadSettings();
  });

  const applyMode = async (mode: EncryptionMode) => {
    setBusy(true);
    setError(null);
    setInfo(null);
    try {
      if (mode === "Passphrase") {
        const pw = passphrase();
        if (!pw) {
          setError(t("encryptionPassphraseRequired"));
          return;
        }
        const ok = await verifyPassphrase(pw);
        if (!ok) {
          setError(t("encryptionPassphraseProbeFailed"));
          return;
        }
        await setEncryptionMode({ kind: "passphrase", passphrase: pw });
      } else if (mode === "Dpapi") {
        await setEncryptionMode({ kind: "dpapi" });
      } else {
        await setEncryptionMode({ kind: "none" });
      }
      await loadSettings();
      setInfo(`${t("encryptionModeApplied")} ${modeLabel(mode)}`);
      setPassphrase("");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section
      class="encryption-card settings-card"
      aria-labelledby="encryption-card-title"
    >
      <h3 id="encryption-card-title" class="settings-card-title">
        {t("encryptionCardTitle")}
      </h3>
      <p class="encryption-card-description">{t("encryptionCardDescription")}</p>

      <div class="encryption-current">
        <span>{t("encryptionCurrentMode")}:</span>
        <strong>{modeLabel(currentMode())}</strong>
      </div>

      <div class="encryption-mode-grid">
        <button
          type="button"
          class={`encryption-mode-btn ${currentMode() === "None" ? "active" : ""}`}
          disabled={busy()}
          onClick={() => applyMode("None")}
        >
          <span class="encryption-mode-title">{t("encryptionModeNone")}</span>
          <span class="encryption-mode-sub">{t("encryptionModeNoneSub")}</span>
        </button>
        <button
          type="button"
          class={`encryption-mode-btn ${currentMode() === "Dpapi" ? "active" : ""}`}
          disabled={busy()}
          onClick={() => applyMode("Dpapi")}
        >
          <span class="encryption-mode-title">{t("encryptionModeDpapi")}</span>
          <span class="encryption-mode-sub">{t("encryptionModeDpapiSub")}</span>
        </button>
        <button
          type="button"
          class={`encryption-mode-btn ${
            currentMode() === "Passphrase" ? "active" : ""
          }`}
          disabled={busy()}
          onClick={() => {
            const pw = passphrase();
            if (!pw) {
              setError(t("encryptionPassphraseRequired"));
              return;
            }
            void applyMode("Passphrase");
          }}
        >
          <span class="encryption-mode-title">{t("encryptionModePassphrase")}</span>
          <span class="encryption-mode-sub">
            {t("encryptionModePassphraseSub")}
          </span>
        </button>
      </div>

      <label class="encryption-passphrase-row">
        <span>{t("encryptionPassphraseLabel")}</span>
        <input
          type="password"
          value={passphrase()}
          onInput={(e) => setPassphrase(e.currentTarget.value)}
          placeholder={t("encryptionPassphrasePlaceholder")}
          autocomplete="new-password"
        />
      </label>

      <p class="encryption-hint">{t("encryptionPassphraseHint")}</p>

      <Show when={error()}>
        <p class="encryption-error" role="alert">
          {error()}
        </p>
      </Show>
      <Show when={info()}>
        <p class="encryption-info" role="status">
          {info()}
        </p>
      </Show>
    </section>
  );
};

export default EncryptionCard;
