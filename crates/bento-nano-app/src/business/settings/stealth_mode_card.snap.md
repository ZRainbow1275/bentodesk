# StealthModeCard — Visual Spec

Source: `bentodesk/src/components/Settings/StealthModeCard.tsx` (201 LOC) + section-scoped CSS in `SettingsPanel.css`.

- **Card:** renders as a `<section class="settings-group">` — uses the host SettingsPanel's settings-group rhythm + `.settings-row` chrome (no own border/padding).
- **Group title (`.settings-group__title`):** **10 px Semibold UPPERCASE** `palette.text_muted`, `letter-spacing: 1.2px` (matches all other settings-group titles).
- **Status pill (`.stealth-status-pill`):** `flex-shrink: 0`, **font-size: 10 px Semibold UPPERCASE**, `letter-spacing: 0.6px`, `padding: 3px 10px`, `border-radius: 10px`, `cursor: help`. Three modifier classes:
  - `--applied` → `rgba(34,197,94,0.18)` bg + `var(--accent-green, #22c55e)` text
  - `--pending` → `rgba(245,158,11,0.18)` bg + `var(--accent-amber, #f59e0b)` text
  - `--failed`  → `rgba(239,68,68,0.18)` bg + `var(--accent-red, #ef4444)` text
- **Info rows:** standard `.settings-row` (label `var(--font-size-sm)` `palette.text_secondary` left + value `var(--font-size-xs)` `palette.text_muted` tabular-nums right). Rows render in this order, conditionally:
  1. `stealthStatusLabel` + status pill (always when status loaded)
  2. `stealthSchemaVersion` + value
  3. `stealthMirrorHealthy` + Yes/No
  4. `stealthRetryCount` + value (only when `retry_count > 0`)
  5. `stealthLastError` + `<code class="settings-row__desc">` body (only when `last_error` populated; uses column layout `.settings-row--column`)
- **Action row:** standard `.settings-row` containing two buttons:
  - Refresh: `.settings-btn settings-btn--secondary` — `rgba(255,255,255,0.06)` bg, `palette.text_secondary` text.
  - Reapply: `.settings-btn settings-btn--primary` — `var(--accent-blue)` bg, white text, `box-shadow: 0 1px 3px rgba(59,130,246,0.3)`. Hover lifts `transform: translateY(-1px)` + brighter shadow.
  Both 8 px 20 px padding, `border-radius: 8px`, `var(--font-size-sm)` Medium. `disabled={busy}` → `opacity: 0.4`, `cursor: not-allowed`, no transform/shadow.
- **OneDrive warning panel (`.stealth-onedrive-warning`):** flex column with 6 px gap, `padding: 10px 12px`, `margin-top: 8px`, `border-radius: 8px`, `rgba(245,158,11,0.08)` bg, 1 px `rgba(245,158,11,0.35)` border. Renders only when `oneDrive.needed`. Contains:
  - `.settings-row__desc` warning copy (11 px `palette.text_muted`)
  - row with Guide button (`.settings-btn--secondary`) + clipboard URL display (`<code class="settings-row__value">`)
- **Loading fallback:** `<div class="settings-row__desc">…</div>` while `status() === null`.
- **Error feedback:** `.settings-row__desc.settings-dev-section__feedback--error` — `rgba(239,68,68,0.12)` bg + `var(--accent-red)` text, `border-radius: 6px`, `padding: 6px 10px`, 11 px.
- **Behaviour:** `onMount` calls `refresh()` which `Promise.all([getStealthStatus(), checkOneDriveExclusionNeeded()])`. Reapply button: sets `busy=true`, awaits `reapplyStealth()`, updates status, clears `busy`. Guide button: copies `oneDrive.guide_url` to clipboard via `navigator.clipboard.writeText` (no shell-open in this build). Backend dep: `bento-nano-backend::stealth::{get_status, reapply, check_onedrive_exclusion}` (T-094a/b/c — already shipped). Dispatcher hook: Reapply emits no Command (status is read-only/event-driven); the StealthStatus refresh is driven by a backend `stealth_updated` event channel the panel subscribes to via `onMount`.
- **Reduced motion:** inherits SettingsPanel's media query for transitions.
