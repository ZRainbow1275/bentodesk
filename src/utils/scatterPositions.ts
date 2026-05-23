/**
 * Stack-dissolve scatter solver — places former stack members in a
 * deterministic, viewport-clamped grid around the original stack
 * position so users don't have to drag overlapping zones apart by
 * hand after dissolving.
 *
 * Background
 * ----------
 * Pre-fix, calling `unstackZonesAction` left every member at the
 * original stack position (their `position.x_percent` /
 * `position.y_percent` were never updated when the stack formed —
 * see `bentodesk/src/stores/stacks.ts:65`). Result: dissolve looked
 * like a no-op because every member sat under the same capsule
 * footprint.
 *
 * Strategy (kept intentionally simple)
 * ------------------------------------
 * 1. Anchor the row at the stack's recorded `(x_percent, y_percent)`.
 *    The first (index 0) member stays AT the anchor — that matches
 *    the user's mental model of "the stack stayed put, the others
 *    fanned out from it".
 * 2. Subsequent members shift right by `capsule_width + GAP_PX` each
 *    step, computed in pixel space against the supplied viewport.
 * 3. If a member's right edge would extend past the viewport (minus
 *    a small inset matching the capsule width), wrap to a new row
 *    `capsule_height + GAP_PX` below the previous row, re-anchoring
 *    x at the original anchor x.
 * 4. After the row layout, every position is clamped to keep the
 *    capsule fully within the viewport (`x ∈ [0, viewport.width -
 *    capsule.width]`, same for y). The clamp is the safety net for
 *    anchors near the right/bottom edges where even index 1 would
 *    overflow without wrapping.
 *
 * The math runs in pixels for clarity; conversion in/out of percent
 * happens at the boundary so the function's contract is "give me a
 * percent-anchored layout, get back a percent-anchored layout" —
 * matching the `BentoZone.position` shape directly.
 *
 * Pure function: no DOM access, no globals. All time-varying inputs
 * (anchor, members, viewport) are explicit parameters so vitest can
 * drive every branch deterministically.
 */

/** Default gap between adjacent capsules / rows in CSS pixels. */
export const SCATTER_GAP_PX = 16;

/**
 * Anchor for the scatter layout — typically the original stack zone's
 * position plus its visible capsule footprint.
 */
export interface ScatterAnchor {
  /** Anchor x in viewport-percent (0–100), top-left corner. */
  x_percent: number;
  /** Anchor y in viewport-percent (0–100), top-left corner. */
  y_percent: number;
  /** Visible capsule width in CSS pixels. Drives spacing + clamp. */
  capsule_width_px: number;
  /** Visible capsule height in CSS pixels. Drives spacing + clamp. */
  capsule_height_px: number;
}

/** Member identity passed in. Position is computed by this function. */
export interface ScatterMember {
  id: string;
}

/** Viewport size in CSS pixels. */
export interface ScatterViewport {
  width: number;
  height: number;
}

/** Result entry — id + a `RelativePosition`-shaped position object. */
export interface ScatteredPosition {
  id: string;
  x_percent: number;
  y_percent: number;
}

/**
 * Compute one scatter target per member, keeping them inside the
 * viewport and (best-effort) non-overlapping.
 *
 * Behaviour pinned by the unit test suite
 * ---------------------------------------
 * - Empty `members` → `[]`.
 * - Single-member input → that member sits AT the anchor (no
 *   movement; matches the user model "the stack stayed where I left
 *   it, the rest fan out").
 * - Multiple members → first stays at anchor, subsequent members
 *   shift right by `capsule.width + gap` each step. When a member
 *   would extend beyond `viewport.width` (less an inset of one
 *   capsule width) the row wraps and the next member resets x to
 *   anchor.x and increments y by `capsule.height + gap`.
 * - Final clamp guarantees every returned position keeps the capsule
 *   fully inside the viewport.
 *
 * The viewport / gap parameters are required (no implicit
 * `window.innerWidth` access) so the function stays pure and
 * trivially testable.
 */
export function computeScatterPositions(
  anchor: ScatterAnchor,
  members: ScatterMember[],
  viewport: ScatterViewport,
  gapPx: number = SCATTER_GAP_PX,
): ScatteredPosition[] {
  if (members.length === 0) return [];
  if (viewport.width <= 0 || viewport.height <= 0) {
    // Degenerate viewport — return the anchor for every member; the
    // clamp branch can't recover from a zero-size viewport so we
    // fall back to "stack stays put" rather than emitting NaN.
    return members.map((m) => ({
      id: m.id,
      x_percent: anchor.x_percent,
      y_percent: anchor.y_percent,
    }));
  }

  const capsuleW = Math.max(0, anchor.capsule_width_px);
  const capsuleH = Math.max(0, anchor.capsule_height_px);
  // Pixel offsets relative to the viewport origin — anchor's top-left.
  const anchorXPx = (anchor.x_percent / 100) * viewport.width;
  const anchorYPx = (anchor.y_percent / 100) * viewport.height;
  // Maximum top-left positions where the capsule still fits fully on
  // screen. When capsule_width >= viewport.width the max becomes 0 —
  // every member collapses back onto x = 0, which is the safest
  // fallback for an over-sized capsule.
  const maxXPx = Math.max(0, viewport.width - capsuleW);
  const maxYPx = Math.max(0, viewport.height - capsuleH);
  // Width of one "column" + gap. When capsule_width is 0 we degrade
  // gracefully by using the gap alone (still produces deterministic
  // offsets rather than NaN / 0-width strides).
  const stepX = capsuleW + gapPx;
  const stepY = capsuleH + gapPx;

  const positions: ScatteredPosition[] = [];
  let cursorX = anchorXPx;
  let cursorY = anchorYPx;

  for (let i = 0; i < members.length; i++) {
    const m = members[i];
    if (i === 0) {
      // First member is the "stack stayed here" anchor.
      positions.push({
        id: m.id,
        x_percent: clampPercent(cursorX, viewport.width, maxXPx),
        y_percent: clampPercent(cursorY, viewport.height, maxYPx),
      });
      cursorX += stepX;
      continue;
    }
    // Row-wrap: if the next placement would push the capsule past the
    // viewport's right edge, drop down a row and reset x. Test against
    // `maxXPx` rather than viewport.width so a position that would
    // require post-clamp overlap forces a wrap instead.
    if (cursorX > maxXPx) {
      cursorX = anchorXPx;
      cursorY += stepY;
    }
    positions.push({
      id: m.id,
      x_percent: clampPercent(cursorX, viewport.width, maxXPx),
      y_percent: clampPercent(cursorY, viewport.height, maxYPx),
    });
    cursorX += stepX;
  }

  return positions;
}

/**
 * Clamp a pixel coordinate to `[0, maxPx]` then convert to a percent
 * of the supplied viewport dimension. Centralised so the same clamp
 * runs on every member regardless of whether the placement came from
 * the row-stride branch or the wrap-reset branch.
 */
function clampPercent(valuePx: number, viewportPx: number, maxPx: number): number {
  if (viewportPx <= 0) return 0;
  const clamped = Math.max(0, Math.min(maxPx, valuePx));
  return (clamped / viewportPx) * 100;
}
