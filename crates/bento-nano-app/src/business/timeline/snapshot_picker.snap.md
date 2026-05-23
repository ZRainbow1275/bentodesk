# SnapshotPicker — Visual Spec

Source: `bentodesk/src/components/SnapshotPicker/SnapshotPicker.tsx` (210 LOC) + `SnapshotPicker.css`.

- **Modal scrim (`.snapshot-overlay`):** full viewport, `rgba(0,0,0,0.5)` bg, flex centered, `z-index: 2000`. Click on overlay (NOT panel) closes.
- **Panel (`.snapshot-picker`):** **440 px wide**, `max-height: 70vh`, `var(--surface-expanded)` bg with `var(--blur-expanded)` backdrop-filter, 1 px `var(--border-expanded)` border, `var(--radius-expanded)` corners, `var(--shadow-expanded)`. Flex column, `overflow: hidden`. Open animation `dialogScaleIn` 200 ms ease-out.
- **Header (`.snapshot-picker__header`):** flex space-between, **52 px tall**, `padding: 0 var(--spacing-xl)`, 1 px `rgba(255,255,255,0.06)` bottom border, `flex-shrink: 0`. Title `var(--font-size-lg)` Semibold. Two right-side controls: "Open Timeline" link button (uses `.snapshot-btn--load` style) + close icon button (32×32 px, `border-radius: 8px`, hover: `rgba(239,68,68,0.2)` bg + `var(--accent-red)` icon).
- **Body (`.snapshot-picker__body`):** `flex: 1`, `overflow-y: auto`, `padding: var(--spacing-lg) var(--spacing-xl)`. 4 px scrollbar with `rgba(255,255,255,0.15)` thumb.
- **Loading / Empty (`.snapshot-picker__loading`, `.snapshot-picker__empty`):** centered, `padding: var(--spacing-xl)`, `var(--font-size-sm)`, `palette.text_muted`. Empty also `font-style: italic`.
- **Snapshot list (`.snapshot-list`):** flex column with `var(--spacing-sm)` gap.
- **Snapshot item (`.snapshot-item`):** flex space-between, `padding: var(--spacing-md)`, `var(--surface-subtle, rgba(255,255,255,0.03))` bg, 1 px `rgba(255,255,255,0.06)` border, `var(--radius-card)` corners. Hover: `rgba(255,255,255,0.06)` bg.
  - Info column (`.snapshot-item__info`): 2 px row gap, `min-width: 0`, `flex: 1`. Name `var(--font-size-md)` Medium `palette.text_primary`, ellipsised. Meta `var(--font-size-xs)` `palette.text_muted` — format `"<n> Zones • <w>x<h> • <date>"`.
  - Actions column (`.snapshot-item__actions`): flex centered, `var(--spacing-xs)` gap, `flex-shrink: 0`, `margin-left: var(--spacing-md)`. **Two states (mutually exclusive):**
    1. Default: "Load" (`.snapshot-btn--load`) + "Delete" (`.snapshot-btn--delete`).
    2. Confirm-delete: confirm-text (`var(--accent-red)` 11 px) + "Yes" (`.snapshot-btn--confirm`) + "No" (`.snapshot-btn--cancel`).
- **Buttons (`.snapshot-btn`):** `padding: 4px 12px`, `border-radius: 6px`, `var(--font-size-xs)` Medium. Modifiers:
  - `--load` → `var(--accent-blue)` bg, white text. Hover: `#4b90f7` bg + drop shadow.
  - `--delete` → `rgba(239,68,68,0.1)` bg, `var(--accent-red)` text. Hover: `rgba(239,68,68,0.2)` bg.
  - `--confirm` → `var(--accent-red)` bg, white text. Hover: `#f05252` bg + drop shadow.
  - `--cancel` → `rgba(255,255,255,0.06)` bg, `palette.text_secondary` text. Hover: `rgba(255,255,255,0.1)` bg.
- **Behaviour:** `createEffect` on `isSnapshotPickerOpen()` → `setLoading(true)` + `ipc.listSnapshots()` + `setConfirmDeleteId(null)`. Item Load → `ipc.loadSnapshot(id)` then close picker. Delete first click sets `confirmDeleteId(id)`; Yes confirms → `ipc.deleteSnapshot(id)` + filter list; No clears `confirmDeleteId`. Open-Timeline button closes picker + opens timeline. Escape closes. Backend dep: `bento-nano-backend::timeline::{list_snapshots, load_snapshot, delete_snapshot}` (T-089). Dispatcher hooks: open via `Command::ShowWindow(WindowKind::SnapshotPicker)`, close via `Command::HideWindow(WindowKind::SnapshotPicker)`. Open-Timeline button issues `Command::HideWindow(WindowKind::SnapshotPicker)` followed by `Command::ShowWindow(WindowKind::Timeline)`.
- **Reduced motion:** scale-in collapses to instant per the modal-wide `prefers-reduced-motion` rule.
