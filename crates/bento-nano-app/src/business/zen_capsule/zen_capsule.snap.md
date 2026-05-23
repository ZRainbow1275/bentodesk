# ZenCapsule — visual snap

Collapsed-state pill rendered when a BentoZone is not focused. Hosts the zone
icon, title and (optional) item-count badge inside a frosted-glass capsule.

| token            | value                                  |
|------------------|----------------------------------------|
| shape            | `CapsuleShape::Pill` (default)         |
| size             | `CapsuleSize::Medium` (default)        |
| height           | 36 px (Small) / 44 px (Medium) / 52 px (Large) |
| corner-radius    | half of height (full pill)             |
| padding          | horiz 14 px, vert 6 px                 |
| background       | `palette.surface_glass` (frosted)      |
| border           | 1 px `palette.outline_subtle`          |
| layout           | row / center / center (icon · title · badge) |
| icon size        | 20 px (Small) / 24 px (Medium) / 28 px (Large) |
| title font       | `typography.body_strong` 13 px         |
| title gap        | 8 px after icon                        |
| badge            | rounded-rect 6 px radius, 11 px caption|
| badge gap        | 6 px after title                       |
| hover            | scale 1.02 over 120 ms                 |
| drag-handle area | full capsule (passes to caller)        |

Reference 1.x source: `bentodesk/src/components/BentoZone/ZenCapsule.tsx`
(48 LOC) + sibling `PanelHeader.tsx` for icon-size taxonomy.

Locked behaviour:
- Three sizes (`Small` / `Medium` / `Large`) and three shapes
  (`Pill` / `Rounded` / `Square`); enums round-trip through serde
  unit tests so 1.x layout JSON keeps loading after the rewrite.
- Title text is **abbreviation-aware**: when the rendered width exceeds the
  capsule, the caller (panel header) supplies the abbreviated text — the
  capsule itself never measures DWrite glyphs.
