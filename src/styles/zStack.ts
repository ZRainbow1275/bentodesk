/**
 * v9 — Centralised z-index ladder for all free-standing zones AND stack
 * wrappers.
 *
 * Pre-v9, BentoZone.tsx and StackWrapper.tsx each maintained their own
 * z-index logic that DID NOT match:
 *
 *   - `BentoZone` idle:      sort_order + 10  (range ~10-50)
 *   - `BentoZone` hover:     800
 *   - `BentoZone` dragging:  1100
 *   - `BentoZone` expanded:  1000
 *   - `StackWrapper` idle:    sort_order + 30 (range ~30-70)
 *   - `StackWrapper` bloom:   950
 *   - `StackWrapper` drag:    950
 *
 * Net effect: at rest, a stack ALWAYS sat above every idle free zone
 * (30-70 > 10-50), even when their visual rects didn't overlap. The user
 * reports stacks visually rendering on top of their neighbouring free
 * zones, with no semantic reason — sort_order should be the sole arbiter
 * at rest, and stacks should not be "promoted" just by virtue of being
 * stacks.
 *
 * v9 fixes this by defining one ladder both surfaces share. `sort_order`
 * is the SOLE arbiter at rest; the same hover / drag / expand / promoted
 * tiers apply to both. The bloom is treated as a "promoted" state because
 * it is a transient visual elevation analogous to a single-zone expand.
 *
 * If you change a constant here you MUST update the contract test in
 * `src/styles/__tests__/zStack.test.ts`. The test asserts the ladder
 * remains monotonically increasing AND that `Z_ZONE_DRAG > Z_ZONE_EXPANDED
 * > Z_ZONE_PROMOTED > Z_ZONE_HOVER`, because the moved zone must always
 * outrank a hovering neighbour, an expanded panel must outrank a passive
 * neighbour, and a transient promotion (bloom) sits between hover and
 * expanded.
 */

/**
 * Offset added to `zone.sort_order` to derive the resting z-index. Picked
 * so the ladder cannot collide with the static UI chrome at z-index ≤ 5
 * (marquee surface, viewport overlay) while leaving room for ~700 sort
 * orders before saturating at the hover band.
 */
export const Z_ZONE_IDLE_OFFSET = 10;

/**
 * Z-index for a zone (or stack) under cursor hover. Lifts ABOVE any idle
 * zone — sort_order + 10 saturates well below 800 in practice (max
 * sort_order is the user's zone count).
 */
export const Z_ZONE_HOVER = 800;

/**
 * Z-index for a zone (or stack) whose bloom (transient promotion) is
 * active. Bloom petals fly outside the wrapper's natural box, so the
 * wrapper itself must outrank a hovering neighbour or the petals would
 * be visually occluded.
 */
export const Z_ZONE_PROMOTED = 950;

/**
 * Z-index for an EXPANDED zone (deliberate user focus). Outranks bloom
 * because expand is the user's deliberate "look here" while bloom is
 * transient — if both are active simultaneously (rare), the expanded
 * panel wins.
 */
export const Z_ZONE_EXPANDED = 1000;

/**
 * Z-index for a zone (or stack) actively being dragged. Outranks
 * everything because the moved capsule must never disappear under a
 * panel left expanded by a neighbour (e.g. always-mode zone).
 */
export const Z_ZONE_DRAG = 1100;
