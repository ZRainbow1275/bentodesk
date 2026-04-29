/**
 * v8 round-14 — shared hover-intent timing constants.
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
 * Constants (do not edit values without a UX decision):
 *   - HOVER_INTENT_MS = 150 — delay before a hover commits to "wake"
 *     (opens preview / expands panel). Short enough to feel responsive
 *     but long enough to skip incidental cursor sweeps across UI.
 *   - LEAVE_GRACE_MS  =  80 — delay before "sleep" commits after the
 *     cursor leaves the target. A re-entry inside this window cancels
 *     the teardown so the user can graze gaps between adjacent targets
 *     without strobing the active visual cue.
 *   - STICKY_GRACE_MS = 200 — window inside which a sticky preview
 *     (one set by an explicit click) survives a hover-off-then-back
 *     gesture. Longer than LEAVE_GRACE_MS because the user has
 *     committed via click and the threshold for tearing down a
 *     committed surface should be more deliberate.
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
 *    - StackWrapper.handleMouseLeave (bloom collapse grace)
 *    - StackWrapper.handlePetalLeave (active-petal revert grace)
 *
 *  External-zone HOVER mode reads `getCollapseDelay()` from settings
 *  (user-tunable, default 400 ms — longer because external panels
 *  carry more user content and a 400 ms grace prevents accidental
 *  collapse during reading). Bloom petals use the shorter 80 ms
 *  because the bloom is a transient "show options" affordance, not
 *  a committed surface. */
export const LEAVE_GRACE_MS = 80;

/** Window (ms) inside which a sticky preview (one set by explicit
 *  click) survives a hover-off-then-back gesture. Currently consumed
 *  conceptually by the round-13 sticky-swap logic in StackWrapper.
 *  The constant is exported so future call sites (e.g. a "linger"
 *  affordance on external-zone click-mode) can reuse the same
 *  threshold without forking the value. */
export const STICKY_GRACE_MS = 200;
