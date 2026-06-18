# Icons — visual snap (T-079: ZoneIcon + LucideDynamic family)

Three-namespace icon dispatcher used by ZenCapsule, BentoPanel, and any
zone-chrome surface that needs a glyph.

| namespace | source                                                        |
|-----------|---------------------------------------------------------------|
| builtin   | `IconKind::*` — 30 hand-rolled 24×24 line-art SVGs            |
| `lucide:` | dynamic load from on-disk lucide-static cache (TBD backend)   |
| `custom:` | user-uploaded PNG via `bentodesk://custom-icon/{uuid}`        |
| (text)    | legacy payload parsed for compatibility, rendered as neutral builtin glyph |

| token         | value                                       |
|---------------|---------------------------------------------|
| viewBox       | `0 0 24 24` (every builtin)                 |
| stroke        | `currentColor`                              |
| stroke-width  | 1.5 px                                      |
| line-cap/join | round                                       |
| fill          | `none`                                      |
| default size  | 20 px (`ICON_DEFAULT_SIZE_PX`)              |

Reference 1.x sources:
- `bentodesk/src/components/Icons/ZoneIcons.tsx` (285 LOC, 30 components)
- `bentodesk/src/components/Icons/ZoneIcon.tsx` (82 LOC, dispatcher)
- `bentodesk/src/components/Icons/LucideDynamic.tsx` (90 LOC, lazy loader)

Locked behaviour:
- `IconRef::parse(s)` decodes any of the four forms above with a single
  pass over `s`. Unknown bare names fall through to `IconRef::Text(s)`,
  matching 1.x wire semantics, but selected-stack runtime renderers must
  draw a neutral built-in glyph for that branch instead of painting the
  payload as visible text / emoji.
- The 30 built-in `IconKind` variants are the selected-stack wire-format
  contract. Existing snake_case values such as `"folder_open"` keep loading,
  and the source Tauri hyphenated aliases (`"folder-open"`,
  `"external-link"`, `"arrow-right"`) also parse successfully.
- The built-in documents are embedded from
  `bentodesk/src/components/Icons/ZoneIcons.tsx` and render through the
  selected-stack D2D SVG cache in IconPicker slots. The lucide loader cache
  remains a separate namespace owned by `bento-nano-platform::svg` once the
  on-disk lucide source lands.
