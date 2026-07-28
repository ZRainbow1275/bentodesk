# CapsulePicker — Visual Fidelity Snapshot

Per Ruling R3. 1.x source: `bentodesk/src/components/CapsulePicker/CapsulePicker.tsx`
+ `CapsuleCard.tsx` + `CapsulePicker.css`.

## Geometry

- Panel size: **480 × 600** px at 96 DPI baseline (matches
  `WindowKind::CapsulePicker` `default_size`).
- Border radius: **12 px**.
- Inset padding: **20 px** uniform.

## Layout

Column, top → bottom:

1. **Header row** — title (icon `briefcase` + text "Context Capsules") on
   the left, close button (×) on the right.
2. **Capture row** — single-line input (`palette.surface_alt` background,
   placeholder "Name (e.g. Coding Mode)") + primary button "Capture
   current". Disabled while `busy`.
3. **Error banner** (conditional) — `palette.danger` background pill with
   the latest error message.
4. **Result banner** (conditional, after a restore) — counts of restored /
   pending / errors per the 1.x `RestoreResult` shape.
5. **List** — vertical scroll of `CapsuleCard` rows; one per saved capsule.
   Each card shows icon + name + captured-at + Restore + Delete buttons.
6. **Empty state** (conditional) — centred grey text "No capsules yet.
   Capture your current windows above."

## Colours

- Panel background: `palette.surface`.
- Title text: `palette.text`.
- Input background: `palette.surface_alt`.
- Primary button (Capture / Restore): `palette.accent` background.
- Secondary button (Delete): `palette.surface_alt` background, hover →
  `palette.danger` for the destructive intent.
- Error banner: `palette.danger` background.
- Empty-state text: `palette.text_muted`.

## Window class

- `WindowKind::CapsulePicker` →
  `(WS_EX_NOREDIRECTIONBITMAP, WS_POPUP | WS_VISIBLE)`.
- Accepts focus (the user types into the capture input).

## Hibernation

§11 R5 eligible. Closing the picker (close button, scrim, Escape) hides
the HWND → `release_swap_chain` fires after the 500 ms gate. Re-opening
calls `ensure_swap_chain` from the wndproc paint guard.

## State surface

- `entries: SmallVec<[CapsuleEntry; 8]>` — typical user has 1-8 saved
  capsules; inline buffer keeps the steady-state alloc-free.
- `new_name: SmolStr` — current capture-name input.
- `busy: bool` — capture / restore round trip in flight.
- `last_error: Option<SmolStr>` — most recent backend error, surfaced in
  the banner.
- `take_action()` — drained per frame for the latest user click
  (`Capture / Restore / Delete / Close`).

## Smoke verification

`mod tests` proves:

- Default chrome reads `palette.surface` / `palette.text`.
- 480 × 600 default size matches `WindowKind::CapsulePicker`.
- `click_capture` uses the trimmed input or the fallback name when blank.
- `click_restore / click_delete / click_close` record the matching actions.
- `take_action` is one-shot.
- `set_busy` / `set_error` surface correctly.
