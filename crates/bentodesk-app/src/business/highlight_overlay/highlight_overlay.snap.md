# HighlightOverlay — Visual Fidelity Snapshot

Per Ruling R3. 1.x source: `bentodesk/src/components/SmartGroup/HighlightOverlay.tsx`
+ `src/styles/highlight.css`. Paired with `smart_group_suggestor.snap.md`.

> 1.x rendered pulsing **circles** at desktop-icon physical coordinates
> via Tauri event payloads. 2.0 keeps the in-zone translucent **fill rect**
> for imported Bento items and now adds off-grid desktop-icon **pulse circles**
> resolved from the real Windows icon-position snapshot / live COM layout.

## Geometry

The overlay sits **inside** the BentoPanel content layer (above the
item grid, below the toolbar). It is not a separate HWND.

- Each highlight rect inherits the matched item's `Rect { x, y, w, h }`
  in the BentoPanel's local coordinate space.
- Each off-grid desktop pulse target carries the real desktop icon center
  coordinate (`x`, `y`) and display name; Search/Suggestor resolve source
  file paths against `icon_layout_backup.json`, then fall back to live
  `icon_positions::save_layout()` when no backup exists.
- Outline width: **2 px** (snap.md token — see Colours).
- Outline corner radius: matches the item card's (`8 px`, mirroring
  `item_card.snap.md`).
- Inner padding (rect inset from cell): **4 px** so the highlight does
  not occlude the item's own border.
- Pulse core radius: **8 px**. Pulse halo radius expands from **14 px** to
  **28 px** over a **1.6 s** deterministic loop.

## Colours

- Fill: `palette.accent` with alpha **0x33** (≈ 20 %). Derived from the
  current theme so dark / light parity stays on rails.
- Outline: `palette.accent` with alpha **0xCC** (≈ 80 %). Optional —
  enabled when `HighlightOverlayState.show_outline` is true (default).
- Pulse core: `palette.accent` with alpha **0.70**.
- Pulse halo: `palette.accent` with alpha **0.34 → 0.0** across the
  current pulse phase.

The overlay never reads from raw `Color` literals; it composes against
`bentodesk_theme::current().palette` at `build()` time so theme switch
re-paints automatically (Wave-E theme-observer pattern).

## Trigger model

The `SearchBar` and `SmartGroupSuggestor` are producers:

```text
Search/Suggestor selected paths
    → zone item match → HighlightOverlayState::set_targets_and_pulses(rects, [])
    → off-grid desktop path → icon_positions backup/live layout
        → HighlightOverlayState::set_targets_and_pulses([], pulses)
Close / no selection
    → HighlightOverlayState::clear()
```

Both calls are **synchronous local-state updates** — no dispatcher
round-trip per team-lead R-2026-05-03 ruling. The overlay is pure
preview chrome.

## Animation

- 1.x ran a 1.6 s `pulse-fade` keyframe on each circle. 2.0 now keeps an
  internal `pulse_elapsed_ms` phase and repaints pulse circles from the shell
  frame loop while any off-grid pulse exists.
- Auto-clear timeout: when the suggestor sets a `duration_ms > 0`, the
  state's `tick()` decrements an internal countdown and clears the
  targets when it reaches zero. Mirrors the 1.x `HIGHLIGHT_DURATION_MS
  = 3_000` default but defers the actual countdown to the shell's
  frame loop (no `setTimeout`, no async).

## State machine

Four fields:

- `targets: SmallVec<[HighlightRect; 8]>` — inline up to 8 highlights
  (typical AI suggestion clusters cap at `MAX_CLUSTER_SIZE = 15` per
  `ai_recommender`, but the most common case is ≤ 8). Beyond the
  inline cap the SmallVec heap-allocates — acceptable, hover is not
  on the §10 hot path.
- `pulses: SmallVec<[HighlightPulse; 8]>` — off-grid desktop-icon pulse
  targets resolved from real desktop icon positions.
- `auto_clear_ms: Option<u32>` — `Some(remaining)` while a countdown
  is in flight, `None` when targets are cleared or the user explicitly
  set them with no duration.
- `pulse_elapsed_ms: u32` — deterministic pulse loop phase for renderers.

## Smoke verification

`mod tests` proves:

- `HighlightOverlayState::set_targets` overwrites the previous list.
- `HighlightOverlayState::clear` empties the list and cancels the
  auto-clear countdown.
- `tick(dt_ms)` decrements `auto_clear_ms`, clears at 0, returns
  `false` when nothing to do.
- `set_targets_for(rects, duration_ms)` records the countdown.
- `set_targets_and_pulses` / `set_targets_and_pulses_for` can paint in-zone
  rects and off-grid desktop pulses in the same frame.
- `tick(dt_ms)` advances pulse phase and requests continuous frames while
  pulses are visible.
- `build()` returns an `AbsoluteFill` Container with no padding so it
  draws edge-to-edge of its parent (the BentoPanel content layer).
- Colours (fill / outline / pulse core / pulse halo) are derived from
  `palette.accent`.
