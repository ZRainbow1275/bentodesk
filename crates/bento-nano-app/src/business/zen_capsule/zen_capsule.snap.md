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
| padding (left)   | 12 px (Small) / 20 px (Medium) / 28 px (Large) — Tauri `--spacing-md/xl/2xl` |
| padding (right)  | 12 px (Small) / 16 px (Medium) / 20 px (Large) — Tauri `--spacing-md/lg/xl` |
| padding (vert)   | 0 (icon-centred via `align-items:center`) |
| inner gap        | 8 px (Small) / 12 px (Medium) / 16 px (Large) — single Tauri `gap`, both icon→title and title→badge |
| background       | non-minimal: acrylic fallback + `palette.surface_zen`; **minimal: transparent (no fill, no blur)** |
| border           | non-minimal: 1 px solid `palette.border_zen` rgba(255,255,255,0.1); **minimal: 1 px DASHED rgba(255,255,255,0.2)** |
| shadow / glow    | none on the collapsed pill (V-9); minimal additionally forces no shadow/no blur |
| layout           | row / center (icon · title flex:1 · badge); circle = icon-only |
| icon size        | non-circle: 14 px (S) / 18 px (M) / 22 px (L); **circle: 22 px (S+M) / 28 px (L)** |
| title font       | per-tier: 11 px (S) / 14 px (M) / 16 px (L), weight 500, single-line nowrap |
| title letter-sp. | 0.3 px (DWrite `SetCharacterSpacing` trailing advance) |
| title overflow   | **font-SHRINK to fit (useTextAbbr), no ellipsis until the 8 px floor**; ellipsis only at the floor |
| badge            | rounded-rect 10 px radius; bg `var(--zone-accent, --badge-bg)`; text `--text-primary` |
| badge font       | per-tier: 10 px (S) / 11 px (M) / 13 px (L), weight 600 (semibold), line-height 1.4 |
| badge padding    | (h,v): (6,1) (S) / (9,2) (M) / (12,3) (L) px |
| badge height     | 14 px (S) / 16 px (M) / 20 px (L)      |
| hover            | background-only transition (Tauri css:10); **pill scale DISABLED (V-12, `pill_scale_for`≡1.0)** — no scale on hover |
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

G5 (2026-06-01) — collapsed-capsule 1:1 finishing pass against **Tauri v1.3.0**
(`ZenCapsule.tsx` / `ZenCapsule.css` / `BentoZone.css` / `variables.css`):
- **Per-tier asymmetric padding** (`ZenCapsule.css:8/:81/:101`): left
  12/20/28, right 12/16/20. Icon left-anchored at the tier LEFT padding; badge
  right-anchored at the tier RIGHT padding. Was a flat symmetric 12/12.
- **Per-tier inner gap** (`:9/:82/:102`): 8/12/16. Was a flat 6. The single
  Tauri `gap` token drives both icon→title and title→badge.
- **Per-tier title font** (`:25/:90/:110`): 11/14/16 px, weight 500. Was the
  global default 16 px on every tier.
- **Title font-SHRINK** (useTextAbbr, `ZenCapsule.tsx:26,37` + `css:19-32`):
  the renderer measures the label via `IDWriteTextLayout::GetMetrics` and steps
  the font-size down (1 px steps) to an 8 px floor to fit the capsule with NO
  ellipsis; the `…` trim is applied ONLY at the floor (matching Tauri's
  MIN_FONT_SIZE_PX). The resolved size is memoised (§10: measure runs only on a
  label/width/tier change, never per-frame in the fits-case).
- **Letter-spacing 0.3 px** (`css:27`): APPLIED via DWrite
  `IDWriteTextLayout1::SetCharacterSpacing` (trailing advance over the run) —
  the same seam `draw_text_tracked` uses. No deviation here.
- **Per-tier badge** (`:34-43/:93-96/:113-116`): font 10/11/13 px, weight 600
  (semibold), padding (h,v) (6,1)/(9,2)/(12,3), box height 14/16/20. Was default
  16 px / medium weight / flat 20 px height.
- **Minimal shape** (`BentoZone.css:92-99 .bento-zone--shape-minimal`):
  TRANSPARENT background (no acrylic + no surface fill), NO backdrop blur, NO
  shadow/neon glow, and a 1 px DASHED border at rgba(255,255,255,0.2) (radius
  8 px). Other shapes keep the glass + solid `border-zen` hairline.
- **Circle icon override** (`:68-70/:127-129`): 22 px (small + medium) / 28 px
  (large) — distinct from the non-circle base icon size (14/18/22).
- **Hover green dot REMOVED** (fix 7): the V-14 hover-gated `accent_green` dot
  over the badge had no Tauri analogue (`css:10` only transitions background);
  the count badge now stays visible on hover. No separate always-on status dot
  is painted by the collapsed pill.
- **Hover scale** (fix 8, VERIFIED): `animator::pill_scale_for` returns EXACTLY
  1.0 — `HOVER_SCALE_DELTA`/`PRESS_SCALE_DELTA` are 0.0 (V-12 disabled pill
  scale). Tauri's ZenCapsule has no scale transform, so this already matches;
  left in place (no re-enable). NOT a nano deviation.

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
- Title text is **font-shrink-aware** (G5): when the rendered width exceeds the
  capsule, the renderer (`draw_pill_title_shrink_to_fit`) measures the label via
  DWrite `GetMetrics` and shrinks the font-size to fit (no ellipsis) down to an
  8 px floor, mirroring Tauri's `useTextAbbr`. The measure runs only on a
  label/width/tier change (memoised), so the fits-case is allocation-free per
  frame (§10).
