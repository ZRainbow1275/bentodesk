# ZenCapsule — visual snap

Collapsed-state pill rendered when a BentoZone is not focused. Hosts the zone
icon, title and (optional) item-count badge inside a frosted-glass capsule.

| token            | value                                  |
|------------------|----------------------------------------|
| shape            | `CapsuleShape::Pill` (default)         |
| size             | `CapsuleSize::Medium` (default)        |
| outer width      | 120 px (Small) / 214 px (Medium video-observed) / 220 px (Large video-observed) |
| height           | 36 px (Small) / 48 px (Medium) / 50 px (Large, 2026-07-23 hand-test refinement) |
| circle diameter  | 42 px (Small) / 52 px (Medium) / 64 px (Large) (1:1 disc) |
| corner-radius    | pill 24 px / rounded 12 px / circle height÷2 / minimal 8 px / square 4 px (legacy) |
| padding (left)   | 12 px (Small) / 20 px (Medium) / 28 px (Large) — Tauri `--spacing-md/xl/2xl` |
| visible large icon left slot | 21 px (Large video-observed, visible glyph only); no-glyph large keeps 28 px + 6 px residual slot |
| padding (right)  | 12 px (Small) / 16 px (Medium) / 20 px (Large) — Tauri `--spacing-md/lg/xl` |
| large badge right inset | 20.5 px (visible glyph Large) / 21.5 px (no-glyph Large), video-observed; Small/Medium keep source right padding |
| padding (vert)   | 0 (icon-centred via `align-items:center`) |
| inner gap        | 8 px (Small) / 12 px (Medium) / 16 px (Large) — single Tauri `gap`, both icon→title and title→badge |
| visible large inner gap | 11 px (Large video-observed, visible glyph only); no-glyph large keeps 16 px |
| visible large content dy | 2 px down (Large video-observed, visible glyph only) |
| no-glyph large content dy | 2 px up (Large video-observed, explicit no-glyph path only) |
| background       | non-minimal: acrylic fallback + `palette.surface_zen`; **minimal: transparent (no fill, no blur)** |
| border           | non-minimal: 1 px solid `palette.border_zen` rgba(255,255,255,0.1); **minimal: 1 px DASHED rgba(255,255,255,0.2)** |
| shadow / glow    | non-minimal: active theme `shadow_zen` stack, matching Tauri `.bento-zone--zen { box-shadow: var(--shadow-zen) }`; **minimal: no shadow/no blur** |
| layout           | row / center (icon · title flex:1 · badge); circle = icon-only |
| icon size        | 18 px for visible glyphs in all sizes and shapes; explicit no-glyph icons use a 6 px video-observed residual slot and skip glyph paint |
| title font       | per-tier: 11 px (S) / 14 px (M) / 15 px (L no-glyph video-observed); visible-glyph Large uses 13.93 px video-observed cap; weight 500, single-line nowrap |
| title letter-sp. | 0.3 px default; visible-glyph Large uses 0.85 px and no-glyph Large uses 0.2 px video-observed DWrite trailing advance |
| title overflow   | **font-SHRINK to fit (useTextAbbr), no ellipsis until the 8 px floor**; ellipsis only at the floor |
| badge            | rounded-rect 10 px radius; bg `var(--zone-accent, --badge-bg)`; text `--text-primary` |
| badge font       | per-tier: 10 px (S) / 11 px (M) / 11 px (L video-observed), DWrite digit weight 800 (V21-C34 video-observed ink density), line-height 1.4 |
| badge padding    | (h,v): (6,1) (S) / (9,2) (M) / (9,2) (L video-observed) px |
| badge height     | 14 px (S) / 16 px (M) / 16 px (visible-glyph Large) / 17 px (no-glyph Large), video-observed |
| no-glyph large badge width | base width + 0.67 px, video-observed; other branches keep source width |
| hover            | no base-fill brighten and no ordinary-pill transform; Tauri only declares `transition: background` for `.zen-capsule` without a hover background rule. **pill scale DISABLED (V-12, `pill_scale_for`≡1.0)** — no scale on hover |
| drag-handle area | full capsule (passes to caller)        |

M2② (2026-05-29) — sizes/shapes were initially re-centred on **Tauri v1.3.0**
pixel ground-truth (Q2 strict 1:1, ruling A):
- Heights from `bentodesk/src/services/hitTest.ts:94-99` `getCapsuleBoxPx`
  (small 36 / medium 48 / large 56); circle diameters from `:89-92`
  (42 / 52 / 64). Medium grew 44→48, Large 52→56 to match Tauri 1:1
  (the live pill grows ~33% small→medium).
- **2026-07-23 hand-test refinement:** Large keeps its 200 px width and all
  content alignment profiles, but its live height is compacted from 56 to
  50 DIPs. This deliberately improves on the Tauri baseline after repeated
  desktop hand tests found the 56-DIP shell visually too coarse; Small/Medium
  remain 36/48 and circle keeps its independent 64-DIP diameter contract.
- Per-shape radii from `bentodesk/src/components/BentoZone/BentoZone.css:80-99`
  (pill 24 / rounded 12 / circle 50% / minimal 8). The actual icon box is the
  fixed `ZoneIcon size={18}` wrapper from `ZenCapsule.tsx`; small/large
  `.zen-capsule__icon` font-size rules do not resize the inner SVG/custom image
  because `ZoneIcon.css` sizes it with `--zone-icon-size`.

G5 (2026-06-01) — collapsed-capsule 1:1 finishing pass against **Tauri v1.3.0**
(`ZenCapsule.tsx` / `ZenCapsule.css` / `BentoZone.css` / `variables.css`):
- **Per-tier asymmetric padding** (`ZenCapsule.css:8/:81/:101`): left
  12/20/28, right 12/16/20. Icon left-anchored at the tier LEFT padding; badge
  right-anchored at the tier RIGHT padding. Was a flat symmetric 12/12.
- **Visible-glyph large left slot** (V21-C21): the source CSS padding table
  remains 12/20/28, but the 2026-06-02 Browser component crop shows the large
  visible icon/title run 7 DIPs left of the CSS 28 px slot. Native therefore uses
  a 21 px left slot only when the large capsule has a visible icon glyph; the
  no-glyph `Compiler` path keeps the C19 28 px + 6 px residual slot.
- **Per-tier inner gap** (`:9/:82/:102`): 8/12/16. Was a flat 6. The single
  Tauri `gap` token drives both icon→title and title→badge.
- **Visible-glyph large inner gap** (V21-C22): the source CSS gap table remains
  8/12/16, but the 2026-06-02 Browser component crop shows the visible icon to
  title start 7 px tighter at 1.5x proof scale after the C21 left-slot fix.
  Native therefore uses an 11 px runtime gap only for large capsules with a
  visible icon glyph; the no-glyph `Compiler` path keeps the C19/C20 16 px
  source gap.
- **Visible-glyph large content y-offset** (V21-C24): after C23, the Browser
  visible-glyph large title/badge band was still high against the 2026-06-02
  component crop. Native applies a 2 DIP downshift only to Large capsules with a
  visible icon glyph, moving the icon/title/badge content band together while
  leaving no-glyph right-rail `Compiler` to its separate video-observed branch.
- **No-glyph large content y-offset** (V21-C26): after C25, the right-rail
  no-glyph `Compiler` title and count badge remained about 3 px lower than the
  2026-06-02 component crop at 1.5x proof scale. Native applies a 2 DIP upshift
  only to Large capsules without a visible icon glyph, moving title and badge
  together while preserving the Browser visible-glyph branch proven by C24/C25.
- **Large badge right inset** (V21-C27): the source CSS right padding table
  remains 12/16/20, but the post-C26 component-local review still showed the
  Large count badges slightly right of the 2026-06-02 crop. Native therefore uses
  video-observed badge right insets only for Large count badges: 20.5 px when a
  visible icon glyph is present (`Browser`) and 21.5 px for the no-glyph
  `Compiler` path. Title bboxes and badge y spans remain on their C26 branches.
- **Visible-glyph large badge height** (V21-C28): after C27, the Browser
  visible-glyph count chip still measured two device pixels taller than the
  2026-06-02 component crop, while the no-glyph `Compiler` badge already
  matched the reference height. Native therefore uses the 16 px medium visual span
  only for Large visible-glyph badges and keeps no-glyph Large at the C23/C26
  17 px span.
- **Per-tier title font** (`:25/:90/:110`, C20 adjusted by 2026-06-02 reference
  frame): small/medium keep the source CSS metrics at 11/14 px, while large
  Browser/Compiler titles visually match a 15 px bbox in the authoritative
  recording. The local CSS still declares 16 px for
  `.zen-capsule--large .zen-capsule__title`; the video frame is the higher
  fidelity visual authority for this slice. Was the global default 16 px on
  every tier before G5.
- **Visible-glyph large title font** (V21-C25): after C24, the Browser
  visible-glyph large title bbox remained too wide against the 2026-06-02
  reference while the no-glyph `Compiler` title already matched the 15 px large
  tier. Native therefore resolves the Large visible-glyph title font separately
  while keeping no-glyph Large at 15 px and leaving badge, gap, y-offset, and
  capsule width unchanged.
- **Visible-glyph large title top/right/height** (V21-C29): after C28, the
  Browser title still measured `[42,60,150,88]` against the component reference
  `[42,62,149,88]`. The C28 14.5 px cap already shrank to a 14 px rendered run,
  so C29 uses a 13.93 px visible-glyph Large cap plus 0.85 px DWrite trailing
  advance only for that branch. This lands the Browser title at
  `[42,62,149,88]` while preserving the Browser badge and no-glyph `Compiler`
  title/badge bboxes.
- **No-glyph large title/badge residual x alignment** (V21-C30): after C29, the
  right-rail no-glyph `Compiler` title and green badge each remained one device
  pixel right of the component reference. Native therefore uses a 0.2 px DWrite
  trailing-advance branch for Large no-glyph titles and expands the Large
  no-glyph badge width by 0.67 DIP while preserving the existing right anchor.
  This moves `Compiler` title `[19,69,114,91] -> [19,69,113,91]` and badge
  `[199,70,241,96] -> [198,70,241,96]` without changing the Browser title or
  badge bboxes.
- **Title font-SHRINK** (useTextAbbr, `ZenCapsule.tsx:26,37` + `css:19-32`):
  the renderer measures the label via `IDWriteTextLayout::GetMetrics` and steps
  the font-size down (1 px steps) to an 8 px floor to fit the capsule with NO
  ellipsis; the `…` trim is applied ONLY at the floor (matching Tauri's
  MIN_FONT_SIZE_PX). The resolved size is memoised (§10: measure runs only on a
  label/width/tier change, never per-frame in the fits-case).
- **Letter-spacing** (`css:27`, V21-C29 branch): default collapsed-pill titles
  keep the source 0.3 px DWrite trailing advance. Large visible-glyph titles use
  the video-observed 0.85 px branch above so the Browser run width matches the
  component crop after the 13.93 px cap; Large no-glyph titles use the C30
  0.2 px branch to remove the `Compiler` one-pixel right overrun. The same
  `IDWriteTextLayout1` `SetCharacterSpacing` seam is used; no new text engine or
  font asset is added.
- **Per-tier badge** (`:34-43/:93-96/:113-116`, C18/C23 adjusted by
  2026-06-02 reference frame): small/medium keep the source CSS metrics, but
  large Browser/Compiler count chips visually sit between the source large chip
  and the medium chip bbox in the authoritative recording. Native therefore keeps
  large capsule/title geometry but uses badge font 10/11/11 px, padding (h,v)
  (6,1)/(9,2)/(9,2), and box height 14/16/17. The local CSS source still
  declares 13px + 3px/12px for
  `.zen-capsule--large .zen-capsule__badge`; the video frame is the higher
  fidelity visual authority for this slice.
- **Minimal shape** (`BentoZone.css:92-99 .bento-zone--shape-minimal`):
  TRANSPARENT background (no acrylic + no surface fill), NO backdrop blur, NO
  shadow/neon glow, and a 1 px DASHED border at rgba(255,255,255,0.2) (radius
  8 px). Other shapes keep the glass + solid `border-zen` hairline.
- **Ordinary non-minimal shadow** (V21-C8/C9/C10 clarification):
  keep the active theme's `shadow_zen` stack. Tauri's ordinary
  `.bento-zone--zen` uses `box-shadow: var(--shadow-zen)`. The removed
  overcorrections were the extra ordinary-capsule sheen and hover fill
  brightening, not the Tauri shadow itself.
- **Icon box runtime contract** (V21-C4): `ZenCapsule.tsx` passes fixed
  `size={18}` into `ZoneIcon`, and `ZoneIcon.css` applies that as
  `--zone-icon-size` for built-in SVG, lucide, custom image, and text fallback
  boxes. The per-size and circle `.zen-capsule__icon { font-size: ... }` CSS
  rules remain on the outer span only, so native keeps every collapsed/circle
  icon box at 18 px.
- **No-glyph icon layout** (V21-C19): an explicit empty/`none` icon name skips
  fallback glyph painting, but still reserves a 6 px video-observed residual
  slot before the title. This lands the right-rail no-icon `Compiler` title
  between the old zero-slot C15 layout and the full visible-glyph 18 px slot,
  matching the 2026-06-02 component crop.
- **Hover green dot REMOVED** (fix 7): the V-14 hover-gated `accent_green` dot
  over the badge had no Tauri analogue (`css:10` only transitions background);
  the count badge now stays visible on hover. No separate always-on status dot
  is painted by the collapsed pill.
- **Hover scale** (fix 8, VERIFIED): `animator::pill_scale_for` returns EXACTLY
  1.0 — `HOVER_SCALE_DELTA`/`PRESS_SCALE_DELTA` are 0.0 (V-12 disabled pill
  scale). Tauri's ZenCapsule has no scale transform, so this already matches;
  left in place (no re-enable). NOT a native deviation.

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
  the per-zone `capsule_size` / `capsule_shape` to drive height +
  corner-radius while the actual icon box stays fixed at Tauri's 18 px; the
  shell hit-rect derives from the same call, so paint-hit parity (V-13) holds
  automatically.
- Title text is **font-shrink-aware** (G5): when the rendered width exceeds the
  capsule, the renderer (`draw_pill_title_shrink_to_fit`) measures the label via
  DWrite `GetMetrics` and shrinks the font-size to fit (no ellipsis) down to an
  8 px floor, mirroring Tauri's `useTextAbbr`. The measure runs only on a
  label/width/tier change (memoised), so the fits-case is allocation-free per
  frame (§10).

Stack capsule variant (V21-C5):
- A collapsed stack anchor no longer reuses the ordinary `ZenCapsule` 214×48
  medium pill. It renders through `StackCapsule` parity geometry:
  `220×52`, radius `24`, padding `10px 12px`, grid gap `10`.
- Inner slots mirror `bentodesk/src/components/BentoZone/StackCapsule.tsx`:
  up to three overlapped 20×20 member peek icons (`slice(-3)`, overlap 6),
  a 28×28 top-member icon bubble containing an 18×18 glyph, a 13px/600 title
  band, and a 24px-high member-count badge at the trailing edge.
- The anchor zone remains the command/hit root, but the visible top icon/title
  come from the last stack member, matching the Tauri sorted stack order.
- Paint, shell hit-test, and chrome region consume
  `zone_pill_geometry::stack_capsule_layout_for_zone`, so the larger stack
  capsule and its clickable region stay pixel-locked.
