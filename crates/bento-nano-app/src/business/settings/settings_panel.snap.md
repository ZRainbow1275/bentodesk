# SettingsPanel — Visual Spec

Source: `bentodesk/src/components/Settings/SettingsPanel.tsx` (910 LOC) + `SettingsPanel.css`.

- **Modal scrim:** full-viewport `palette.scrim` `rgba(0,0,0,0.5)`, click dismisses (`closeSettingsPanel`), Escape key dismisses, `z-index: 2000`.
- **Panel:** centered, **480 px wide** × `max-height: 80vh` (no fixed cap), `var(--radius-expanded)` corners, `var(--surface-expanded)` background with `var(--blur-expanded)` backdrop-filter, 1 px `var(--border-expanded)` border, `var(--shadow-expanded)` drop shadow.
- **Open animation:** `scale-in` keyframe — scale 0.96 → 1.0, opacity 0 → 1, 200 ms `cubic-bezier(0.16, 1, 0.3, 1)` (shared keyframe with all other modals).
- **Header:** **52 px tall**, `padding: 0 var(--spacing-xl)` (horizontal only), 1 px `rgba(255,255,255,0.06)` bottom border, title `var(--font-size-lg)` Semibold `palette.text`, close button 32×32 px right, `border-radius: 8px`, **hover state: `rgba(239,68,68,0.2)` background + `var(--accent-red)` icon tint**.
- **Body:** `flex: 1`, scrollable, `padding: var(--spacing-xl) var(--spacing-xl)`, 4 px scrollbar with `rgba(255,255,255,0.2)` thumb.
- **Footer:** `padding: var(--spacing-lg) var(--spacing-xl)`, right-aligned, `var(--spacing-sm)` gap, 1 px `rgba(255,255,255,0.06)` top border, `flex-shrink: 0`. Save button uses `.settings-btn--primary` (disabled until `dirty==true`).
- **Section title (`.settings-group__title`):** **10 px Semibold UPPERCASE** `palette.text_muted`, `letter-spacing: 1.2px`, `margin-bottom: var(--spacing-md)`. Each `.settings-group` has `margin-bottom: var(--spacing-2xl, 28px)`.
- **Settings row:** flex space-between, `min-height: 42px`, `padding: var(--spacing-xs) 0`. Label `var(--font-size-sm)` `palette.text_secondary`; value `var(--font-size-xs)` `palette.text_muted` tabular-nums.
- **Inline error banner:** `margin: 0 var(--spacing-xl) var(--spacing-md)`, `padding: 10px 12px`, `border-radius: 10px`, 1 px `rgba(239,68,68,0.35)` border, `rgba(239,68,68,0.1)` bg, `#fecaca` text, `var(--font-size-xs)` `line-height: 1.5`.
- **Sections (top → bottom, each is `.settings-group`):** General, Paths, Appearance, Display Mode, Performance, Startup, StealthModeCard, UpdaterCard, BackupCard, EncryptionCard, Plugins.
- **Command surface (dispatcher hooks):** open via `Command::OpenSettings`; close via `Command::CloseSettings`; per-row mutation via `Command::SetSetting { key, value }` where `key` is the dotted-path token (e.g. `"display.mode"`, `"performance.target_fps"`).
- **Reduced motion:** `@media (prefers-reduced-motion: reduce)` collapses transitions/animations to `0.01ms` for the panel chrome + close button + inputs + toggles + swatches + buttons.
