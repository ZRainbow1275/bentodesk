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

// ─── Anchor flip decision (pure) ────────────────────────────

export interface CapsuleRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface WorkArea {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface DecideAnchorOpts {
  /** Capsule's bounding rect in viewport CSS pixels. */
  rect: CapsuleRect;
  /** Effective panel size after CSS max clamp (px). */
  effPanelW: number;
  effPanelH: number;
  /** Work area (viewport or monitor-resolved) in CSS pixels. */
  work: WorkArea;
  /** Edge-margin from viewport border before flipping (shadow/ring buffer). */
  margin?: number;
  /** Safety band near the bottom/right edge that triggers a flip even if
   * `wouldOverflow*` is just barely false. */
  edgeSafety?: number;
}

/**
 * v8: decide expand-time anchor from capsule rect + work area.
 *
 * Rules (per zone-real-fix-v8 PRD §1):
 *  1. Skip multi-monitor logic at the call site — `work` should already
 *     reflect either viewport or the active monitor.
 *  2. `inLowerHalf` triggers if EITHER capsule center is below work-area
 *     center, OR natural-direction (top anchor) growth would exceed
 *     `workBottom - edgeSafety`.
 *  3. Same for `inRightHalf` on X axis.
 *  4. `flipStillFits*` is NOT a precondition — CSS max-height/width will
 *     clamp the panel if the flipped side is also tight; the visual
 *     intuition "lower half → grow upward" wins.
 */
export function decideAnchorFromRect(opts: DecideAnchorOpts): {
  x: AnchorX;
  y: AnchorY;
} {
  const MARGIN = opts.margin ?? 8;
  const EDGE_SAFETY = opts.edgeSafety ?? 32;
  const { rect, effPanelW, effPanelH, work } = opts;

  const wouldOverflowX = rect.left + effPanelW + MARGIN > work.right;
  const wouldOverflowY = rect.top + effPanelH + MARGIN > work.bottom;

  const spaceBelow = work.bottom - rect.bottom;
  const spaceAbove = rect.bottom - work.top;
  const spaceRight = work.right - rect.right;
  const spaceLeft = rect.right - work.left;

  const nearBottomEdge =
    spaceBelow < effPanelH + MARGIN && spaceAbove >= effPanelH + MARGIN;
  const nearRightEdge =
    spaceRight < effPanelW + MARGIN && spaceLeft >= effPanelW + MARGIN;

  const capsuleCenterX = (rect.left + rect.right) / 2;
  const capsuleCenterY = (rect.top + rect.bottom) / 2;
  const workCenterX = (work.left + work.right) / 2;
  const workCenterY = (work.top + work.bottom) / 2;

  const inLowerHalf =
    capsuleCenterY > workCenterY ||
    rect.top + effPanelH + MARGIN > work.bottom - EDGE_SAFETY;
  const inRightHalf =
    capsuleCenterX > workCenterX ||
    rect.left + effPanelW + MARGIN > work.right - EDGE_SAFETY;

  return {
    x: wouldOverflowX || nearRightEdge || inRightHalf ? "right" : "left",
    y: wouldOverflowY || nearBottomEdge || inLowerHalf ? "bottom" : "top",
  };
}

// ─── Pre-drop anchor pre-computation (DOM-free) ─────────────

export interface ComputeAnchorFromCapsuleOpts {
  /** Capsule home position as percentages of the viewport. */
  pos: { x_percent: number; y_percent: number };
  /** Zen capsule pixel box (matches getCapsuleBoxPx output). */
  capsulePx: { width: number; height: number };
  /** Stored expanded size in viewport percent (0 = use defaults). */
  expandedPct: { w_percent: number; h_percent: number };
  /** Viewport size in CSS pixels. */
  viewport: { width: number; height: number };
  /** Optional resolved monitor work-area. Defaults to viewport bounds. */
  work?: WorkArea;
  /** Edge buffer; matches captureAnchorSnapshot default of 8. */
  margin?: number;
}

/**
 * Predict the anchor a freshly-dropped capsule should adopt, WITHOUT
 * reading any DOM. Used in the drag-mouseup `finally` block to install
 * the correct `right/bottom` anchor in the SAME Solid batch that flips
 * `isDragRepositioning` back to false.
 *
 * Why DOM-free: at mouseup time the rendered element is still showing
 * the dragging-zen layer; its `getBoundingClientRect()` reflects the
 * cursor-following position, not the just-persisted `pos`. Reading the
 * DOM in the next RAF leaves a 1-frame window where the panel re-inflates
 * with a stale `{left: x%, top: y%}` style and visibly over-runs the
 * right edge of the screen — the "flash to left/right" symptom the user
 * reported on every drop near a screen edge.
 *
 * Mirrors `captureAnchorSnapshot` in BentoZone.tsx: same MAX_PANEL_PX
 * clamp (600), same default panel size fallback (360x420), same
 * `decideAnchorFromRect` call, same flipOffset math.
 */
export function computeAnchorFromCapsulePosition(
  opts: ComputeAnchorFromCapsuleOpts,
): AnchorSnapshot {
  const MARGIN = opts.margin ?? 8;
  const MAX_PANEL_PX = 600;

  const capLeft = (opts.pos.x_percent / 100) * opts.viewport.width;
  const capTop = (opts.pos.y_percent / 100) * opts.viewport.height;
  const capRight = capLeft + opts.capsulePx.width;
  const capBottom = capTop + opts.capsulePx.height;

  const cfgW = opts.expandedPct.w_percent;
  const cfgH = opts.expandedPct.h_percent;
  const panelW = cfgW > 0 ? (cfgW / 100) * opts.viewport.width : 360;
  const panelH = cfgH > 0 ? (cfgH / 100) * opts.viewport.height : 420;
  const effPanelW = Math.min(panelW, MAX_PANEL_PX);
  const effPanelH = Math.min(panelH, MAX_PANEL_PX);

  const work: WorkArea = opts.work ?? {
    left: 0,
    top: 0,
    right: opts.viewport.width,
    bottom: opts.viewport.height,
  };

  const { x: anchorX, y: anchorY } = decideAnchorFromRect({
    rect: { left: capLeft, top: capTop, right: capRight, bottom: capBottom },
    effPanelW,
    effPanelH,
    work,
    margin: MARGIN,
  });

  const flipOffsetX = Math.max(0, opts.viewport.width - capRight);
  const flipOffsetY = Math.max(0, opts.viewport.height - capBottom);

  return { x: anchorX, y: anchorY, flipOffsetX, flipOffsetY };
}
