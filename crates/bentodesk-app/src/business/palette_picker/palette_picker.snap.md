# PalettePicker — Visual Fidelity Snapshot

Per Ruling R3. 1.x source: `bentodesk/src/components/BulkManager/PalettePicker.tsx`.
Inline popover triggered from `BulkManagerPanel`'s palette button; lets the
user pick one accent colour from a curated swatch set, applied to every
selected zone in a single bulk update.

## Geometry

- Popover width: **240 px** (`min(240px, 92vw)`).
- Popover height: **Auto** — sized to the swatch grid + header.
- Border radius: **12 px**.
- Outer padding: **12 px** uniform.
- Background: `palette.surface_alt`.
- Border: 1 px `palette.border`.
- Drop shadow: `palette.scrim` colour, 8 px blur, 4 px Y offset.

## Header row

- Height: **28 px**.
- Title: `palette.text`, 13 pt Semibold, "Pick Color".
- Close `IconButton`: 20 × 20 px, right-aligned, `palette.text_muted` tint.
- 8 px bottom margin between header and swatch grid.

## Swatch grid

- Layout: **4 columns** wide, gap **8 px** both axes.
- Per-cell: **40 × 40 px** square, 8 px corner radius (inner swatch is a
  32 × 32 px circle centred in the cell so hover scale doesn't clip the
  outline).
- Hover: cell scales to **1.05x** (200 ms ease-out); state lives in the
  renderer (overlay), not in the picker — no per-cell hover state stored.
- Selected: cell renders a 2 px `palette.accent` ring, 2 px outset from
  the swatch circle.
- Idle background: cell is transparent — only the swatch circle paints.

## Swatch palette (12 presets)

Curated 1.x `ACCENT_PRESETS` set, flattened into a single 12-entry list.
Names ship as `SmolStr` (i18n keys live in the renderer; the picker
exposes the raw slug + hex pair).

| Slot | Slug | Hex |
|------|------|-----|
| 0 | `slate` | `#64748b` |
| 1 | `blue` | `#3b82f6` |
| 2 | `indigo` | `#6366f1` |
| 3 | `violet` | `#8b5cf6` |
| 4 | `pink` | `#ec4899` |
| 5 | `red` | `#ef4444` |
| 6 | `orange` | `#f97316` |
| 7 | `amber` | `#f59e0b` |
| 8 | `yellow` | `#eab308` |
| 9 | `green` | `#22c55e` |
| 10 | `teal` | `#14b8a6` |
| 11 | `cyan` | `#06b6d4` |

Iteration order matches the table — top-left → bottom-right reading
order across the 4-wide grid (3 rows total).

## Selection contract

- Single-shot — the user clicks one swatch, the popover dismisses, the
  shell forwards the chosen colour to `bulk_update_zones`.
- The picker remembers the previously-selected swatch (when seeded via
  [`set_selected`]) so the highlight ring renders on re-open.

## Action surface

| User intent | Action recorded | Drained `Command` |
|-------------|-----------------|--------------------|
| Click swatch | `PalettePickerAction::Pick { swatch }` | shell forwards a `bulk_update_zones` request (sequenced as `Command::SetSetting`-style calls per zone in Phase 1). |
| Click close `X` / Escape | `PalettePickerAction::Close` | shell hides host HWND — no Command. |

`take_action()` is one-shot.

## Window class

Hosted in its own popover HWND (`WindowKind::Popup` shape — layered, no
focus). Anchored to the `BulkManagerPanel`'s palette button via the
`Popup` widget primitive's anchor + placement contract.

## Hibernation

§11 R5 eligible — pick or close triggers `WM_SHOWWINDOW(false)` and
the per-window swap chain release.

## Smoke verification

`mod tests` proves:

- `swatch_table()` ships exactly 12 entries in the snap-table order.
- `Swatch::find_by_slug` resolves each slug to the matching hex; unknown
  slug returns `None`.
- `PalettePickerState::set_selected(slug)` updates the highlight; passing
  an unknown slug clears the selection without panicking.
- `pick(slug)` records `Pick { swatch }` only when the slug matches a
  known swatch; unknown slug records nothing (returns `false`).
- `close` records `Close`.
- `take_action` is one-shot.
- `build()` returns the chrome Container at `POPOVER_WIDTH_PX` width.
- `Swatch` serde round-trip preserves slug + hex (ΔB lock).
