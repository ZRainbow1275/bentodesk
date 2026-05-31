# ZenCapsule — visual snap

Collapsed-state pill rendered when a BentoZone is not focused. Hosts the zone
icon, title and (optional) item-count badge inside a frosted-glass capsule.

| token            | value                                  |
|------------------|----------------------------------------|
| shape            | `CapsuleShape::Pill` (default)         |
| size             | `CapsuleSize::Medium` (default)        |
| height           | 36 px (Small) / 48 px (Medium) / 56 px (Large) |
| circle diameter  | 42 px (Small) / 52 px (Medium) / 64 px (Large) (1:1 disc) |
| corner-radius    | pill 24 px / rounded 12 px / circle height÷2 / minimal 8 px / square 4 px (legacy) |
| padding          | horiz 12 px, vert (icon-centred)       |
| background       | `palette.surface_glass` (frosted)      |
| border           | 1 px `palette.outline_subtle`          |
| layout           | row / center / center (icon · title · badge); circle = icon-only |
| icon size        | 14 px (Small) / 18 px (Medium) / 22 px (Large) |
| title font       | `typography.md` (single-line, no-wrap) |
| title gap        | 6 px after icon                        |
| badge            | rounded-rect 10 px radius, count caption |
| badge gap        | 6 px after title                       |
| hover            | scale ~1.02 (V-8 animator)             |
| drag-handle area | full capsule (passes to caller)        |

M2② (2026-05-29) — sizes/shapes re-centred on **Tauri v1.3.0** pixel
ground-truth (Q2 strict 1:1, ruling A):
- Heights from `bentodesk/src/services/hitTest.ts:94-99` `getCapsuleBoxPx`
  (small 36 / medium 48 / large 56); circle diameters from `:89-92`
  (42 / 52 / 64). Medium grew 44→48, Large 52→56 to match Tauri 1:1
  (the live pill grows ~33% small→medium).
- Per-shape radii from `bentodesk/src/components/BentoZone/BentoZone.css:80-99`
  (pill 24 / rounded 12 / circle 50% / minimal 8). Icon font-sizes from
  `ZenCapsule.css` (small 14 / medium 18 / large 22).

Reference 1.x source: `bentodesk/src/components/BentoZone/ZenCapsule.tsx`
+ `ZenCapsule.css` / `BentoZone.css` (shape variants) + `hitTest.ts`
(`getCapsuleBoxPx` — authoritative capsule box dims).

Locked behaviour:
- Three sizes (`Small` / `Medium` / `Large`) and — post-M2② — **four**
  Tauri-parity shapes (`Pill` / `Rounded` / `Circle` / `Minimal`) plus the
  retained legacy `Square` back-compat variant. The two Tauri shapes were
  *appended* to `CapsuleShape` (never reordered/renamed), and the `"square"`
  wire tag still deserializes, so saved `zones.bin` / 1.x layout JSON keeps
  loading. Enums round-trip through serde unit tests.
- The live collapsed pill (`zone_pill_geometry::pill_layout_for_zone`) reads
  the per-zone `capsule_size` / `capsule_shape` to drive height + icon +
  corner-radius; the shell hit-rect derives from the same call, so paint–hit
  parity (V-13) holds automatically.
- Title text is **abbreviation-aware**: when the rendered width exceeds the
  capsule, the caller (panel header) supplies the abbreviated text — the
  capsule itself never measures DWrite glyphs.
