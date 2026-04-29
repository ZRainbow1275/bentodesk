/**
 * v8 Font-uniformity service — group multiple title elements so they all
 * render at the same font-size, eliminating the visually ragged column that
 * v7 produced when each ItemCard / StackTray member row independently
 * proportionally-shrank to its own width.
 *
 * Contract:
 *   - A `FontGroup` is created at the boundary (BentoPanel, StackTray) by
 *     `createFontGroup(defaultSize)` and exposed via `FontGroupContext`.
 *   - Each member calls `useTextAbbrGroup(fullText)` instead of `useTextAbbr`.
 *     The hook still owns its own ResizeObserver / fonts.ready / measurement
 *     pipeline (so each element knows the size it *would* need on its own),
 *     but it registers that "needed size" with the surrounding group; the
 *     `fontSize` accessor it returns is the group-wide minimum, not the
 *     per-element fitted value.
 *   - When no provider is mounted (the default), `useTextAbbrGroup` falls
 *     back to behaving exactly like `useTextAbbr` so callers like
 *     ZenCapsule / StackCapsule / PanelHeader (which are standalone, not
 *     part of a column) keep their per-element sizing.
 *
 * Why minimum instead of average / median?
 *   - Mixing sizes in one row produces the same ragged effect we're trying
 *     to eliminate. The only size guaranteed to fit every member is the
 *     smallest size any one of them needs; anything larger overflows.
 *   - The downside is that one very long name shrinks every other name in
 *     the panel. That matches the v8 product decision: prefer columns that
 *     read as a uniform block over information density on short names.
 *
 * The group's `groupFontSize` is clamped to MIN_FONT_SIZE_PX so a single
 * pathologically long name can't blow the whole panel down to e.g. 4px.
 *
 * Lifecycle:
 *   - Members register on mount and unregister on cleanup.
 *   - Each member's "needed size" is itself reactive (a Solid Accessor), so
 *     the group's `groupFontSize` createMemo automatically recomputes when
 *     any single member's local fit changes (resize, font load, name edit).
 */

import {
  createSignal,
  createMemo,
  createContext,
  useContext,
  onMount,
  onCleanup,
  type Accessor,
  type Context,
} from "solid-js";
import {
  fitFontSize,
  readFontContext,
  MIN_FONT_SIZE_PX,
  type FitFontContext,
  type FitResult,
  type UseTextAbbrResult,
} from "./textAbbr";

/**
 * The shape exposed to consumers of the FontGroupContext. Members register
 * a reactive "needed size" and subscribe to the group's emitted minimum.
 */
export interface FontGroup {
  /**
   * Register a member with its reactive "needed size" accessor. The id must
   * be unique within the group (a fresh `Symbol().toString()` works fine).
   * Calling `register` with an existing id replaces the old accessor — this
   * keeps things simple if a member's id is derived from a prop that may
   * legitimately change identity.
   */
  register(id: string, neededSize: () => number): void;
  /** Remove a member; harmless if the id was never registered. */
  unregister(id: string): void;
  /**
   * Reactive accessor exposing the group-wide font size — the minimum of
   * every registered member's needed size, clamped to MIN_FONT_SIZE_PX.
   * Empty groups emit the default size supplied at creation.
   */
  groupFontSize: Accessor<number>;
}

/**
 * Build a fresh FontGroup. Call from a parent component (BentoPanel,
 * StackTray) and pipe the returned group through `FontGroupContext.Provider`.
 *
 * `defaultSize` is the CSS-declared default for the children — 11px for
 * ItemCards, 13px for StackTray member rows. The group never returns a size
 * larger than the smallest needed size, so this default only matters before
 * any member has registered (i.e. the first frame after mount).
 */
export function createFontGroup(defaultSize: number): FontGroup {
  // We hold the registered accessors in a Solid signal so changes to the
  // membership set trigger the groupFontSize memo. Storing the Map directly
  // would not — Solid signals compare by reference, and mutating the map in
  // place would be invisible to dependents.
  const [members, setMembers] = createSignal<Map<string, Accessor<number>>>(
    new Map(),
  );

  const register = (id: string, neededSize: () => number): void => {
    setMembers((prev) => {
      const next = new Map(prev);
      next.set(id, neededSize);
      return next;
    });
  };

  const unregister = (id: string): void => {
    setMembers((prev) => {
      if (!prev.has(id)) return prev;
      const next = new Map(prev);
      next.delete(id);
      return next;
    });
  };

  const groupFontSize = createMemo<number>(() => {
    const map = members();
    if (map.size === 0) return defaultSize;
    let min = Number.POSITIVE_INFINITY;
    for (const accessor of map.values()) {
      const v = accessor();
      if (Number.isFinite(v) && v < min) min = v;
    }
    if (!Number.isFinite(min)) return defaultSize;
    if (min < MIN_FONT_SIZE_PX) return MIN_FONT_SIZE_PX;
    return min;
  });

  return { register, unregister, groupFontSize };
}

/**
 * Context handle. Default value is `null` so consumers can detect "no
 * provider mounted" and fall back to standalone (per-element) sizing,
 * matching the v7 useTextAbbr behaviour.
 */
export const FontGroupContext: Context<FontGroup | null> =
  createContext<FontGroup | null>(null);

// ─── Composable ─────────────────────────────────────────────

/**
 * Generate a stable, collision-resistant id for one hook instance. We avoid
 * a global counter (would persist across HMR reloads) and avoid Symbol (the
 * FontGroup keys by string for trivial Map equality semantics).
 */
let nextHookId = 0;
function makeHookId(): string {
  nextHookId += 1;
  return `tab-${nextHookId}`;
}

/**
 * Group-aware drop-in for `useTextAbbr`. Behaviour:
 *
 *   - With a `FontGroupContext` provider: the returned `fontSize()` is the
 *     group's `groupFontSize` (i.e. the column-wide uniform size). The
 *     hook still measures its own needed size locally and registers it,
 *     so the group's minimum is up to date.
 *   - Without a provider: behaves exactly like `useTextAbbr` — the
 *     returned `fontSize()` is the per-element fitted size.
 *
 * `tooltipDisabled()` semantics:
 *   - True when the rendered (group-decided or local) size matches the
 *     CSS-declared default. At smaller sizes the tooltip stays enabled so
 *     users can hover for a normal-weight read of names that were shrunk
 *     either by their own width or by a sibling's pressure.
 *
 * Why duplicate the ResizeObserver / fonts.ready logic from useTextAbbr
 * rather than calling it and overlaying the group? Because we need the
 * locally-measured `neededSize` accessor to register with the group **even
 * when** the visible `fontSize` is the group's minimum. The composition
 * useTextAbbr → group would only expose the already-resolved `fontSize`,
 * and we'd lose the per-member need that drives the minimum. Keeping the
 * measurement pipeline here is the simplest way to expose both numbers.
 */
export function useTextAbbrGroup(fullText: () => string): UseTextAbbrResult {
  const group = useContext(FontGroupContext);

  const [el, setEl] = createSignal<HTMLElement | undefined>();
  const [maxPx, setMaxPx] = createSignal(0);
  const [fontCtx, setFontCtx] = createSignal<FitFontContext>({
    fontFamilyShorthand: "sans-serif",
    defaultFontSizePx: 13,
  });

  onMount(() => {
    const node = el();
    if (!node) return;
    setFontCtx(readFontContext(node));

    let rafId: number | null = null;
    let lastWidth = -1;
    let disposed = false;
    const commitMeasure = () => {
      rafId = null;
      const w = Math.round(node.clientWidth);
      if (w !== lastWidth && w > 0) {
        lastWidth = w;
        setMaxPx(w);
      }
    };
    const measure = () => {
      if (rafId !== null) return;
      rafId =
        typeof requestAnimationFrame === "function"
          ? requestAnimationFrame(commitMeasure)
          : (setTimeout(commitMeasure, 16) as unknown as number);
    };

    // v8 round-4 fix: bound the bootstrap rAF loop. v8.3 dropped the
    // previous MAX_BOOTSTRAP_FRAMES=5 cap to handle slow release WebView2,
    // but with N mounted ItemCards each running an unconditional retry
    // every frame the cumulative cost during home-view idle hit ~N rAF
    // callbacks/frame. Since ResizeObserver is also attached and fires
    // when clientWidth becomes non-zero, we only need the rAF chain to
    // *initialise* the signal — the RO will catch any later width change.
    // 30 frames ≈ 500 ms at 60 Hz is enough headroom for slow dev WebView
    // first-paint without leaving idle frames burning CPU forever.
    const MAX_BOOTSTRAP_FRAMES = 30;
    let bootstrapFrames = 0;
    const bootstrapMeasure = () => {
      if (disposed) return;
      const w = Math.round(node.clientWidth);
      if (w > 0) {
        lastWidth = w;
        setMaxPx(w);
        return;
      }
      bootstrapFrames += 1;
      if (bootstrapFrames >= MAX_BOOTSTRAP_FRAMES) {
        // Give up retrying — RO will still re-measure when layout settles.
        return;
      }
      if (typeof requestAnimationFrame === "function") {
        requestAnimationFrame(bootstrapMeasure);
      } else {
        setTimeout(bootstrapMeasure, 16);
      }
    };
    bootstrapMeasure();

    const fontsApi =
      typeof document !== "undefined"
        ? (document as Document & { fonts?: FontFaceSet }).fonts
        : undefined;
    if (fontsApi) {
      fontsApi.ready
        .then(() => {
          if (disposed) return;
          const w = Math.round(node.clientWidth);
          if (w > 0) {
            lastWidth = -1;
            setMaxPx(w);
          }
        })
        .catch(() => {
          /* fonts API may reject in some embedded WebViews — ignore */
        });
    }

    measure();

    const ro = new ResizeObserver(measure);
    ro.observe(node);
    onCleanup(() => {
      disposed = true;
      ro.disconnect();
      if (rafId !== null) {
        if (typeof cancelAnimationFrame === "function") cancelAnimationFrame(rafId);
        else clearTimeout(rafId as unknown as number);
        rafId = null;
      }
    });
  });

  // Per-element fit (the size this name would need *on its own*).
  const fit = createMemo<FitResult>(() => {
    const width = maxPx();
    const name = fullText();
    return fitFontSize(name, width, fontCtx());
  });
  // v8.3 fix: pre-measurement members must NOT pin the group's minimum at
  // the bootstrap default. Before this change every freshly-mounted member
  // had `maxPx === 0`, so `fit()` returned `defaultFontSizePx` (the only
  // available value before `readFontContext` runs in `onMount`); the
  // group's `groupFontSize` would then settle at that default and never
  // descend, since at least one member always reports the default while
  // panel updates ripple through the For. By emitting +Infinity until a
  // real width is observed, unmeasured members are effectively ignored by
  // `Math.min` — once `setMaxPx` lands a real value the createMemo flips
  // to the true needed size and the group recomputes. The `<= 0` guard
  // matches `fitFontSize`'s own pre-layout short-circuit.
  const localSize = createMemo(() =>
    maxPx() <= 0 ? Number.POSITIVE_INFINITY : fit().fontSizePx,
  );
  const text = createMemo(() => fit().text);

  // Register with the surrounding group (if any). The group sees a live
  // accessor so it recomputes whenever this member's localSize moves.
  if (group) {
    const id = makeHookId();
    group.register(id, localSize);
    onCleanup(() => group.unregister(id));
  }

  // Visible size: group minimum when in a group, local fit otherwise.
  const fontSize = createMemo(() => (group ? group.groupFontSize() : localSize()));
  const tooltipDisabled = createMemo(
    () => fontSize() >= fontCtx().defaultFontSizePx,
  );

  return { setRef: setEl, text, fontSize, tooltipDisabled };
}
