# Tooltip — Visual Fidelity Snapshot

Per Ruling R3. 1.x source: `bentodesk/src/components/shared/Tooltip.tsx` +
`Tooltip.css`.

## Geometry

- Border radius: **4 px**.
- Inset padding: **4 px top/bottom, 8 px left/right**.
- Width / height: `Length::Auto` — sized to the text run, clamped by the
  hosting `WindowKind::Tooltip` HWND's 200 px default width.

## Layout

Single-row container. Text run only — no icons, no decorations.

## Colours

- Background: `palette.surface_alt` (DARK = 0x22222 8FF — `.95` opacity is
  baked into the alpha channel of the dark token; LIGHT = 0xF0F0F2FF).
- Text: `palette.text` (DARK = 0xE0E0E6FF).

## Typography

- Font size: 12 px (1.x `font-size: 12px`).
- Line height: 16 px.
- Font: Microsoft YaHei UI (zh-CN) / Segoe UI Variable (en-US).

## Show / hide timing

- Show delay: **400 ms** of continuous hover (1.x `DEFAULT_DELAY = 400`).
- Configurable per anchor via `TooltipController::with_delay_ms`.
- Hide: immediate on mouseleave (no fade-out delay, just CSS opacity zero
  in 1.x; in 2.0 the HWND `WM_SHOWWINDOW(SW_HIDE)` triggers T-099 hibernation).

## Animation

- 1.x: 150 ms ease-out fade-in + 4 px translate-from-direction (above /
  below) per `@keyframes tooltip-fade`.
- 2.0 mapping: animate the visual's α channel via the
  `bento-nano-animation` `AnimatedValue<f32>` once T-042 (easing) lands in
  Wave 3. Phase 1 ships the descriptor + timing controller; the fade-in
  hooks up alongside the other Phase-3 animation work without re-touching
  this file.

## Window class

- `WindowKind::Tooltip` →
  `(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_TOPMOST,
    WS_POPUP | WS_VISIBLE)`.
- ToolWindow keeps it out of alt-tab + taskbar; Transparent makes the
  whole tip click-through.
- Per §4.1: NoRedirectionBitmap is **omitted** here — Transparent + ToolWindow
  expects the GDI/redirection-bitmap path; combining with NoRedirectionBitmap
  silently breaks click-through.

## Hibernation

- All Tooltip HWNDs are §11 R5 eligible (every non-Main window is). The
  WM_SHOWWINDOW(false) / WM_SHOWWINDOW(true) round trip drives the same
  `release_swap_chain` / `ensure_swap_chain` path the MiniBar exercises;
  there's no Tooltip-specific gate because the controller doesn't own a
  Renderer reference (the dispatcher's `Command::ShowTooltip` /
  `Command::HideTooltip` handlers do).

## Smoke verification

`mod tests` in `mod.rs` proves:

- 400 ms default delay → Show emitted exactly once after cumulative 400 ms
  hover.
- Custom `delay_ms = 50` → Show emitted at 60 ms tick.
- `on_anchor_leave` while visible → Hide; while pending → Idle.
- Re-entering mid-session resets the accumulator (matches 1.x `setTimeout`
  re-arm behaviour).
