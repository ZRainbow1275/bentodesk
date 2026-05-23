/**
 * v8 round-14 / v9 stack-wake-mutex — shared hover-intent timing
 * constants.
 *
 * Pre-round-14, BentoZone (external/free-standing zones) and
 * StackWrapper (bloom petals inside a stack) used independent timing
 * numbers for the same conceptual operations:
 *
 *   - external-zone hover-wake → `getExpandDelay()` (user-configurable;
 *     default 150 ms in `stores/settings.ts`)
 *   - external-zone click-defer → `SINGLE_CLICK_DEFER_MS` (120 ms,
 *     hard-coded in BentoZone.tsx)
 *   - bloom petal hover-intent → `PREVIEW_HOVER_INTENT_MS` (150 ms,
 *     hard-coded in StackWrapper.tsx)
 *   - bloom petal active-revert grace → `ACTIVE_PETAL_GRACE_MS` (80 ms,
 *     hard-coded in StackWrapper.tsx)
 *   - bloom collapse grace → bare `80` magic number on line ~843 of
 *     StackWrapper.tsx
 *
 * The user feedback ("打开stack后内部的zone的唤醒和离开与外部的zone不
 * 一致，统一") demands a single source of truth: a bloomed petal IS
 * conceptually a zone (a member of the stack), so its wake/hover/leave
 * timing must match the way external zones behave when hovered.
 *
 * Round-14 unifies all three call sites against this module's
 * exported constants. The user-configurable `expand_delay_ms` setting
 * keeps a separate identity (some users tune that for slower hover
 * trigger) — its default value aligns with `HOVER_INTENT_MS` so a
 * fresh install behaves identically across zones and petals.
 *
 * v9 timing model (post-PR3 regression fix):
 *   - LEAVE_GRACE_MS stays at 80 ms — required for cursor traversal
 *     between bloom-family elements. A live trace confirms the path
 *     `.stack-wrapper` (capsule natural rect) → `.stack-bloom__petal`
 *     (with 12 px halo) crosses a ~16 px gap that the cursor is on
 *     NEITHER element while in flight (hoveredZone$ flips to null
 *     for that interval). At a typical pointer speed of 500–1000 px/s
 *     the gap takes 16–32 ms to traverse, so an 80 ms cushion gives
 *     the user comfortable headroom before the bloom tears itself
 *     down. PR3 of v9 erroneously reduced this to 0 ms on the
 *     assumption that the hoveredZone$ poller's "non-family hit →
 *     immediate collapse" branch covered the same case — but that
 *     branch only fires when the cursor lands on a NON-family
 *     registered zone, NOT during a transient hovered === null
 *     between two family elements.
 *   - STICKY_GRACE_MS stays at 80 ms (== LEAVE_GRACE_MS). The
 *     committed-preview path (sticky === true) gets a clear
 *     non-family-immediate-collapse signal from the v9 hoveredZone$
 *     effect on a neighbour click, so the sticky tail no longer
 *     needs the legacy 200 ms cushion to debounce committed
 *     teardowns.
 *
 * Constants (do not edit values without a UX decision):
 *   - HOVER_INTENT_MS = 150 — delay before a hover commits to "wake"
 *     (opens preview / expands panel). Short enough to feel responsive
 *     but long enough to skip incidental cursor sweeps across UI.
 *   - LEAVE_GRACE_MS  =  80 — delay before "sleep" commits after the
 *     cursor leaves every registered family element (hovered === null
 *     branch in StackWrapper's hoveredZone$ effect). Bridges the
 *     16 px capsule→petal gap so a cursor mid-traversal does not
 *     accidentally collapse the bloom.
 *   - STICKY_GRACE_MS = 80 — window inside which a sticky preview
 *     (one set by an explicit click) survives a hover-off-then-back
 *     gesture. Same value as LEAVE_GRACE_MS in v9 because the v9
 *     non-family-immediate-collapse path already provides clear
 *     responsiveness for committed teardowns; the sticky tail only
 *     needs to bridge the same family-internal traversal gap.
 *
 * Decision pin (must hold in any future round):
 *   LEAVE_GRACE_MS ≥ ~50 ms is REQUIRED for family-internal cursor
 *   traversal (capsule ↔ petal ↔ floating preview). Only the
 *   non-family hit case takes the immediate-collapse path in
 *   StackWrapper's hoveredZone$ effect. Reducing LEAVE_GRACE_MS to
 *   0 ms regresses the stack hover UX (bloom collapses before the
 *   cursor reaches the petal halo).
 */

/** Delay (ms) before a hover commits to "wake" — opens preview /
 *  expands panel. Used by:
 *    - StackWrapper.handlePetalEnter (bloom petal hover-intent)
 *    - BentoZone click-defer (replaces the legacy hard-coded 120 ms
 *      `SINGLE_CLICK_DEFER_MS` so click-mode external zones share
 *      the same commit window as bloom petal hover)
 *
 *  External-zone HOVER mode still reads `getExpandDelay()` from
 *  settings (user-tunable). The default value of `expand_delay_ms`
 *  in `stores/settings.ts` aligns with this constant — out of the
 *  box every wake path uses 150 ms. */
export const HOVER_INTENT_MS = 150;

/** Delay (ms) before "sleep" commits — collapses the wake state.
 *  Used by:
 *    - StackWrapper.handleMouseLeave (bloom collapse grace, only on
 *      the !isBloomed() hover-tray fallback path; the bloom-active
 *      path is now driven by the hoveredZone$ poller signal)
 *    - StackWrapper.handlePetalLeave (active-petal revert grace)
 *    - StackWrapper hoveredZone$ effect (cursor-left-everything
 *      grace before the bloom tears down — i.e. hovered === null
 *      while the cursor is mid-traversal between two family elements
 *      such as capsule → petal across the 16 px halo gap)
 *
 *  External-zone HOVER mode reads `getCollapseDelay()` from settings
 *  (user-tunable, default 400 ms — longer because external panels
 *  carry more user content and a 400 ms grace prevents accidental
 *  collapse during reading).
 *
 *  v9 stack-wake-mutex (post-PR3 regression fix): kept at 80 ms.
 *  PR3 originally tightened this to 0 ms based on the (incorrect)
 *  assumption that the hoveredZone$ poller's non-family-immediate-
 *  collapse branch covered every leave case. In practice the cursor
 *  spends ~16–32 ms in `hovered === null` while crossing the
 *  capsule→petal gap (capsule rect ends, petal halo starts 12 px
 *  away), and the prior 0 ms value collapsed the bloom before the
 *  cursor ever reached the petal. Restoring 80 ms gives the user
 *  comfortable headroom for family-internal traversal. The
 *  non-family-hit case (cursor lands on a neighbour zone capsule)
 *  is unchanged — the StackWrapper hoveredZone$ effect still tears
 *  the bloom down synchronously on that branch with no grace. */
export const LEAVE_GRACE_MS = 80;

/** Window (ms) inside which a sticky preview (one set by explicit
 *  click) survives a hover-off-then-back gesture. Used by
 *  StackWrapper.handleMouseLeave + the hoveredZone$ effect when the
 *  cursor leaves every registered zone with a sticky preview alive.
 *
 *  v9 stack-wake-mutex: tightened from 200 ms → 80 ms (matches
 *  LEAVE_GRACE_MS). The committed (sticky) path no longer needs the
 *  legacy 200 ms debounce because the v9 hoveredZone$ effect tears
 *  a click-committed preview down synchronously the moment the
 *  cursor lands on a non-family zone. The 80 ms tail covers only
 *  the family-internal hovered === null traversal — same gap, same
 *  headroom. */
export const STICKY_GRACE_MS = 80;
