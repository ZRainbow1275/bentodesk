/**
 * anchorOrigin — pure helpers for the BentoZone anchor → CSS transform-origin
 * mapping consumed by `.spring-expand`.
 *
 * The contract is: when the panel grows from a capsule that lives near a
 * screen edge, the anchor snapshot (taken at expand time) tells us which
 * corner the panel is anchored to. The same anchor must drive the
 * `transform-origin` of the spring animation **for both expand AND
 * collapse** so the panel doesn't appear to "fly" toward the wrong
 * corner when retracting back to the capsule.
 *
 * Bug history (#5 in zone-real-fix-v3):
 *   v1.2.2 cleared the anchor snapshot synchronously on collapse, which
 *   reset transform-origin from e.g. `right bottom` back to `left top`
 *   while the spring transition (500ms width/height) was still running.
 *   The visual symptom was the capsule appearing to "flash" toward the
 *   bottom-right corner of the screen as the panel shrank.
 *
 *   The fix keeps the anchor active until the spring transition has
 *   completed, then releases the snapshot. Until then, the
 *   transform-origin computed here keeps the collapse animation pinned
 *   to the same corner the expand animation grew from.
 */

export type AnchorX = "left" | "right";
export type AnchorY = "top" | "bottom";

export interface AnchorSnapshot {
  x: AnchorX;
  y: AnchorY;
  flipOffsetX: number;
  flipOffsetY: number;
}

export interface TransformOriginCss {
  x: AnchorX;
  y: AnchorY;
}

/**
 * Map an anchor snapshot to the CSS keyword pair used in `transform-origin`.
 * Right-anchored panels grow leftward → origin must be on the right edge.
 * Bottom-anchored panels grow upward → origin must be on the bottom edge.
 */
export function computeTransformOrigin(
  snapshot: AnchorSnapshot | null,
): TransformOriginCss {
  if (!snapshot) {
    return { x: "left", y: "top" };
  }
  return {
    x: snapshot.x === "right" ? "right" : "left",
    y: snapshot.y === "bottom" ? "bottom" : "top",
  };
}

/**
 * Approximate maximum duration of `.spring-expand` in milliseconds.
 *
 * `animations.css` defines width/height/--rad transitions at 0.5s with a
 * `cubic-bezier(0.34, 1.56, 0.64, 1)` overshoot curve. The visual settle
 * (after overshoot relaxes) is well within 0.5s, but we add a small
 * cushion so transitionend listeners that key off width/height fire
 * before we drop the snapshot.
 */
export const SPRING_TRANSITION_MS = 520;

/**
 * Decide whether the snapshot may be released.
 *
 * Released when:
 *   - panel is no longer expanded AND the spring transition has finished
 *   - or no snapshot is active in the first place
 *
 * Holding the snapshot during the collapse animation guarantees
 * transform-origin stays on the anchor corner throughout the retract,
 * which is what fixes the "flash to bottom-right" bug.
 */
export function canReleaseSnapshot(opts: {
  expanded: boolean;
  snapshot: AnchorSnapshot | null;
  collapseStartedAt: number | null;
  now?: number;
}): boolean {
  if (opts.snapshot === null) return true;
  if (opts.expanded) return false;
  const startedAt = opts.collapseStartedAt;
  if (startedAt === null) return false;
  const now = opts.now ?? Date.now();
  return now - startedAt >= SPRING_TRANSITION_MS;
}

// ─── Position style decision ────────────────────────────────

/**
 * The pair of CSS positioning declarations a zone should emit on each axis.
 * Exactly one of {left, right} is set, and exactly one of {top, bottom}.
 */
export interface ZonePositionStyle {
  left?: string;
  right?: string;
  top?: string;
  bottom?: string;
}

export interface ComputeZonePositionOpts {
  snapshot: AnchorSnapshot | null;
  /** Capsule home position as percentages of the viewport. */
  pos: { x_percent: number; y_percent: number };
  /** True when the user is dragging the capsule in zen state. */
  isDraggingZen: boolean;
}

/**
 * Pick the positioning declarations a `.bento-zone` element should emit.
 *
 * #5 fix invariant: the SAME side-anchor is used in expanded and zen states
 * whenever a snapshot exists. Previously the style flipped between
 * `right: Npx` (expanded) and `left: X%` (zen) on collapse — two
 * incompatible coordinate systems that the browser cannot interpolate, so
 * the rendered position jumped during the spring transition. The visible
 * symptom was a capsule that "flashed to the bottom-right corner" when
 * retracting near a screen edge.
 *
 * Carve-out: while the user is actively dragging the capsule in zen state,
 * the live `pos` must drive the style. The captured `flipOffsetX/Y` is
 * stale during a drag and would freeze the capsule mid-motion.
 */
export function computeZonePositionStyle(
  opts: ComputeZonePositionOpts,
): ZonePositionStyle {
  const out: ZonePositionStyle = {};
  const snap = opts.snapshot;

  const allowSideAnchor = !opts.isDraggingZen && snap !== null;
  const useRightAnchor = allowSideAnchor && snap.x === "right";
  const useBottomAnchor = allowSideAnchor && snap.y === "bottom";

  if (useRightAnchor) {
    out.right = `${snap.flipOffsetX}px`;
  } else {
    out.left = `${opts.pos.x_percent}%`;
  }

  if (useBottomAnchor) {
    out.bottom = `${snap.flipOffsetY}px`;
  } else {
    out.top = `${opts.pos.y_percent}%`;
  }

  return out;
}
