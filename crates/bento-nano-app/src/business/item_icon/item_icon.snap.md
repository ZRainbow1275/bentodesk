# ItemIcon — visual snap

Per-card icon. Resolves to a backend-supplied PNG (via the protocol
`bentodesk://icon/{hash}`) for the standard case, and falls back to an
emoji glyph keyed off the file extension when extraction fails.

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
- `fallback_emoji_for(extension)` lifts the 1.x extension→emoji table into
  Rust. Returns `📁` (folder) for unknown extensions. Lookup is
  ASCII-case-insensitive.
- `IconSize` exposes Standard / Wide variants; wire-format-locked via
  serde unit tests.
