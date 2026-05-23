# AutoLayoutMenu — Visual Fidelity Snapshot

Per Ruling R3. 1.x source: `bentodesk/src/components/BulkManager/AutoLayoutMenu.tsx`.
Popover triggered from `BulkManagerPanel`'s "Auto Layout" button; lets the
user pick one of five Tauri-compatible layout strategies that the shell then
applies via `Command::BulkApplyLayout`.

## Geometry

- Popover width: **280 px** (`min(280px, 92vw)` for narrow displays).
- Popover height: **Auto** — sized to the row count (5 rows fixed today).
- Border radius: **12 px**.
- Outer padding: **8 px** uniform.
- Background: `palette.surface_alt` (popover floats above the
  `BulkManagerPanel`'s `palette.surface` body — slight tint differentiates).
- Border: 1 px `palette.border`.
- Drop shadow: `palette.scrim` colour, 8 px blur, 4 px Y offset.

## Header row

- Height: **32 px**.
- Title: `palette.text`, 14 pt Semibold, "Auto Layout".
- Close `IconButton`: 24 × 24 px, right-aligned, `palette.text_muted` tint.
- 8 px bottom margin between header and option list.

## Option row geometry

Row direction Row, 12 px inner gap. Per-row:

| Slot | Width | Notes |
|------|-------|-------|
| Icon | 32 px square | Strategy glyph (`SvgIcon` once T-079 lands; today the row stores the slug). |
| Info column | flex 1 | Stack: name (`palette.text`, 13 pt Semibold) + 1-line description (`palette.text_muted`, 11 pt). |

Row outer padding: 10 px vertical / 12 px horizontal. Row corner radius:
8 px. Row background: `palette.surface_alt` idle, `palette.hover_overlay`
on hover (renderer applies the overlay; no state stored here).

## Strategies

| Slug | Label | Description | Icon slug |
|------|-------|-------------|-----------|
| `grid` | "Grid" | "Snap selected zones to a uniform grid." | `grid-3x3` |
| `row` | "Row" | "Arrange selected zones in a horizontal row." | `rows-3` |
| `column` | "Column" | "Arrange selected zones in a vertical column." | `columns-3` |
| `spiral` | "Spiral" | "Place selected zones along a deterministic spiral." | `rotate-ccw` |
| `organic` | "Organic" | "Pack selected zones with organic repulsion." | `sparkles` |

Iteration order matches the table above — the user reads top-down.

## Selection contract

- The popover is **single-shot** — the user picks one strategy and the
  popover dismisses. The shell tears the host HWND down on `Pick` /
  `Close` action drain.
- No multi-select, no preview state.

## Action surface

| User intent | Action recorded | Drained `Command` |
|-------------|-----------------|--------------------|
| Click strategy row | `AutoLayoutAction::Pick { strategy }` | shell resolves selected/listed zone ids and forwards `Command::BulkApplyLayout { ids, algorithm }`. |
| Click close `X` / Escape | `AutoLayoutAction::Close` | shell hides host HWND — no Command. |

`take_action()` is one-shot; subsequent calls without further
interaction return `None`.

## Window class

Hosted in its own popover HWND (`WindowKind::Popup` shape — layered, no
focus). Anchored to the `BulkManagerPanel`'s "Auto Layout" button via
the `Popup` widget primitive's anchor + placement contract.

## Hibernation

§11 R5 eligible — pick or close triggers `WM_SHOWWINDOW(false)` and
the per-window swap chain release. Popover state resets across show /
hide cycles (no persistent selection to preserve).

## Smoke verification

`mod tests` proves:

- `LayoutStrategy::ALL` lists the five strategies in the spec table order.
- `LayoutStrategy::wire` round-trips through `parse`; unknown wire token
  falls back to the default (`Grid`).
- `LayoutStrategy::label` / `description` / `icon_slug` return non-empty
  static strings for every strategy.
- `AutoLayoutMenuState::pick` records `Pick { strategy }`; `close`
  records `Close`.
- `take_action` is one-shot.
- `into_command` returns `None` for both variants because the popover itself
  does not own selected row ids; the shell emits `BulkApplyLayout`.
- `build()` returns a Column container at `POPOVER_WIDTH_PX` width.
- Strategy serde round-trip survives JSON encode/decode (ΔB lock).
