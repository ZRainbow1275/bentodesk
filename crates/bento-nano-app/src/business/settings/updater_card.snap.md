# UpdaterCard — Visual Spec

Source: `bentodesk/src/components/Settings/UpdaterCard.tsx` (189 LOC) + `UpdaterCard.css`.

- **Card (`.updater-card`):** flex column with **12 px gap** (no own chrome — composes within `.settings-group`).
- **Title:** inherits `.settings-card-title` from host.
- **Status row (`.updater-row`):** flex space-between, `align-items: center`, 13 px text. Left: label `palette.text_secondary` (`#a0a0b0`); right: status pill.
- **Status pill (`.updater-status-pill`):** `inline-flex` align center, `padding: 2px 10px`, `border-radius: 999px` (full-rounded), **font-size: 11 px Semibold (`font-weight: 600`)**, `rgba(255,255,255,0.08)` bg, `palette.text_secondary` text (default). State modifiers:
  - `--available`, `--ready` → `rgba(52,211,153,0.16)` bg + `#34d399` text
  - `--downloading`, `--checking` → `rgba(96,165,250,0.16)` bg + `#60a5fa` text
  - `--error` → `rgba(248,113,113,0.16)` bg + `#f87171` text
  - `--idle` → defaults (no modifier class beyond base)
  Note: the React baseline does NOT animate `downloading` with a stripe — earlier snap drift fabricated this. The `<progress>` element below carries the activity indicator.
- **Version block (`.updater-version-block`):** `rgba(255,255,255,0.04)` bg, `padding: 10px 12px`, `border-radius: 8px`, flex column with 6 px gap. Renders only when `info()` resolves.
  - **Version row (`.updater-version-row`):** flex with 8 px gap, `align-items: baseline`, `flex-wrap: wrap`, 13 px. "Available:" label + `<strong>` version + `(Current: <v>)` (`.updater-version-current` — 12 px `palette.text_muted`).
  - **Release notes (`.updater-release-body`):** `<pre>`, monospaced (`var(--font-mono)`), 12 px, `max-height: 160px`, scrollable, `rgba(0,0,0,0.2)` bg, `border-radius: 6px`, `padding: 8px`, `white-space: pre-wrap`.
- **Progress block (`.updater-progress`):** flex with 10 px gap, `align-items: center`. Visible only during `status() === "downloading"`.
  - `<progress class="updater-progress-bar">` — `flex: 1`, **6 px tall**, browser-styled.
  - Label (`.updater-progress-label`): 12 px `palette.text_secondary`, `min-width: 48px`, right-aligned. Shows `"<pct>%"` when known else `formatBytes(downloaded)`.
- **Error banner (`.updater-error`):** `#f87171` 12 px text, `margin: 0`. Renders when `getUpdaterError()` is set.
- **Actions row (`.updater-actions`):** flex with 8 px gap, `flex-wrap: wrap`. Buttons:
  - Default: `padding: 6px 14px`, 12 px, `border-radius: 6px`, 1 px `rgba(255,255,255,0.12)` border, `rgba(96,165,250,0.18)` bg, `#60a5fa` text. Hover: `rgba(96,165,250,0.3)` bg.
  - `.updater-secondary` modifier: `rgba(255,255,255,0.05)` bg, `palette.text_secondary` text.
  - **Visibility per status (mutually exclusive):**
    - `idle` or `error` → "Check Now" only
    - `available` → "Download" + "Skip Version" (`--secondary`)
    - `ready` → "Install & Restart"
    - `checking`, `downloading` → no buttons
- **Prefs row (`.updater-prefs`):** flex column with 6 px gap, `margin-top: 6px`, `padding-top: 10px`, 1 px `rgba(255,255,255,0.06)` top border.
  - Frequency (`.updater-pref-row`): label 13 px + `<select>` with `Daily`, `Weekly`, `Manual` options. Select: `rgba(255,255,255,0.06)` bg, 1 px `rgba(255,255,255,0.12)` border, `border-radius: 6px`, `padding: 4px 8px`, 12 px.
  - Auto-download (`.updater-pref-row`): `<input type="checkbox">` + label 13 px.
- **Behaviour:** `onMount` → `wireUpdaterEvents()` returns unwire fn; `onCleanup` invokes it. Reactive memos pull from updater store: `status`, `info`, `pct`, `frequency`, `autoDownload`. Buttons call `manualCheck`, `startDownload`, `installAndRestart`, `skipCurrentVersion` from `stores/updater`. Frequency / auto-download mutations call `updateSettings({ updates: { ... } })`. Backend dep: `bento-nano-backend::updater::*` (T-091 — in flight per task #36). Dispatcher hooks: `Command::SetSetting { key: "updates.check_frequency", value: SettingValue::Str(freq.as_wire) }` and `Command::SetSetting { key: "updates.auto_download", value: SettingValue::Bool(checked) }`. Updater status itself is event-driven (no Command), updates flow via a `crossbeam_channel` from the updater background thread.
