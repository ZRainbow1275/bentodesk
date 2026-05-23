# Popover — Visual Fidelity Snapshot

Per Ruling R3. 1.x parallels: `ContextMenu.css` + the inline portal chrome
used by `Dropdown` / `AutoLayoutMenu` / `PalettePicker`.

## Geometry

- Border radius: **8 px**.
- Border thickness: **1 px** (hairline).
- Inset padding: **4 px** uniform.
- Anchor margin (gap between popover and anchor): **8 px**.

## Colours

- Background: `palette.surface_alt`.
- Border: `palette.border` (DARK = 0x33333AFF, LIGHT = 0xD8D8DDFF).

## Placement contract

Default = `Above` the anchor, horizontally centred. Auto-flip rules:

1. If the requested placement would clip the viewport edge (< 4 px), flip
   to the opposite side.
2. Horizontal position clamped to `[4, viewport.width - content.width - 4]`.

## Window class

- `WindowKind::ContextMenu` →
  `(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
    WS_POPUP | WS_VISIBLE)`.
- NoActivate so opening a popover doesn't steal focus from the owner
  window.
- Topmost so the popover paints above the BentoCard.

## Hibernation

§11 R5 eligible. Closing a popover (click-outside dismiss, Escape) hides
the HWND → triggers the standard hibernation pipeline.

## Smoke verification

`mod tests` proves the placement algorithm:

- Anchor at mid-screen with default `Above` preference → above (y = anchor.y -
  content.h - 8).
- Anchor near top edge → flip to `Below` (y = anchor.y + anchor.h + 8).
- Anchor near right edge → horizontal clamp to `viewport.w - content.w - 4`.
- Anchor near left edge → horizontal clamp to `4`.
