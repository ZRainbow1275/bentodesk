/**
 * Petal-row layout solver (v8 round-12 — abandon radial bloom).
 *
 * Round-9..11 placed bloom petals at polar coordinates around the cursor
 * with a collision-avoidance solver. Live testing on the round-11 build
 * showed the radial layout failing visually at viewport edges: when the
 * stack capsule sits near the top-right corner, the solver rotates and
 * expands the ring to dodge clipping, scattering petals to weird
 * positions (one petal below-left of the capsule, another flying down
 * to mid-screen). The user feedback was unambiguous:
 *
 *   "在边框处的显示依然不够优雅，就让两个内容围绕着 stack 胶囊的下方
 *    并列呈现好了" — abandon the radial bloom; petals should always
 *    appear as a horizontal row directly below the stack capsule.
 *
 * Round-12 replaces the radial solver with a far simpler row solver:
 *
 *   1. Lay petals out in a single row, centred horizontally on the
 *      capsule's centre, sitting `gapBelowCapsule` below the capsule's
 *      bottom edge.
 *   2. If the row overflows the viewport's right edge, slide the entire
 *      row left until its right edge sits at `viewport.width - 16`.
 *      Symmetrical for the left edge.
 *   3. If the row would extend below the viewport (capsule near bottom
 *      edge), flip the entire row ABOVE the capsule with the same gap.
 *      `flipped` is set in the result.
 *   4. If the row is too wide to fit horizontally even after sliding
 *      (very large stack on a narrow viewport), wrap to a multi-row
 *      grid centred horizontally below the capsule. `wrapped` is set.
 *
 * Output `centers` are the TOP-LEFT corner of each petal's box in
 * viewport-fixed coordinates — same convention as the previous radial
 * solver, so the caller can keep its `petal.x - PETAL_W / 2` math
 * unchanged. Pure function: no DOM, no globals; takes a capsule rect,
 * petal box size, count, viewport, and optional gaps.
 */

export interface Point {
  x: number;
  y: number;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Viewport {
  width: number;
  height: number;
}

export interface PetalRowOpts {
  /** Capsule rect in viewport-fixed coords (x/y are top-left). */
  capsuleRect: Rect;
  /** Size of one petal box (px). */
  petalSize: Size;
  /** Number of petals (>= 1). */
  petalCount: number;
  /** Viewport size (px). */
  viewport: Viewport;
  /** Horizontal gap between petals (px). Default 12. */
  gap?: number;
  /** Vertical gap between capsule and the petal row (px). Default 16. */
  gapBelowCapsule?: number;
  /** Edge inset — how close to the viewport edge the row may sit before
   *  clamping kicks in. Default 16. */
  viewportInset?: number;
}

export interface PetalRowResolution {
  /** TOP-LEFT corner of each petal's box, in viewport-fixed coords. */
  centers: Point[];
  /** True iff the row was placed ABOVE the capsule (capsule was near
   *  the bottom edge so the row flipped). */
  flipped: boolean;
  /** True iff the row wrapped to multiple rows because the single-row
   *  layout could not fit horizontally. */
  wrapped: boolean;
}

const DEFAULT_GAP_PX = 12;
const DEFAULT_GAP_BELOW_CAPSULE_PX = 16;
const DEFAULT_VIEWPORT_INSET_PX = 16;

/**
 * v8 round-14 — adaptive petal sizing for many-member stacks.
 *
 * Round-12 hard-coded petal size at 108×96 with a 36 px icon. That works
 * cleanly for stacks of ≤ 4 members, but breaks down at scale: a 16-
 * member stack ends up with the wrap solver placing 4 rows of 108×96
 * tiles, dominating the viewport AND animating each at the same full
 * scale and with a full 38 ms-per-petal stagger (608 ms total entry).
 * The user's round-14 feedback flagged "多zone堆叠在一起的性能问题和
 * 显示问题" — both a perf and a display concern.
 *
 * `pickPetalSize` picks one of four size buckets based on the stack's
 * member count:
 *   - ≤ 4   → 108 × 96 px tile, 36 px icon (round-12 default)
 *   - ≤ 8   → 92 × 84 px tile,  32 px icon
 *   - ≤ 16  → 80 × 72 px tile,  28 px icon
 *   - > 16  → 72 × 64 px tile,  24 px icon (compact)
 *
 * Callers feed the resulting `width`/`height` into `resolvePetalRow` so
 * the row geometry uses the appropriate size, and propagate `iconSize`
 * to the rendered icon (via inline `--petal-icon-size` custom prop or
 * direct prop drilling). The thresholds are chosen so a 12-member
 * stack still fits in two rows on a 1920-wide viewport instead of
 * three at the round-12 default size.
 */
export function pickPetalSize(memberCount: number): {
  width: number;
  height: number;
  iconSize: number;
} {
  if (memberCount <= 4) return { width: 108, height: 96, iconSize: 36 };
  if (memberCount <= 8) return { width: 92, height: 84, iconSize: 32 };
  if (memberCount <= 16) return { width: 80, height: 72, iconSize: 28 };
  return { width: 72, height: 64, iconSize: 24 };
}

/**
 * Resolve the petal row layout.
 *
 * Strategy (round-12):
 *   1. Compute single-row total width and centre on the capsule.
 *   2. Clamp horizontally to viewport (with `viewportInset` margin).
 *   3. If row fits horizontally and vertically below capsule → done.
 *   4. If row extends past viewport bottom → flip above capsule, set
 *      `flipped: true`.
 *   5. If row is too wide to fit horizontally even at full viewport
 *      width → wrap to a multi-row grid, set `wrapped: true`.
 *
 * Returns a list of TOP-LEFT corners per petal. Caller is responsible
 * for rendering each petal at `(centers[i].x, centers[i].y)` with size
 * = petalSize.
 */
export function resolvePetalRow(
  opts: PetalRowOpts,
): PetalRowResolution {
  const {
    capsuleRect,
    petalSize,
    petalCount,
    viewport,
    gap = DEFAULT_GAP_PX,
    gapBelowCapsule = DEFAULT_GAP_BELOW_CAPSULE_PX,
    viewportInset = DEFAULT_VIEWPORT_INSET_PX,
  } = opts;

  if (petalCount <= 0) {
    return { centers: [], flipped: false, wrapped: false };
  }

  const capsuleCenterX = capsuleRect.x + capsuleRect.width / 2;
  const capsuleBottom = capsuleRect.y + capsuleRect.height;

  // Total width of a SINGLE row containing every petal, including gaps.
  const singleRowWidth =
    petalCount * petalSize.width + Math.max(0, petalCount - 1) * gap;

  // Available horizontal space inside the viewport (with insets).
  const availableWidth = Math.max(0, viewport.width - 2 * viewportInset);

  // ── Wrap branch ─────────────────────────────────────────────────────
  // If even at full available width the single row cannot fit, fall
  // back to a multi-row grid. Compute petalsPerRow as the maximum
  // number of petals that fit in the available width given the gap.
  if (singleRowWidth > availableWidth) {
    const petalsPerRow = Math.max(
      1,
      Math.floor((availableWidth + gap) / (petalSize.width + gap)),
    );
    return layoutWrappedGrid({
      petalCount,
      petalsPerRow,
      petalSize,
      gap,
      gapBelowCapsule,
      viewport,
      viewportInset,
      capsuleCenterX,
      capsuleRect,
      capsuleBottom,
    });
  }

  // ── Single-row branch ───────────────────────────────────────────────
  // Centre the row on capsuleCenterX.
  let rowLeft = capsuleCenterX - singleRowWidth / 2;
  const rowRight = (left: number): number => left + singleRowWidth;

  // Clamp to right edge.
  const maxLeft = viewport.width - viewportInset - singleRowWidth;
  if (rowRight(rowLeft) > viewport.width - viewportInset) {
    rowLeft = maxLeft;
  }
  // Clamp to left edge.
  if (rowLeft < viewportInset) {
    rowLeft = viewportInset;
  }

  // Vertical placement: default below the capsule.
  let rowTop = capsuleBottom + gapBelowCapsule;
  let flipped = false;
  // Flip above if the row would overflow the viewport bottom.
  if (rowTop + petalSize.height > viewport.height - viewportInset) {
    rowTop = capsuleRect.y - gapBelowCapsule - petalSize.height;
    flipped = true;
  }

  const centers: Point[] = new Array(petalCount);
  for (let i = 0; i < petalCount; i++) {
    centers[i] = {
      x: rowLeft + i * (petalSize.width + gap),
      y: rowTop,
    };
  }

  return { centers, flipped, wrapped: false };
}

interface WrappedGridOpts {
  petalCount: number;
  petalsPerRow: number;
  petalSize: Size;
  gap: number;
  gapBelowCapsule: number;
  viewport: Viewport;
  viewportInset: number;
  capsuleCenterX: number;
  capsuleRect: Rect;
  capsuleBottom: number;
}

/**
 * Lay petals out as a multi-row grid centred horizontally below (or
 * above, if necessary) the capsule. Each row has up to `petalsPerRow`
 * petals; the final row may be shorter and is centred independently
 * so visually the cluster reads as a centred block.
 */
function layoutWrappedGrid(opts: WrappedGridOpts): PetalRowResolution {
  const {
    petalCount,
    petalsPerRow,
    petalSize,
    gap,
    gapBelowCapsule,
    viewport,
    viewportInset,
    capsuleCenterX,
    capsuleRect,
    capsuleBottom,
  } = opts;

  const totalRows = Math.ceil(petalCount / petalsPerRow);
  const totalGridHeight =
    totalRows * petalSize.height + (totalRows - 1) * gap;

  // Vertical placement: default below the capsule. Flip if the whole
  // grid would overflow the viewport bottom.
  let gridTop = capsuleBottom + gapBelowCapsule;
  let flipped = false;
  if (gridTop + totalGridHeight > viewport.height - viewportInset) {
    gridTop = capsuleRect.y - gapBelowCapsule - totalGridHeight;
    flipped = true;
  }

  const centers: Point[] = new Array(petalCount);
  for (let row = 0; row < totalRows; row++) {
    const startIdx = row * petalsPerRow;
    const endIdx = Math.min(startIdx + petalsPerRow, petalCount);
    const petalsInThisRow = endIdx - startIdx;
    const rowWidth =
      petalsInThisRow * petalSize.width +
      Math.max(0, petalsInThisRow - 1) * gap;

    // Centre this row on the capsule's centre, then clamp to viewport.
    let rowLeft = capsuleCenterX - rowWidth / 2;
    const maxLeft = viewport.width - viewportInset - rowWidth;
    if (rowLeft + rowWidth > viewport.width - viewportInset) {
      rowLeft = maxLeft;
    }
    if (rowLeft < viewportInset) {
      rowLeft = viewportInset;
    }

    const rowY = gridTop + row * (petalSize.height + gap);
    for (let i = 0; i < petalsInThisRow; i++) {
      centers[startIdx + i] = {
        x: rowLeft + i * (petalSize.width + gap),
        y: rowY,
      };
    }
  }

  return { centers, flipped, wrapped: true };
}
