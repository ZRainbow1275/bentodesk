# ZoneEditor — Visual Fidelity Snapshot

Per Ruling R3. 1.x source: `bentodesk/src/components/ZoneEditor/ZoneEditor.tsx`
(316 LOC) + `ZoneEditor.css`.

## Geometry

- Modal panel: **400 px wide × max-height 80vh**, `palette.surface` background,
  1 px `palette.border`, 16 px corner radius, scale-in 200 ms ease-out.
- Header row: **52 px tall**, 20 px horizontal padding. Title (`palette.text`,
  16 pt semibold) on the left, 32 × 32 px close button (8 px corner radius,
  hover tint = `palette.danger`) on the right.
- Body: 16 px vertical / 20 px horizontal padding, vertically scrollable
  when content exceeds the 80vh cap. 16 px gap between fields.
- Footer row: 16 px vertical / 20 px horizontal padding. Cancel (left,
  secondary `palette.surface_alt`) + Save (right, primary `palette.accent`),
  8 px gap, both 36 px tall × auto width × 6 px corner radius.

## Fields (top → bottom)

1. **Zone name** — single-line Input, 36 px tall, max 32 chars, placeholder
   reads `palette.text_muted`.
2. **Icon picker row** — 6-column grid of 36 × 36 cells (8 px gap), each cell
   8 px corner radius, hover bg = `palette.hover_overlay`, selected bg =
   `palette.accent` 12% opacity + 2 px `palette.accent` border.
3. **Accent palette swatch row** — leading "None" swatch (diagonal slash on
   `palette.surface_alt` background) + 10 fixed accent colours; each swatch
   28 × 28 px circle with 2 px transparent border, selected = 2 px
   `palette.accent` border.
4. **Grid columns slider** — Slider 2 → 6 step 1, current value rendered at
   12 pt next to label.
5. **Capsule shape** — 4 toggle buttons (pill / rounded / circle / minimal),
   each 64 px wide × 48 px tall, 8 px corner radius, SVG preview on top +
   label below in 11 pt `palette.text_muted`.
6. **Capsule size** — 3-option segmented toggle (small / medium / large),
   total 240 px wide × 32 px tall, 6 px corner radius, selected segment uses
   `palette.accent` background + white text.

## Save / Cancel rules

- Save button is disabled when `name.trim().is_empty()` OR no field is dirty.
- Cancel discards local edits, closes modal — no Command emitted.
- Save emits `ZoneEditorAction::Save { zone_id, update }` with a `ZoneUpdate`
  containing only the fields the user touched (1.x parity — partial update
  semantics).

## Keyboard contract

- Escape → cancel (matches 1.x `keydown` handler).
- Click on the 50% black scrim → cancel.
- Enter inside the name input commits Save (only when Save is enabled).

## Hibernation

§11 R5 eligible — when the modal closes the host HWND fires
`WM_SHOWWINDOW(false)` which triggers `release_swap_chain`.
