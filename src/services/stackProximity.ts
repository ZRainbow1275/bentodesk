/**
 * Stack-on-drop proximity detection (v8 round-4 #2).
 *
 * Encapsulates the permissive proximity scoring used by `BentoZone`
 * after a zone-drag settles. Replaces the v8.3 strict center-in-rect
 * test that required the dropped capsule's center to land INSIDE
 * another zone's bounding rect — too brittle for two ~220×52 pill
 * capsules sitting side by side.
 *
 * Trigger logic (EITHER fires):
 *   (a) AABB area overlap ≥ OVERLAP_THRESHOLD of the smaller capsule.
 *   (b) Center-to-center distance ≤ (rSelf + rOther) × PROXIMITY_FACTOR,
 *       where r* is the average half-extent. Kicks in for adjacent
 *       capsules that "kiss" but don't overlap pixel-for-pixel.
 *
 * Among multiple candidates the highest-scoring one wins:
 *   - Any overlapping hit beats every proximity-only hit (overlap
 *     score = overlapRatio + 1, proximity score ≤ 1).
 *   - Within proximity-only hits, closer = higher score.
 *
 * Self-skip rules:
 *   - The dragged zone (`selfId`) is never considered.
 *   - If the dragged zone is already part of a stack, every other
 *     member of THAT stack is skipped (returning a target inside the
 *     same stack would re-stack into a no-op).
 *
 * Pure function: takes plain rects + ids, returns the merge target
 * description or `null`. No DOM, no stores. Tests live alongside.
 */

export interface ProximityCandidate {
  /** Zone id (must be unique within the candidate list). */
  id: string;
  /** Stack the zone currently belongs to, or `null` if free-standing. */
  stackId: string | null;
  /** AABB in client-space pixels. */
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface ProximityInput {
  selfId: string;
  selfStackId: string | null;
  /** Dropped capsule rect in client-space pixels. */
  selfLeft: number;
  selfTop: number;
  selfWidth: number;
  selfHeight: number;
  candidates: ReadonlyArray<ProximityCandidate>;
  overlapThreshold?: number;
  proximityFactor?: number;
}

export interface ProximityHit {
  /** The candidate's id that won the scoring. */
  targetId: string;
  /** Score used to pick the winner (overlap+1 if overlapping, else closeness). */
  score: number;
}

export const DEFAULT_OVERLAP_THRESHOLD = 0.3;
export const DEFAULT_PROXIMITY_FACTOR = 0.8;

/**
 * Decide which candidate (if any) the dropped capsule should stack with.
 * Returns the id of the winning candidate plus its score, or `null` if no
 * candidate satisfies either trigger.
 *
 * The caller is responsible for translating the winning id back into the
 * full set of zone ids that should form the resulting stack (e.g. pulling
 * existing stack members along), since that requires reading the stack
 * registry.
 */
export function findStackProximityHit(input: ProximityInput): ProximityHit | null {
  const overlapThreshold = input.overlapThreshold ?? DEFAULT_OVERLAP_THRESHOLD;
  const proximityFactor = input.proximityFactor ?? DEFAULT_PROXIMITY_FACTOR;

  const selfRight = input.selfLeft + input.selfWidth;
  const selfBottom = input.selfTop + input.selfHeight;
  const selfCenterX = input.selfLeft + input.selfWidth / 2;
  const selfCenterY = input.selfTop + input.selfHeight / 2;
  const selfArea = Math.max(1, input.selfWidth * input.selfHeight);

  let best: ProximityHit | null = null;

  for (const other of input.candidates) {
    if (other.id === input.selfId) continue;
    // Same-stack skip: a zone already grouped with us isn't a valid merge target.
    if (input.selfStackId !== null && other.stackId === input.selfStackId) continue;

    const oRight = other.left + other.width;
    const oBottom = other.top + other.height;
    const oCenterX = other.left + other.width / 2;
    const oCenterY = other.top + other.height / 2;

    const interW = Math.max(0, Math.min(selfRight, oRight) - Math.max(input.selfLeft, other.left));
    const interH = Math.max(0, Math.min(selfBottom, oBottom) - Math.max(input.selfTop, other.top));
    const interArea = interW * interH;
    const otherArea = Math.max(1, other.width * other.height);
    const minArea = Math.min(selfArea, otherArea);
    const overlapRatio = interArea / minArea;

    const dx = selfCenterX - oCenterX;
    const dy = selfCenterY - oCenterY;
    const dist = Math.hypot(dx, dy);
    const rSelf = (input.selfWidth + input.selfHeight) / 4;
    const rOther = (other.width + other.height) / 4;
    const proximityRadius = (rSelf + rOther) * proximityFactor;

    const triggers =
      overlapRatio >= overlapThreshold ||
      (proximityRadius > 0 && dist <= proximityRadius);
    if (!triggers) continue;

    // Score: any overlap beats every proximity-only hit (the +1 ensures
    // a 5% overlap still outranks a near-perfect kiss).
    const score =
      overlapRatio > 0
        ? overlapRatio + 1
        : Math.max(0, 1 - dist / Math.max(1, proximityRadius));

    if (best === null || score > best.score) {
      best = { targetId: other.id, score };
    }
  }

  return best;
}
