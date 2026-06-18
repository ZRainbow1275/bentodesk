# ItemIcon — visual snap

Per-card icon. Resolves to a backend-supplied PNG (via the protocol
`bentodesk://icon/{hash}`) for the standard case, and falls back to an
extension-keyed selected-stack line-art glyph when extraction fails.

| token             | value                                |
|-------------------|--------------------------------------|
| container width   | 36 px (Standard) / 28 px (Wide)      |
| container height  | 36 px (Standard) / 28 px (Wide)      |
| render size       | 24 px (Standard) / 20 px (Wide)      |
| flex-shrink       | 0 (locked)                           |
| placeholder       | pulse animation (2 phase, 800 ms)    |
| error fallback    | glyph at `render_size − 4` px        |
| preload margin    | 200 px above/below viewport (lazy)   |
| protocol          | `bentodesk://icon/{icon_hash}`       |
| draggable         | false (host owns DnD)                |

Reference 1.x source: `bentodesk/src/components/ItemCard/ItemIcon.tsx` (174 LOC).

Locked behaviour:
- `IconRenderState` tracks the four lifecycle states
  (`Idle` → `Loading` → `Ready` | `Error`) so the renderer can pick the
  right brush without a string check.
- `fallback_icon_kind_for(extension)` maps the old 1.x extension categories
  onto selected-stack `IconKind` glyphs. Unknown extensions return
  `IconKind::Folder`. Lookup is ASCII-case-insensitive.
- `fallback_emoji_for(extension)` is retained only as a legacy compatibility
  helper for the old 1.x table; runtime item cards must not paint it.
- `IconSize` exposes Standard / Wide variants; wire-format-locked via
  serde unit tests.
