# Toolbar (business surface) — Visual Spec

Source: `bentodesk/src/components/shared/SelectionFloatingBar.tsx` patterns + main app top action row (1.x main App.tsx renders the toolbar inline; no dedicated 1.x file).

- **Geometry:** 40 px tall, full window-content width, 8 px inner gap between icon buttons, 4 px outer padding (matches `bento-nano-widget::Toolbar::default`).
- **Background:** `theme::current().palette.surface` (DARK = `#18181CCC` translucent), no border-radius (sits flush at top of viewport).
- **Children (left → right):** PIN icon-button, SETTINGS icon-button, separator, NEW-ZONE icon-button, AUTO-ORGANIZE icon-button, spacer, TRAY icon-button (right-aligned).
- **Icon buttons:** 32×32 px, 6 px corner radius, `palette.text` tint `#E0E0E6FF`, hover background `palette.hover_overlay` `#FFFFFF14`.
- **Hover transition:** 120 ms `EaseOut` on background opacity (matches `bento-nano-widget::icon_button::HOVER_DURATION_SECS = 0.12`).
- **Active/pressed state:** background → `palette.active_overlay` `#FFFFFF29`, no scale change (1.x has no press animation).
- **Pin-on state:** PIN button background → `palette.accent` `#3366CCFF` when `app_state.is_pinned == true`.
