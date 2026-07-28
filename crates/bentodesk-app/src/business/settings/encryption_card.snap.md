# EncryptionCard — Visual Spec

Source: `bentodesk/src/components/Settings/EncryptionCard.tsx` (164 LOC) + `EncryptionCard.css`.

> **native status:** the §10 card is NOW implemented in native (previously deferred to task #7). Paint lives in `crates/bentodesk-app/src/render.rs` (§10 block) and geometry in `crates/bentodesk-app/src/settings_panel.rs` (`settings_encryption_*_rect` + `SETTINGS_ENCRYPTION_*` consts). The 5 §10 text draws (mode-title 13/600, mode-sub 11/400, current-mode label 13/400, passphrase label 13/400, input mask + placeholder 12/400) were brought to the Tauri sizes via a new no-wrap styled draw (`draw_text_no_wrap_with_style`) on 2026-06-01.
> **2026-06-26 hover status:** native now threads real Settings `WM_MOUSEMOVE` through `SettingsHit::{SelectEncryptionModeNone,Dpapi,Passphrase}` into `AppState::settings_encryption_mode_hover`; inactive mode buttons paint the Tauri hover fill `rgba(96,165,250,0.12)`, while active mode fill `rgba(96,165,250,0.18)` remains stronger.

- **Card:** plain flex column with `gap: 10px` (no chrome — composes within `.settings-group`).
- **Title:** inherits `.settings-card-title` from host.
- **Description:** 12 px `palette.text_secondary` (`#a0a0b0`), `margin: 0`.
- **Current mode display (`.encryption-current`):** 13 px flex row, 6 px gap, `align-items: center`. `<span>` label "Current:" + `<strong>` mode label.
- **Mode grid (`.encryption-mode-grid`):** CSS grid with `grid-template-columns: repeat(auto-fit, minmax(160px, 1fr))`, **8 px gap**. Three buttons: None, DPAPI, Passphrase.
- **Mode button (`.encryption-mode-btn`):** flex column, `padding: 10px 12px`, `border-radius: 8px`, `rgba(255,255,255,0.04)` bg, 1 px `rgba(255,255,255,0.08)` border, inherit color, `text-align: left`, 4 px gap, `cursor: pointer`. Hover (non-disabled): `rgba(96,165,250,0.12)` bg. **Active state:** 1 px `#60a5fa` border + `rgba(96,165,250,0.18)` bg. Disabled: `opacity: 0.5`, `cursor: not-allowed`.
- **Mode button text:** title (`.encryption-mode-title`) `font-weight: 600`, 13 px. Sub (`.encryption-mode-sub`) 11 px `palette.text_muted` `line-height: 1.3`.
- **Passphrase row (`.encryption-passphrase-row`):** flex space-between with 10 px gap, 13 px text. Input flex: 1, `rgba(255,255,255,0.06)` bg, 1 px `rgba(255,255,255,0.12)` border, `border-radius: 6px`, `padding: 6px 10px`, 12 px text, `inherit` color. `type="password"`, `autocomplete="new-password"`, `placeholder={t("encryptionPassphrasePlaceholder")}` (`EncryptionCard.tsx:143`).
- **Hint:** 11 px `palette.text_muted`, `margin: 0`.
- **Status banners:** error `#f87171` 12 px; info `#34d399` 12 px (both `margin: 0`).
- **Behaviour:** the Passphrase button's inline `onClick` (`EncryptionCard.tsx:121-128`) runs a PRE-FLIGHT guard `if (!passphrase()) { setError(encryptionPassphraseRequired); return; }` BEFORE invoking `applyMode("Passphrase")` (the None/DPAPI buttons skip this pre-flight). `applyMode("Passphrase")` then re-validates non-empty passphrase, then `verifyPassphrase()` probe round-trip BEFORE `setEncryptionMode({ kind: "passphrase", passphrase })`. `applyMode("Dpapi"/"None")` direct call. After mutate: `loadSettings()` then info banner. Backend dep: `bentodesk-backend::config_vault::{set_mode, verify_passphrase}` (T-092). Dispatcher hook: emits `Command::SetSetting { key: "encryption.mode", value: SettingValue::Str(EncryptionMode::as_wire) }` so the host reflects the new mode without a full settings reload.
