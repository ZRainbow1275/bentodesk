# Dialog — Visual Fidelity Snapshot

Per Ruling R3. 1.x source: `bentodesk/src/components/shared/PromptModal.tsx`
+ `PromptModal.css`.

## Geometry

- Panel size: **400 px wide**, height auto (typically ~180 px for the
  title + input + actions stack).
- Border radius: **12 px**.
- Inset padding: **20 px** uniform.

## Layout

Column, top → bottom:

1. Title row (`palette.text` colour, 16 pt heading).
2. Input row (single-line text field; placeholder reads `palette.text_muted`).
3. Action row — Cancel (secondary, left) + OK (primary, right) with
   8 px gap.

## Colours

- Panel background: `palette.surface`.
- Title: `palette.text`.
- Primary button: `palette.accent` background, `#FFFFFF` text.
- Secondary button: `palette.surface_alt` background, `palette.text` text.
- Scrim (full-viewport overlay behind the panel): `palette.scrim` (DARK =
  0x00000080, LIGHT = 0x00000040).

## Keyboard contract

- Enter → confirm (`DialogState::confirm`).
- Escape → cancel (`DialogState::cancel`).
- Click on the scrim outside the panel → cancel (matches 1.x backdrop click).

## Animation

1.x uses `scale-in` keyframes on the panel when the modal opens (200 ms
ease-out from 0.96 → 1.0 scale). 2.0 wires the same via the
`bento-nano-animation` `AnimatedValue<f32>` once T-042 (easing) lands;
descriptor surface is unchanged.

## Window class (when dialog hosts itself in its own HWND)

- `WindowKind::Settings` →
  `(WS_EX_NOREDIRECTIONBITMAP, WS_POPUPWINDOW | WS_CAPTION | WS_VISIBLE)`.
- For inline-overlay use (rendered into the Main HWND), see the existing
  `app::settings_panel` for the scrim + centred panel pattern.

## Hibernation

If hosted in a dedicated HWND, §11 R5 eligible — the WM_SHOWWINDOW(false)
on dismiss triggers `release_swap_chain`. Inline overlays don't add a
new HWND, so hibernation is a no-op.

## Smoke verification

`mod tests` proves:

- Default chrome reads `palette.surface` / `palette.text` (theme switch
  follows automatically).
- `with_ok_label / with_cancel_label / with_placeholder` overrides apply.
- `DialogState::confirm` records `Submit(value.clone())`.
- `DialogState::cancel` records `Cancel`.
- `take_action` is one-shot (returns `None` on second call without further
  user interaction).
