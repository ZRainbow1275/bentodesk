/**
 * BentoZone — The primary zone component.
 * Two visual states: zen (collapsed capsule) and bento (expanded panel).
 * Manages hover intent timers for expand/collapse transitions.
 *
 * Architecture: Both ZenCapsule and BentoPanel are always mounted in the DOM.
 * The transition between states is driven by CSS class toggling on the container,
 * which enables smooth CSS transitions on width/height/border-radius/background.
 * Inner layers use opacity/visibility cross-fade to switch visible content.
 */
import { Component, Show, batch, createEffect, createMemo, createSignal, onMount, onCleanup, untrack } from "solid-js";
import type { BentoZone as BentoZoneType } from "../../types/zone";
import { isZoneExpanded, expandZone, collapseZone, getViewportSize } from "../../stores/ui";
import { getExpandDelay, getCollapseDelay, getZoneDisplayMode } from "../../stores/settings";
import { isDragging, setIsDragging } from "../../stores/dragging";
import { monitorForClientRect, cachedMonitors } from "../../services/geometry";
import {
  createHitTestHandlers,
  registerZoneElement,
  unregisterZoneElement,
  updateZoneInflate,
  acquireDragLock,
  computeInflateForPosition,
  getCapsuleBoxPx,
} from "../../services/hitTest";
import { getDebugOverlayEnabled } from "../../stores/settings";
import {
  createDropHandlers,
  activeDropZone,
  registerDropZoneElement,
  unregisterDropZoneElement,
} from "../../services/dropTarget";
import { internalDrag } from "../../services/drag";
import { bulkUpdateZones, preloadIcons } from "../../services/ipc";
import { loadZones, updateZone, zonesStore } from "../../stores/zones";
import { stackZonesAction, stackMap, zoneStackId } from "../../stores/stacks";
import {
  computeTransformOrigin,
  computeZonePositionStyle,
  decideAnchorFromRect,
  SPRING_TRANSITION_MS,
} from "../../services/anchorOrigin";
import { HOVER_INTENT_MS } from "../../services/hoverIntent";
import {
  Z_ZONE_IDLE_OFFSET,
  Z_ZONE_HOVER,
  Z_ZONE_EXPANDED,
  Z_ZONE_DRAG,
} from "../../styles/zStack";
import {
  beginGroupZoneDrag,
  endGroupZoneDrag,
  getGroupDragPreviewPosition,
  isZoneMultiSelected,
  selectZone,
  selectedZoneIds,
  updateGroupZoneDrag,
} from "../../stores/selection";
import ZenCapsule from "./ZenCapsule";
import BentoPanel from "./BentoPanel";
import "./BentoZone.css";

interface BentoZoneProps {
  zone: BentoZoneType;
  interactionMode?: "standalone" | "stack-member";
}

/** Batch size for idle-scheduled icon preload. Matches the tokio
 * extractor's thread-pool depth so batches don't queue behind each
 * other during the expand animation. */
const PRELOAD_BATCH = 8;
const ZONE_DRAG_THRESHOLD_PX = 4;

type IdleSchedule = (cb: () => void) => void;

const scheduleIdle: IdleSchedule =
  typeof window !== "undefined" && typeof window.requestIdleCallback === "function"
    ? (cb) => {
        window.requestIdleCallback(() => cb(), { timeout: 200 });
      }
    : (cb) => {
        window.setTimeout(cb, 1);
      };

function schedulePreloadBatches(paths: string[]) {
  for (let i = 0; i < paths.length; i += PRELOAD_BATCH) {
    const slice = paths.slice(i, i + PRELOAD_BATCH);
    scheduleIdle(() => {
      void preloadIcons(slice);
    });
  }
}

const BentoZone: Component<BentoZoneProps> = (props) => {
  let expandTimer: ReturnType<typeof setTimeout> | null = null;
  let collapseTimer: ReturnType<typeof setTimeout> | null = null;
  let zoneRef: HTMLDivElement | undefined;
  const [isDragRepositioning, setIsDragRepositioning] = createSignal(false);
  const [isHovering, setIsHovering] = createSignal(false);
  const [dragOffset, setDragOffset] = createSignal({ x: 0, y: 0 });
  // R2 hover-lock: timestamp (Date.now-style) before which we must not
  // collapse. Populated when the expand timer fires so the overshoot
  // spring can't race mouseleave.
  const [expandLockUntil, setExpandLockUntil] = createSignal(0);
  // R1 anchor snapshot: recomputed only on collapse→expand transition
  // so the panel never jumps between `left` and `right` mid-animation.
  const [anchorSnapshot, setAnchorSnapshot] = createSignal<{
    x: "left" | "right";
    y: "top" | "bottom";
    flipOffsetX: number;
    flipOffsetY: number;
  } | null>(null);
  // #5 fix: snapshot is held through the collapse transition so transform-origin
  // stays pinned to the anchor corner until the spring has fully retracted. We
  // schedule a deferred release once collapseZone() runs.
  let snapshotReleaseTimer: ReturnType<typeof setTimeout> | null = null;
  const clearSnapshotReleaseTimer = () => {
    if (snapshotReleaseTimer !== null) {
      clearTimeout(snapshotReleaseTimer);
      snapshotReleaseTimer = null;
    }
  };
  const scheduleSnapshotRelease = () => {
    clearSnapshotReleaseTimer();
    snapshotReleaseTimer = setTimeout(() => {
      snapshotReleaseTimer = null;
      // Only clear if we're still collapsed — a re-expand re-captures.
      if (!expanded()) {
        setAnchorSnapshot(null);
      }
    }, SPRING_TRANSITION_MS);
  };

  // ─── Resize state ────────────────────────────────────────────
  const [isResizing, setIsResizing] = createSignal(false);
  // Local resize signal — updated every mousemove for instant visual feedback
  const [resizeSize, setResizeSize] = createSignal<{
    w_percent: number;
    h_percent: number;
  } | null>(null);

  const hitTestHandlers = createHitTestHandlers();
  const dropHandlers = createDropHandlers(props.zone.id);
  const isStackMember = () => props.interactionMode === "stack-member";
  const capsulePixels = () =>
    getCapsuleBoxPx(props.zone.capsule_shape, props.zone.capsule_size);
  // Fix-FE-7: global setting now wins over the per-zone override so the
  // Settings picker behaves authoritatively. v1.2.3 had per-zone first
  // which silently shadowed any global change for zones that had ever
  // been touched by the bulk manager — picker felt "no-op" to users.
  // The per-zone override only applies as a fallback when the global
  // setting is absent (legacy migration path).
  // v6 fix #2: wrapped in createMemo so SolidJS tracks the underlying
  // settings signal explicitly. Plain `() =>` worked when the value was
  // dereferenced inside a tracking scope (effect, jsx), but click /
  // mouse-enter handlers read it imperatively — without the memo the
  // `display_mode` cascade was sometimes evaluated against a stale
  // capture from the first render, which is exactly why the picker
  // appeared to "fall back to hover" after switching it to always/click.
  const zoneDisplayMode = createMemo(
    () => getZoneDisplayMode() ?? props.zone.display_mode ?? "hover",
  );
  const zoneLocked = () => props.zone.locked === true;
  const zoneSelected = () => !isStackMember() && isZoneMultiSelected(props.zone.id);

  const expanded = () => isZoneExpanded(props.zone.id);
  const isDropTarget = () => activeDropZone() === props.zone.id;
  const isHoverIntentSuspended = () =>
    isDragRepositioning() || isResizing();

  /** True when a cross-zone internal drag hovers over this zone. */
  const isCrossDragHover = () => {
    const drag = internalDrag();
    return (
      drag !== null &&
      drag.targetZoneId === props.zone.id &&
      drag.sourceZoneId !== props.zone.id
    );
  };

  // Register this zone's DOM element for cursor position hit-testing and drop targeting
  onMount(() => {
    if (zoneRef) {
      registerZoneElement(zoneRef, {
        inflate: computeInflateForPosition(props.zone.position, {
          kind: isStackMember() ? "stack" : "zone",
          boxPx: capsulePixels(),
        }),
      });
      registerDropZoneElement(zoneRef, props.zone.id);
    }
    // v1.2.1 — `always` mode: mount the zone already expanded so the user
    // sees a Fences-style persistent panel without any hover trigger.
    if (!isStackMember() && zoneDisplayMode() === "always") {
      expandZone(props.zone.id);
    }
  });

  // v1.2.1 — react to runtime display-mode switches. Flipping into `always`
  // immediately pops every zone open; flipping out leaves the current state
  // alone (next hover-leave / re-click will collapse naturally).
  createEffect(() => {
    if (!isStackMember() && zoneDisplayMode() === "always" && !expanded()) {
      expandZone(props.zone.id);
    }
  });

  // v8 fix #2 (round 3): collapse only on the always→non-always TRANSITION.
  // Round 2 removed the `!isHovering()` guard but accidentally made the
  // effect re-fire on EVERY expand under hover/click mode — the moment the
  // user hovered a zone, expanded() flipped true → effect saw mode="hover"
  // and immediately collapsed it, making zones feel uninteractive. Track
  // the previous mode and only force-collapse when the picker actually
  // leaves "always". The hover/click code paths handle their own collapse
  // via mouseleave / explicit click.
  let prevDisplayMode: "hover" | "always" | "click" | undefined = undefined;
  createEffect(() => {
    const mode = zoneDisplayMode();
    const wasAlways = prevDisplayMode === "always";
    prevDisplayMode = mode;
    if (!isStackMember() && wasAlways && mode !== "always" && expanded()) {
      if (import.meta.env.DEV) {
        // eslint-disable-next-line no-console
        console.debug("[picker-pre-collapse]", { zoneId: props.zone.id, mode });
      }
      collapseZone(props.zone.id);
      setAnchorSnapshot(null);
    }
  });

  createEffect(() => {
    if (isStackMember() && expanded()) {
      collapseZone(props.zone.id);
      setAnchorSnapshot(null);
    }
  });

  // D1: refresh inflate when zone position changes (user drags capsule).
  createEffect(() => {
    // Touch position to subscribe the effect; computeInflateForPosition reads it internally.
     void props.zone.position;
    if (zoneRef) {
      updateZoneInflate(
        zoneRef,
        computeInflateForPosition(props.zone.position, {
          kind: isStackMember() ? "stack" : "zone",
          boxPx: capsulePixels(),
        }),
      );
    }
  });

  const clearTimers = () => {
    if (expandTimer !== null) {
      clearTimeout(expandTimer);
      expandTimer = null;
    }
    if (collapseTimer !== null) {
      clearTimeout(collapseTimer);
      collapseTimer = null;
    }
  };

  /**
   * R1: compute expand-time anchor snapshot. Uses the capsule's current
   * bounding rect + its containing monitor work-area (when available)
   * to decide whether growing rightward / downward would overflow. The
   * snapshot is frozen for the lifetime of the expanded state so
   * animation doesn't flip mid-transition.
   */
  const captureAnchorSnapshot = () => {
    if (!zoneRef) {
      setAnchorSnapshot(null);
      return;
    }
    const rect = zoneRef.getBoundingClientRect();
    const vp = getViewportSize();

    // Panel expanded dims in CSS pixels (match CSS min/max + stored size)
    const cfgW = props.zone.expanded_size.w_percent;
    const cfgH = props.zone.expanded_size.h_percent;
    const panelW = cfgW > 0 ? (cfgW / 100) * vp.width : 360;
    const panelH = cfgH > 0 ? (cfgH / 100) * vp.height : 420;

    // Margin from the edge at which we flip (account for shadow/ring)
    const MARGIN = 8;

    // v8 fix #1: BentoDesk overlay is a single-monitor full-screen webview on
    // the user's machine. The multi-monitor monitor-resolve path was masking
    // the simple case (on release builds the IPC sometimes returned a stale
    // primary). Trust the viewport unless we genuinely have >1 monitor.
    const cached = cachedMonitors();
    let workLeft = 0;
    let workTop = 0;
    let workRight = vp.width;
    let workBottom = vp.height;

    if (cached && cached.length > 1) {
      const mon = monitorForClientRect(rect);
      if (mon) {
        // D1.3: use per-monitor dpi_scale when available so secondary
        // displays with different DPI resolve work-area correctly. Falls back
        // to window.devicePixelRatio (which always reflects the primary) when
        // MonitorInfo didn't provide a scale value.
        const dpr = mon.dpi_scale && mon.dpi_scale > 0
          ? mon.dpi_scale
          : (window.devicePixelRatio || 1);
        workLeft = mon.rect_work.x / dpr;
        workTop = mon.rect_work.y / dpr;
        workRight = (mon.rect_work.x + mon.rect_work.width) / dpr;
        workBottom = (mon.rect_work.y + mon.rect_work.height) / dpr;
      }
    }

    // Clamp against CSS max (600px) so overflow test matches actual render size.
    const MAX_PANEL_PX = 600;
    const effPanelW = Math.min(panelW, MAX_PANEL_PX);
    const effPanelH = Math.min(panelH, MAX_PANEL_PX);

    // v8: pure decision. See `decideAnchorFromRect` for the lower-half /
    // right-half rules and the 32px edge-safety band that replaces v7's
    // `flipStillFitsY/X` precondition.
    const { x: anchorX, y: anchorY } = decideAnchorFromRect({
      rect: {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
      },
      effPanelW,
      effPanelH,
      work: {
        left: workLeft,
        top: workTop,
        right: workRight,
        bottom: workBottom,
      },
      margin: MARGIN,
    });

    // When anchored right/bottom, `right:` / `bottom:` is measured from
    // the viewport edge, which equals (viewport - rect.right/bottom).
    const flipOffsetX = Math.max(0, vp.width - rect.right);
    const flipOffsetY = Math.max(0, vp.height - rect.bottom);

    if (
      typeof window !== "undefined" &&
      (window as unknown as { __bento_debug_anchor?: boolean })
        .__bento_debug_anchor
    ) {
      // eslint-disable-next-line no-console
      console.log("[anchor]", {
        rect: {
          top: rect.top,
          left: rect.left,
          right: rect.right,
          bottom: rect.bottom,
        },
        vp,
        cached: cached ? cached.length : null,
        anchor: { x: anchorX, y: anchorY, flipOffsetX, flipOffsetY },
      });
    }

    setAnchorSnapshot({
      x: anchorX,
      y: anchorY,
      flipOffsetX,
      flipOffsetY,
    });
  };

  // v8 round-2 fix: defensive anchor capture for ALL paths that flip
  // `expanded()` to true without going through `triggerExpand`. Smoking-gun
  // bypass cases:
  //   1. `always` display-mode at mount (line ~187: expandZone in onMount)
  //   2. `always` runtime switch (line ~196: createEffect on display-mode)
  //   3. App.tsx keyboard nav `expandZone(focusedZone)` (App.tsx:493)
  //   4. ContextMenu "Search in zone" (ContextMenu.tsx:375)
  // All four set the global `expandedZoneIds` signal directly. Without this
  // effect the panel paints with snapshot=null → `computeZonePositionStyle`
  // emits `{left: x%, top: y%}` and the panel grows downward+rightward
  // even when the capsule sits in the lower half of the screen.
  let prevExpandedForAnchor = false;
  createEffect(() => {
    const isExp = expanded();
    if (
      isExp &&
      !prevExpandedForAnchor &&
      untrack(() => anchorSnapshot()) === null
    ) {
      captureAnchorSnapshot();
    }
    prevExpandedForAnchor = isExp;
  });

  // D1: velocity tracker for fast-path hover trigger.
  // Last seen cursor coords + timestamp, reset on enter.
  let lastPosX = 0;
  let lastPosY = 0;
  let lastPosTime = 0;
  const VELOCITY_THRESHOLD_PX_PER_SEC = 800;

  const triggerExpand = (fastPath: boolean) => {
    // Cancel any pending snapshot release left from a recent collapse so
    // the freshly-captured anchor isn't nulled mid-spring.
    clearSnapshotReleaseTimer();
    // Freeze the anchor direction before flipping state → expanded
    // so `zoneStyle()` observes a stable snapshot on first paint.
    captureAnchorSnapshot();
    expandZone(props.zone.id);
    // R2: block collapse scheduling so the spring settles before honouring
    // a transient mouseleave caused by animation. Shorter window for
    // fast-path so lock doesn't feel like "stickiness".
    setExpandLockUntil(Date.now() + (fastPath ? 300 : 550));
    // Preload icons in idle batches so the tokio extractor can't
    // saturate while the spring animation is still running.
    // Each batch is 8 paths — small enough to finish in one idle
    // slot on a cold machine, large enough that 30-item zones
    // complete within ~2 frames worth of idle time.
    const paths = props.zone.items.map((i) => i.path);
    if (paths.length > 0) {
      schedulePreloadBatches(paths);
    }
  };

  const handleMouseEnter = (e: MouseEvent) => {
    // v6 fix #3: dragging guard fires BEFORE hit-test bookkeeping so a
    // drag in progress can't race a hover-expand timer onto another
    // zone the cursor sweeps across. We still update hover state for
    // visual feedback (z-index lift) but never schedule expand.
    if (isDragging()) return;
    hitTestHandlers.onPointerEnter();
    setIsHovering(true);
    clearTimers();
    if (isStackMember()) return;
    if (isHoverIntentSuspended()) return;
    // v1.2.1 — only `hover` mode schedules an expand on pointer entry.
    // `always` is already expanded at mount; `click` only reacts to a
    // deliberate click so hovering alone must not flicker the capsule.
    if (zoneDisplayMode() !== "hover") return;
    // Seed velocity baseline at enter so the first onMouseMove computes from a
    // stable reference (movementX/Y is unreliable on first frame).
    lastPosX = e.clientX;
    lastPosY = e.clientY;
    lastPosTime = performance.now();
    if (!expanded()) {
      expandTimer = setTimeout(() => {
        triggerExpand(false);
      }, getExpandDelay());
    }
  };

  // D1: on every mousemove inside the zone, sample cursor velocity.
  // If > VELOCITY_THRESHOLD_PX_PER_SEC the user is "slashing through" — fire the
  // expand immediately so the interaction feels responsive on fast flicks.
  const handleMouseMove = (e: MouseEvent) => {
    if (isStackMember()) return;
    if (expanded() || isHoverIntentSuspended()) return;
    // v6 fix #3: never fire the velocity fast-path while a drag is live —
    // a flick across another zone during a drag must not pop it open.
    if (isDragging()) return;
    // Velocity fast-path only applies to `hover` mode — `click` mode must
    // never expand on movement alone, even a "slashing" gesture.
    if (zoneDisplayMode() !== "hover") return;
    const now = performance.now();
    const dt = now - lastPosTime;
    if (dt > 0 && dt < 200) {
      const dx = e.clientX - lastPosX;
      const dy = e.clientY - lastPosY;
      const dist = Math.hypot(dx, dy);
      const velocity = (dist / dt) * 1000;
      if (velocity > VELOCITY_THRESHOLD_PX_PER_SEC && expandTimer !== null) {
        clearTimeout(expandTimer);
        expandTimer = null;
        triggerExpand(true);
      }
    }
    lastPosX = e.clientX;
    lastPosY = e.clientY;
    lastPosTime = now;
  };

  // v6 fix #1 (revert v5 over-eager click=expand): single click ONLY expands
  // in `click` display mode. The v5 build wired single-click to expand for
  // all three modes — that flipped the user's documented contract:
  //   - hover mode  → mouse-over expands (default)
  //   - always mode → mounted expanded, no click action
  //   - click mode  → single click expands; click outside collapses
  // The earlier "click also expands hover-mode zones" behaviour caused two
  // visible regressions: (a) selecting a zone with a single click triggered
  // an unwanted expand spring, and (b) the panel grew rightward/downward
  // even when the capsule sat at the bottom-right of the screen, because
  // the click path ran triggerExpand BEFORE the hover-path captured an
  // anchor — so the panel had no flip-snapshot to consult.
  // Now triggerExpand is the single funnel that always runs
  // captureAnchorSnapshot first, so the click path inherits the same
  // bottom-right avoidance the hover path always had. We still defer with
  // SINGLE_CLICK_DEFER_MS so a follow-up dblclick (selection toggle) can
  // cancel the pending expand.
  //
  // v8 round-14 (unify hover-intent): the click-defer commit window is
  // unified with the bloom petal hover-intent threshold via the shared
  // `HOVER_INTENT_MS` constant. Pre-round-14 this used a stand-alone 120 ms
  // value, while bloom petals used 150 ms and the settings default
  // `expand_delay_ms` was also 150 ms. The 30 ms gap had no UX
  // justification — it was just an inherited-from-v5 magic number.
  // Aligning to HOVER_INTENT_MS keeps every "wake commit" window
  // consistent across external zones and bloom petals so the user
  // doesn't perceive different "feels" between the two surfaces.
  // NOTE: hover mode's expand timer still reads `getExpandDelay()` from
  // user settings — that value is intentionally configurable per the
  // settings picker. The default in `stores/settings.ts` aligns with
  // HOVER_INTENT_MS so a fresh install matches the click-defer window.
  const SINGLE_CLICK_DEFER_MS = HOVER_INTENT_MS;
  let pendingExpandTimer: ReturnType<typeof setTimeout> | null = null;
  const clearPendingExpand = () => {
    if (pendingExpandTimer !== null) {
      clearTimeout(pendingExpandTimer);
      pendingExpandTimer = null;
    }
  };

  const handleZoneClick = (e: MouseEvent) => {
    if (isStackMember()) return;
    if (e.button !== 0) return;
    if (expanded()) return;
    // v6 fix #3: a click event is dispatched after a drag mouseup. If a
    // drag was just released, swallow the click so we don't kick off an
    // expand spring on the freshly-dropped zone.
    if (isDragging() || isDragRepositioning()) return;
    // v6 fix #1: only `click` mode treats a single click as an expand
    // gesture. `hover` and `always` modes intentionally do NOT expand on
    // click — hover relies on pointer-enter, always is mounted expanded.
    if (zoneDisplayMode() !== "click") return;
    const target = e.target as HTMLElement | null;
    if (target && target.closest(".bento-zone__resize-handle")) return;
    if (target && target.closest(".bento-panel__header")) return;
    // Defer the expand so a follow-up dblclick can cancel it. We capture
    // detail at scheduling time — by the time the timer fires the original
    // event is gone.
    clearPendingExpand();
    pendingExpandTimer = setTimeout(() => {
      pendingExpandTimer = null;
      // Re-check expanded state in case another path expanded us during
      // the defer window (hover-intent timer, programmatic etc). The
      // anchor snapshot is captured inside triggerExpand BEFORE state
      // flips, so the click path automatically inherits the screen-edge
      // avoidance / flip-toward-left-top behaviour of the hover path.
      if (!expanded() && !isDragging()) {
        triggerExpand(false);
      }
    }, SINGLE_CLICK_DEFER_MS);
  };

  const handleZoneDblClick = (e: MouseEvent) => {
    if (isStackMember()) return;
    if (e.button !== 0) return;
    const target = e.target as HTMLElement | null;
    if (target && target.closest(".bento-zone__resize-handle")) return;
    // Cancel any pending single-click expand so dblclick is a pure
    // selection gesture — never both expanding AND selecting at once.
    clearPendingExpand();
    e.preventDefault();
    e.stopPropagation();
    // v5 Fix #C2: plain dblclick is a TOGGLE (using selectZone's `ctrl=true`
    // path that adds-or-removes). shift dblclick falls through to range
    // semantics with the lastZoneAnchor in selection store. This way a
    // second dblclick on a selected zone deselects it without forcing the
    // user to ctrl/shift-click — matching the "natural toggle" feel.
    const additive = e.shiftKey || e.ctrlKey || e.metaKey;
    selectZone(props.zone.id, {
      shift: e.shiftKey,
      // Plain dblclick → ctrl=true so it toggles instead of replacing the
      // entire selection; modifier dblclick keeps its richer behaviour
      // (shift = range, ctrl/meta = additive toggle).
      ctrl: !additive ? true : (e.ctrlKey || e.metaKey),
      orderedIds: zonesStore.zones.map((zone) => zone.id),
    });
  };

  const handleMouseLeave = () => {
    hitTestHandlers.onPointerLeave();
    setIsHovering(false);
    clearTimers();
    if (isStackMember()) return;
    if (isHoverIntentSuspended()) return;
    // `always` mode: zones are persistently expanded — mouse leave is a no-op.
    if (zoneDisplayMode() === "always") return;
    if (expanded()) {
      // R2: if we're still inside the post-expand lock window, defer
      // the collapse until after the spring settles. We schedule a
      // single timer; the lock window is short so this doesn't feel
      // laggy in practice.
      const now = Date.now();
      const lockRemain = expandLockUntil() - now;
      const baseDelay = getCollapseDelay();
      const effectiveDelay = lockRemain > 0
        ? Math.max(baseDelay, lockRemain)
        : baseDelay;
      collapseTimer = setTimeout(() => {
        collapseZone(props.zone.id);
        // #5 fix: don't drop the snapshot synchronously — keep it for the
        // duration of the spring transition so transform-origin stays
        // pinned to the anchor corner while the panel retracts. The
        // deferred release re-evaluates against current capsule position
        // on the next expand.
        scheduleSnapshotRelease();
      }, effectiveDelay);
    }
  };

  // Local drag position signal — updated on every mousemove, no IPC overhead
  const [dragPosition, setDragPosition] = createSignal<{
    x_percent: number;
    y_percent: number;
  } | null>(null);

  const handleZoneMouseDown = (e: MouseEvent) => {
    if (isStackMember()) return;
    if (e.button !== 0) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest(".bento-zone__resize-handle")) return;
    // Don't intercept clicks bubbling up from PanelHeader (its onMouseDown
    // already starts a header-driven drag). When expanded, the BentoPanel
    // surface should not start a group drag — only the capsule body does.
    if (target?.closest(".bento-panel__header")) return;

    const currentSelection = selectedZoneIds();
    const isCollapsedSurface = !expanded();
    const wasInSelection =
      currentSelection.has(props.zone.id) && currentSelection.size > 1;

    // Capsule-driven group drag: if the user mousedowns the collapsed capsule
    // of a zone that is part of a multi-selection, drag the whole selection.
    // Single-zone selection or non-multi-select still falls through to the
    // existing single-zone selection update.
    if (isCollapsedSurface && wasInSelection && !zoneLocked()) {
      const selectedZones = zonesStore.zones.filter((zone) =>
        currentSelection.has(zone.id),
      );
      if (
        selectedZones.length >= 2 &&
        !selectedZones.some((zone) => zone.locked)
      ) {
        e.preventDefault();
        clearTimers();
        // v8 round-5 (Bug A defensive fix): kill the click-defer timer too;
        // a queued expand firing during/after the group drag would race the
        // drop and re-inflate the anchor zone at the cursor release point.
        clearPendingExpand();

        const releaseDrag = acquireDragLock();
        const startX = e.clientX;
        const startY = e.clientY;
        let moved = false;

        beginGroupZoneDrag(
          selectedZones.map((zone) => ({
            id: zone.id,
            position: zone.position,
          })),
        );
        setIsDragRepositioning(true);
        // v6 fix #3: global drag flag for cross-zone hover/click guards.
        setIsDragging(true);

        // v8 round-4 real-fix (Bug 1): center-on-cursor for the dragged
        // anchor zone. Delta is computed so the dragged zone's capsule
        // center matches the cursor, then `updateGroupZoneDrag` shifts
        // every other selected member by the same delta — preserving the
        // existing group-drag preview semantics while killing the
        // "capsule lands wherever I clicked" feel.
        const capPx = capsulePixels();
        const onMouseMove = (ev: MouseEvent) => {
          const dx = ev.clientX - startX;
          const dy = ev.clientY - startY;
          if (!moved && Math.hypot(dx, dy) < ZONE_DRAG_THRESHOLD_PX) return;
          moved = true;
          const desiredX =
            ((ev.clientX - capPx.width / 2) / window.innerWidth) * 100;
          const desiredY =
            ((ev.clientY - capPx.height / 2) / window.innerHeight) * 100;
          updateGroupZoneDrag({
            x_percent: desiredX - props.zone.position.x_percent,
            y_percent: desiredY - props.zone.position.y_percent,
          });
        };

        const onMouseUp = async () => {
          document.removeEventListener("mousemove", onMouseMove);
          document.removeEventListener("mouseup", onMouseUp);
          try {
            const finalPreview = endGroupZoneDrag();
            if (!moved) return;
            const updates = Object.entries(finalPreview).map(
              ([id, position]) => ({ id, position }),
            );
            if (updates.length > 0) {
              await bulkUpdateZones(updates);
              await loadZones();
            }
          } finally {
            // Fix-FE-2: bundle anchor reset into the same batch as the
            // dragging flag so the browser never sees the dragging class
            // removed before the anchor side flips.
            clearSnapshotReleaseTimer();
            // v8 round-5 (Bug A defensive fix): drop any hover-defer
            // timers so a delayed expand can't fire AFTER the drop and
            // re-inflate from the cursor position.
            clearTimers();
            clearPendingExpand();
            batch(() => {
              setIsDragRepositioning(false);
              setAnchorSnapshot(null);
            });
            // v6 fix #3: clear the global flag last so any click event
            // synthesized by mouseup is still suppressed by the guard
            // in handleZoneClick.
            setIsDragging(false);
            releaseDrag();
          }
        };

        document.addEventListener("mousemove", onMouseMove);
        document.addEventListener("mouseup", onMouseUp);
        return;
      }
    }

    // v5 Fix #4 (Q-click=A): single mousedown no longer joins the
    // selection — selection is now strictly a dblclick gesture
    // (handleZoneDblClick). Leaving this intentionally empty so the
    // browser still propagates the click event to handleZoneClick which
    // schedules the single-click expand.
  };

  /**
   * v8 round-3 #3: find a stack-merge target for a dropped capsule.
   * Returns the full list of zone ids that should form the resulting
   * stack (including the dragged zone), or null when there is no
   * overlap (zone stays free-standing).
   *
   * Rules:
   *   - Compute every other zone's expected capsule rect from its
   *     stored x/y_percent + capsule shape/size (no DOM read; the
   *     dragged zone may have unmounted and remounted by now).
   *   - If the dropped capsule's center lies inside any other zone's
   *     rect, treat that zone as the merge target.
   *   - If the target is already a stack member, pull every member of
   *     that stack into the new group so the dropped zone joins.
   *   - If both source and target already share the same stack_id,
   *     return null (no-op — they're already grouped).
   */
  const findOverlapStackTarget = (
    finalPos: { x_percent: number; y_percent: number },
    selfId: string,
    selfShape: string | null | undefined,
    selfSize: string | null | undefined,
  ): { zoneIds: string[] } | null => {
    const vp = getViewportSize();
    const selfBox = getCapsuleBoxPx(selfShape, selfSize);
    const selfLeft = (finalPos.x_percent / 100) * vp.width;
    const selfTop = (finalPos.y_percent / 100) * vp.height;
    const selfRight = selfLeft + selfBox.width;
    const selfBottom = selfTop + selfBox.height;
    const selfCenterX = selfLeft + selfBox.width / 2;
    const selfCenterY = selfTop + selfBox.height / 2;

    const selfStackId = props.zone.stack_id ?? null;

    // v8 round-4 #2: replace strict center-in-rect with permissive
    // proximity. The previous test required the dragged capsule's center
    // to land INSIDE another zone's rect — for two ~220×52 capsules side
    // by side, the user would need to drop within a 220-px-wide sweet
    // spot, which felt brittle. Two kinder triggers, EITHER fires:
    //   (a) AABB area overlap ≥ 30 % of the smaller capsule
    //   (b) center-to-center distance ≤ (rSelf + rOther) × 0.8, where
    //       r* is the average half-extent — kicks in for adjacent
    //       capsules that "kiss" but don't overlap pixel-for-pixel.
    // Among multiple candidates we pick the one with the highest score
    // (overlap ratio first, then closeness) so dropping in the middle
    // of three adjacent zones merges with the actual closest, not the
    // first one in iteration order.
    let best: { zoneIds: string[]; score: number } | null = null;
    const PROXIMITY_FACTOR = 0.8;
    const OVERLAP_THRESHOLD = 0.3;

    for (const other of zonesStore.zones) {
      if (other.id === selfId) continue;
      if (selfStackId && other.stack_id === selfStackId) continue;

      const otherBox = getCapsuleBoxPx(
        other.capsule_shape,
        other.capsule_size,
      );
      const oLeft = (other.position.x_percent / 100) * vp.width;
      const oTop = (other.position.y_percent / 100) * vp.height;
      const oRight = oLeft + otherBox.width;
      const oBottom = oTop + otherBox.height;
      const oCenterX = oLeft + otherBox.width / 2;
      const oCenterY = oTop + otherBox.height / 2;

      const interW = Math.max(0, Math.min(selfRight, oRight) - Math.max(selfLeft, oLeft));
      const interH = Math.max(0, Math.min(selfBottom, oBottom) - Math.max(selfTop, oTop));
      const interArea = interW * interH;
      const selfArea = selfBox.width * selfBox.height;
      const otherArea = otherBox.width * otherBox.height;
      const minArea = Math.max(1, Math.min(selfArea, otherArea));
      const overlapRatio = interArea / minArea;

      const dx = selfCenterX - oCenterX;
      const dy = selfCenterY - oCenterY;
      const dist = Math.hypot(dx, dy);
      const rSelf = (selfBox.width + selfBox.height) / 4;
      const rOther = (otherBox.width + otherBox.height) / 4;
      const proximityRadius = (rSelf + rOther) * PROXIMITY_FACTOR;

      const triggers =
        overlapRatio >= OVERLAP_THRESHOLD || dist <= proximityRadius;
      if (!triggers) continue;

      // Score: overlap ratio dominates; falling-back proximity hits use
      // (1 - dist/radius) so a near-perfect kiss still beats a 5 % overlap.
      const score =
        overlapRatio > 0
          ? overlapRatio + 1
          : Math.max(0, 1 - dist / proximityRadius);

      const otherStackId = zoneStackId().get(other.id);
      let zoneIds: string[];
      if (otherStackId) {
        const members = stackMap().get(otherStackId) ?? [];
        const ids = members.map((z) => z.id);
        if (!ids.includes(selfId)) ids.push(selfId);
        if (ids.length < 2) continue;
        zoneIds = ids;
      } else {
        zoneIds = [other.id, selfId];
      }

      if (best === null || score > best.score) {
        best = { zoneIds, score };
      }
    }

    return best ? { zoneIds: best.zoneIds } : null;
  };

  // Zone repositioning via drag on header
  const handleHeaderDragStart = (e: MouseEvent) => {
    if (zoneLocked()) return;
    e.preventDefault();
    clearTimers();
    // v8 round-5 (Bug A defensive fix): also kill any pending click-defer
    // expand timer. Without this, a "click → quickly start drag" gesture
    // leaves a 120ms timer armed; when it fires AFTER drop, triggerExpand
    // runs with a fresh anchor at the dropped position, then the panel
    // re-inflates from the cursor release point — which the user
    // perceives as a flash to the drag end position.
    clearPendingExpand();
    const rect = (e.currentTarget as HTMLElement)
      .closest(".bento-zone")
      ?.getBoundingClientRect();
    if (!rect) return;

    // v8 round-4 real-fix (Bug 1): when the user grabs the header, the model
    // is "drag = collapse to zen, capsule centers on cursor, settles where
    // released, no auto-reexpand". Collapsing immediately eliminates the
    // displayExpanded flip-back that was driving the coordinate-system jump
    // (left:%↔right:px) at drop. Clearing the anchor snapshot frees the
    // panel from any stale right/bottom anchor that captureAnchorSnapshot
    // installed when expanding, so the zen capsule freely follows pos.x%/y%.
    collapseZone(props.zone.id);
    clearSnapshotReleaseTimer();
    setAnchorSnapshot(null);

    const currentSelection = selectedZoneIds();
    const shouldGroupDrag =
      !isStackMember() &&
      currentSelection.has(props.zone.id) &&
      currentSelection.size > 1;

    if (shouldGroupDrag) {
      const selectedZones = zonesStore.zones.filter((zone) =>
        currentSelection.has(zone.id),
      );
      if (
        selectedZones.length < 2 ||
        selectedZones.some((zone) => zone.locked)
      ) {
        return;
      }

      const releaseDrag = acquireDragLock();
      const startX = e.clientX;
      const startY = e.clientY;
      let moved = false;

      beginGroupZoneDrag(
        selectedZones.map((zone) => ({
          id: zone.id,
          position: zone.position,
        })),
      );
      setIsDragRepositioning(true);
      // v6 fix #3: global drag flag for cross-zone hover/click guards.
      setIsDragging(true);

      // v8 round-4 #1: rAF-coalesce mousemove. Native mousemove can
      // fire 120+ Hz on hi-fps mice; without this the per-event signal
      // write cascades through every selected zone's position style on
      // every event, causing visible stutter on the dragged group.
      // v8 round-4 real-fix (Bug 1): same center-on-cursor delta math as
      // the capsule-driven group drag — keeps the dragged anchor zone's
      // capsule centered on the cursor while every other selected
      // member shifts by the same delta.
      const headerCapPx = capsulePixels();
      let lastMoveEvent: MouseEvent | null = null;
      let moveRafId: number | null = null;
      const flushMove = () => {
        moveRafId = null;
        const ev = lastMoveEvent;
        if (!ev) return;
        const dx = ev.clientX - startX;
        const dy = ev.clientY - startY;
        if (!moved && Math.hypot(dx, dy) < ZONE_DRAG_THRESHOLD_PX) return;
        moved = true;
        const desiredX =
          ((ev.clientX - headerCapPx.width / 2) / window.innerWidth) * 100;
        const desiredY =
          ((ev.clientY - headerCapPx.height / 2) / window.innerHeight) * 100;
        updateGroupZoneDrag({
          x_percent: desiredX - props.zone.position.x_percent,
          y_percent: desiredY - props.zone.position.y_percent,
        });
      };
      const onMouseMove = (ev: MouseEvent) => {
        lastMoveEvent = ev;
        if (moveRafId !== null) return;
        moveRafId = requestAnimationFrame(flushMove);
      };

      const onMouseUp = async () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        if (moveRafId !== null) {
          cancelAnimationFrame(moveRafId);
          moveRafId = null;
        }
        // v8 round-4 hotfix (Bug 1): flush the latest mousemove event
        // SYNCHRONOUSLY before reading the persisted preview position.
        // cancelAnimationFrame above discards the still-pending tick, so
        // without this the final cursor position never reaches the signal
        // and the persisted positions are one frame stale — the dropped
        // group visibly oscillates back to the previous frame's location
        // on the next render. flushMove is idempotent (it just reads
        // lastMoveEvent and writes the preview signal).
        flushMove();
        try {
          const finalPreview = endGroupZoneDrag();
          if (!moved) return;
          const updates = Object.entries(finalPreview).map(([id, position]) => ({
            id,
            position,
          }));
          if (updates.length > 0) {
            await bulkUpdateZones(updates);
            await loadZones();
          }
        } finally {
          // v8 round-4 real-fix (Bug 1): same model — stay zen on drop.
          // Group drag also collapsed every member at start (each BentoZone
          // hits this same path or already lives as a stack member); the
          // drop must NOT re-expand any of them. A fresh hover/click
          // re-captures anchor through triggerExpand.
          clearSnapshotReleaseTimer();
          // v8 round-5 (Bug A defensive fix): kill any stale hover-defer
          // timers that survived the drag (clearTimers + clearPendingExpand
          // are both idempotent, so calling them in finally is safe even
          // when the start path already cleared them).
          clearTimers();
          clearPendingExpand();
          batch(() => {
            setIsDragRepositioning(false);
            setAnchorSnapshot(null);
          });
          // v6 fix #3: see capsule-drag finally for rationale.
          setIsDragging(false);
          releaseDrag();
        }
      };

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
      return;
    }

    // v8 round-4 real-fix (Bug 1): center the zen capsule on the cursor
    // throughout the drag. Previously offset was the click point inside
    // the panel rect — when the panel collapsed to zen at drop, the
    // capsule landed wherever the user happened to click inside the
    // panel, NOT at the cursor. The user's stated mental model is
    // "press top edge → capsule shrinks and centers on cursor → drag →
    // settles where released". Using the zen capsule's half-extent as
    // the offset achieves this for both the initial drag and the drop
    // position.
    const capPx = capsulePixels();
    const offsetX = capPx.width / 2;
    const offsetY = capPx.height / 2;

    // Acquire drag lock — prevents the poller from toggling passthrough
    // while the zone element is moving under the cursor
    const releaseDrag = acquireDragLock();

    setIsDragRepositioning(true);
    // v6 fix #3: global drag flag for cross-zone hover/click guards.
    setIsDragging(true);
    setDragOffset({ x: offsetX, y: offsetY });

    // Initialize local drag position so the capsule centers on the cursor
    // RIGHT NOW (not the previous zone position). This kills the brief
    // visible "snap to old position" frame between collapseZone() above
    // and the first mousemove.
    {
      const initialX =
        ((e.clientX - offsetX) / window.innerWidth) * 100;
      const initialY =
        ((e.clientY - offsetY) / window.innerHeight) * 100;
      const maxXPctInit = Math.max(
        0,
        100 - (capPx.width / window.innerWidth) * 100,
      );
      const maxYPctInit = Math.max(
        0,
        100 - (capPx.height / window.innerHeight) * 100,
      );
      setDragPosition({
        x_percent: Math.max(0, Math.min(maxXPctInit, initialX)),
        y_percent: Math.max(0, Math.min(maxYPctInit, initialY)),
      });
    }

    // v8 round-4 #1: rAF-coalesce mousemove. See group-drag handler above
    // for rationale. Solid signal writes from every native event were
    // cascading through `dragPosition` → zoneStyle memo → DOM, causing
    // visible jank under high-Hz mice.
    let lastMoveEvent: MouseEvent | null = null;
    let moveRafId: number | null = null;
    const flushMove = () => {
      moveRafId = null;
      const ev = lastMoveEvent;
      if (!ev) return;
      if (!isDragRepositioning()) return;

      const xPercent =
        ((ev.clientX - offsetX) / window.innerWidth) * 100;
      const yPercent =
        ((ev.clientY - offsetY) / window.innerHeight) * 100;

      // Clamp to viewport — max is 100% minus the *zen capsule* dimension
      // so the dropped capsule stays on-screen. v8 round-3 #2 fix: the
      // previous version used `rect.width/height` which, when the zone was
      // dragged from an expanded panel, was the panel rect (360x420).
      // Using the panel size shrunk the allowed x_percent range to ~81%,
      // so any zone whose stored x_percent was higher (e.g. 85% near the
      // right edge) got snapped left to ~81% on the very first mousemove.
      // That is the "flash to left after collapse from right edge" bug.
      const capPx = capsulePixels();
      const maxXPct = Math.max(0, 100 - (capPx.width / window.innerWidth) * 100);
      const maxYPct = Math.max(0, 100 - (capPx.height / window.innerHeight) * 100);
      const clampedX = Math.max(0, Math.min(maxXPct, xPercent));
      const clampedY = Math.max(0, Math.min(maxYPct, yPercent));

      // Update local signal only — no IPC call
      setDragPosition({ x_percent: clampedX, y_percent: clampedY });
    };
    const onMouseMove = (ev: MouseEvent) => {
      lastMoveEvent = ev;
      if (moveRafId !== null) return;
      moveRafId = requestAnimationFrame(flushMove);
    };

    const onMouseUp = async () => {
      // Remove listeners first to prevent duplicate triggers
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      if (moveRafId !== null) {
        cancelAnimationFrame(moveRafId);
        moveRafId = null;
      }
      // v8 round-4 hotfix (Bug 1): flush the latest mousemove event
      // SYNCHRONOUSLY before reading dragPosition. The cancelAnimationFrame
      // above discards the pending tick, so without this the last
      // mousemove never makes it into `dragPosition` and we persist the
      // *previous frame's* coordinates. The subsequent re-render snaps
      // the capsule from the cursor's actual release point back to that
      // stale frame, producing the visible left/right oscillation the
      // user reported. flushMove is idempotent: re-running it is safe
      // because lastMoveEvent is the most recent native event.
      flushMove();

      // v8 round-3 #2: persist + auto-stack inside try, ALWAYS run the
      // cleanup in finally. Previously a thrown IPC (e.g. backend errors
      // mid-await) could leave `isDragging`/`isDragRepositioning` stuck
      // true forever, blocking every subsequent stack click/hover via
      // the `if (isDragging()) return` guards in StackWrapper. The
      // resulting "stack won't open" symptom was the user-visible
      // consequence of that latent state leak.
      let didMutate = false;
      const finalPos = dragPosition();
      // v8 round-5 (Bug A diagnostic): gated logger so the user can flip
      // window.__bento_debug_drag = true in DevTools and see the exact
      // divergence between the frontend-clamped finalPos and the
      // post-IPC props.zone.position. Hot path is one property read.
      const debugDrag = (label: string, extra: Record<string, unknown> = {}): void => {
        if (typeof window === "undefined") return;
        const flag = (window as unknown as { __bento_debug_drag?: boolean })
          .__bento_debug_drag;
        if (!flag) return;
        // eslint-disable-next-line no-console
        console.log(`[drag-end:${props.zone.id}] ${label}`, {
          finalPos,
          dragPositionNow: dragPosition(),
          propsPosNow: props.zone.position,
          expanded: expanded(),
          isDragRepositioning: isDragRepositioning(),
          anchorSnapshot: anchorSnapshot(),
          ...extra,
        });
      };
      debugDrag("before-await");
      try {
        if (finalPos) {
          await updateZone(props.zone.id, {
            position: {
              x_percent: finalPos.x_percent,
              y_percent: finalPos.y_percent,
            },
          });
        }
        debugDrag("after-await");

        // v8 round-3 #3: drop-on-zone forms a stack. The user expects
        // "drag one zone onto another → they pile up", which is the
        // prerequisite for the StackWrapper bloom flow.
        if (finalPos) {
          const target = findOverlapStackTarget(
            finalPos,
            props.zone.id,
            props.zone.capsule_shape,
            props.zone.capsule_size,
          );
          if (target) {
            await stackZonesAction(target.zoneIds);
            didMutate = true;
          }
        }
      } finally {
        // v8 round-4 real-fix (Bug 1): NO preAnchor / re-expand on drop.
        // The drag model is "drag = collapse to zen, settle where released,
        // stay zen". Re-expanding here was the source of the
        // coordinate-system flip flicker (`left: x%` during drag → `right:
        // Npx` after drop). The next hover/click runs `triggerExpand` which
        // captures a fresh snapshot for the new position; until then the
        // capsule renders cleanly via `pos.x_percent`/`pos.y_percent`.
        // `didMutate` (used by stack auto-merge) is intentionally untouched
        // because stack merging unmounts this zone anyway.
        void didMutate;
        clearSnapshotReleaseTimer();
        // v8 round-5 (Bug A defensive fix): also kill any pending click
        // defer + hover expand/collapse timers that may have been armed
        // before the drag started. A stale timer firing AFTER finalize
        // would re-trigger expandZone at the dropped position, which is
        // the visual "flash to drag position" the user reports. clearTimers
        // is idempotent and cheap.
        clearTimers();
        clearPendingExpand();
        batch(() => {
          setIsDragRepositioning(false);
          setDragPosition(null);
          setAnchorSnapshot(null);
        });
        setIsDragging(false);
        releaseDrag();
        debugDrag("finally-done");
      }
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  };

  // ─── Resize handle drag ──────────────────────────────────────
  type ResizeAxis = "se" | "e" | "s";

  const handleResizeStart = (axis: ResizeAxis, e: MouseEvent) => {
    if (zoneLocked()) return;
    e.preventDefault();
    e.stopPropagation();
    clearTimers();

    const rect = zoneRef?.getBoundingClientRect();
    if (!rect) return;

    // Acquire drag lock — prevents poller from toggling passthrough
    const releaseDrag = acquireDragLock();

    setIsResizing(true);

    // Initialise local resize signal from current zone config (or fallback defaults)
    const currentW = props.zone.expanded_size.w_percent > 0
      ? props.zone.expanded_size.w_percent
      : (360 / window.innerWidth) * 100;
    const currentH = props.zone.expanded_size.h_percent > 0
      ? props.zone.expanded_size.h_percent
      : (420 / window.innerHeight) * 100;
    setResizeSize({ w_percent: currentW, h_percent: currentH });

    // Remember start mouse position and start size for delta calculation
    const startX = e.clientX;
    const startY = e.clientY;
    const startWPercent = currentW;
    const startHPercent = currentH;

    // Minimum / maximum constraints (match CSS min-width/max-width)
    const minW = (280 / window.innerWidth) * 100;
    const maxW = (600 / window.innerWidth) * 100;
    const minH = (200 / window.innerHeight) * 100;
    const maxH = (600 / window.innerHeight) * 100;

    // R1: when anchored right/bottom the grow direction is mirrored —
    // moving the handle left/up grows the panel. Invert delta signs to
    // preserve the "drag handle outward grows the panel" invariant.
    const anchorAtStart = currentAnchor();
    const xSign = anchorAtStart.x === "right" ? -1 : 1;
    const ySign = anchorAtStart.y === "bottom" ? -1 : 1;

    const onMouseMove = (ev: MouseEvent) => {
      const deltaXPercent = xSign * ((ev.clientX - startX) / window.innerWidth) * 100;
      const deltaYPercent = ySign * ((ev.clientY - startY) / window.innerHeight) * 100;

      let newW = startWPercent;
      let newH = startHPercent;

      if (axis === "se" || axis === "e") {
        newW = Math.max(minW, Math.min(maxW, startWPercent + deltaXPercent));
      }
      if (axis === "se" || axis === "s") {
        newH = Math.max(minH, Math.min(maxH, startHPercent + deltaYPercent));
      }

      setResizeSize({ w_percent: newW, h_percent: newH });
    };

    const onMouseUp = async () => {
      // Remove listeners first to prevent duplicate triggers
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);

      // Persist final size via IPC — MUST await so that the store
      // updates props.zone.expanded_size BEFORE we clear the local signal.
      // Same flicker/jump fix as position drag.
      const finalSize = resizeSize();
      if (finalSize) {
        await updateZone(props.zone.id, {
          expanded_size: { w_percent: finalSize.w_percent, h_percent: finalSize.h_percent },
        });
      }

      // Store is now synced — safe to clear local resize state
      batch(() => {
        setIsResizing(false);
        setResizeSize(null);
      });

      // Release drag lock last — zone dimensions are stable
      releaseDrag();
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  };

  onCleanup(() => {
    clearTimers();
    clearSnapshotReleaseTimer();
    clearPendingExpand();
    if (zoneRef) {
      unregisterZoneElement(zoneRef);
      unregisterDropZoneElement(zoneRef);
    }
  });

  // Expanded panel size: during resize use local signal for instant feedback;
  // otherwise fall back to persisted zone config.
  const expandedWidth = () => {
    const rs = resizeSize();
    const w = rs ? rs.w_percent : props.zone.expanded_size.w_percent;
    return w > 0 ? `${(w / 100) * window.innerWidth}px` : "360px";
  };
  const expandedHeight = () => {
    const rs = resizeSize();
    const h = rs ? rs.h_percent : props.zone.expanded_size.h_percent;
    return h > 0 ? `${(h / 100) * window.innerHeight}px` : "420px";
  };

  // Capsule (zen) dimensions based on shape and size
  const zenDimensions = () => {
    const dims = capsulePixels();
    return { w: `${dims.width}px`, h: `${dims.height}px` };
  };

  /**
   * Current anchor in use. When the snapshot exists we use it for BOTH the
   * expand AND collapse spring transitions — the snapshot is only released
   * after the collapse animation completes (see `scheduleSnapshotRelease`).
   *
   * #5 fix: previous incarnation reset to top-left immediately on collapse,
   * which yanked transform-origin away from the anchor corner mid-animation
   * and made the capsule appear to "flash" to the bottom-right corner.
   */
  const currentAnchor = createMemo(() => {
    const snap = anchorSnapshot();
    if (snap) return snap;
    return { x: "left" as const, y: "top" as const, flipOffsetX: 0, flipOffsetY: 0 };
  });

  // v8 round-3 #1: drag always renders the idle zen capsule, regardless of
  // the active display mode (hover / click / always) and regardless of
  // whether the zone is currently `expanded()`. The user pulls a zone by
  // its body — that body must visually collapse to the small capsule the
  // moment a drag begins, then re-inflate when dropped. This is purely a
  // *visual* override; `expanded()` (the logical state) is left intact so
  // the panel pops back open after drop without an extra transition.
  const displayExpanded = createMemo(
    () => expanded() && !isDragRepositioning(),
  );

  // Compute inline position + animated dimensions
  const zoneStyle = () => {
    const isExp = displayExpanded();
    const accent = props.zone.accent_color;
    // During drag, use local signal for instant visual feedback; otherwise use store
    const pos =
      dragPosition() ??
      getGroupDragPreviewPosition(props.zone.id) ??
      props.zone.position;
    const zen = zenDimensions();
    // Fix-FE-1 z-index lift: order = expanded > dragging > hovered > idle.
    // expanded() is the user's deliberate focus and must outrank everything;
    // dragging next so the moved zone never disappears under a neighbour;
    // hovered next so a back-zone surfaces just by mousing over it; idle
    // zones use sort_order as the deterministic base.
    //
    // v9: the ladder constants now live in `styles/zStack.ts` and are
    // shared with StackWrapper.tsx so a stack and a free zone with the
    // same `sort_order` resolve to the same z-index at rest. Pre-v9
    // StackWrapper used `sort_order + 30` while BentoZone used
    // `sort_order + 10`, which let stacks unconditionally outrank free
    // zones at rest without semantic justification.
    const baseSort = props.zone.sort_order + Z_ZONE_IDLE_OFFSET;
    // z-index: dragging wins absolutely so the moving zen capsule never
    // disappears under a panel left expanded (e.g. always-mode neighbour).
    const zIndex = isDragRepositioning()
      ? Z_ZONE_DRAG
      : isExp
        ? Z_ZONE_EXPANDED
        : isHovering()
          ? Z_ZONE_HOVER
          : baseSort;
    const base: Record<string, string> = {
      position: "absolute",
      "pointer-events": "auto",
      "z-index": String(zIndex),
      // Dimensions driven by state — CSS transition animates the change
      width: isExp ? expandedWidth() : zen.w,
      height: isExp ? expandedHeight() : zen.h,
    };
    // R1 + #5 fix: keep the coordinate system stable across the expand /
    // collapse spring by emitting `right:` / `bottom:` whenever a snapshot
    // is anchored to those edges — in BOTH expanded and zen states.
    // Previously the style flipped between `right: Npx` (expanded) and
    // `left: X%` (zen) at the moment `expanded()` went false, and the
    // browser cannot interpolate between two different anchor sides, so
    // the rendered position jumped — the visible "flash to bottom-right
    // corner" the user reported.
    //
    // See computeZonePositionStyle for the decision logic + the drag
    // carve-out (live drag must honor pos.x_percent).
    // Fix-FE-2: drop the previous `!isExp` guard so an expanded panel
    // dragged from a screen edge also bypasses the snapshot anchor side
    // and follows the live cursor coordinates. Without the carve-out the
    // panel froze at its expand-time `right:`/`bottom:` offset during the
    // drag and snapped on release.
    const isDraggingPanel =
      dragPosition() !== null ||
      getGroupDragPreviewPosition(props.zone.id) !== null;
    const positionStyle = computeZonePositionStyle({
      snapshot: anchorSnapshot(),
      pos,
      isDraggingZen: isDraggingPanel,
    });
    if (positionStyle.left !== undefined) base.left = positionStyle.left;
    if (positionStyle.right !== undefined) base.right = positionStyle.right;
    if (positionStyle.top !== undefined) base.top = positionStyle.top;
    if (positionStyle.bottom !== undefined) base.bottom = positionStyle.bottom;
    // Inject zone accent as CSS custom property for child consumption
    if (accent) {
      base["--zone-accent"] = accent;
    }
    // D1 + #5 fix: bind transform-origin to the anchor direction so the
    // spring-expand animation grows from (and retracts back into) the
    // snapshot anchor corner. The snapshot is held through the collapse
    // transition (see scheduleSnapshotRelease) so this stays pinned.
    const origin = computeTransformOrigin(anchorSnapshot());
    base["--origin-x"] = origin.x;
    base["--origin-y"] = origin.y;
    return base;
  };

  const zoneClasses = () => {
    const base = "bento-zone spring-expand";
    // v8 round-3 #1: zen visual while dragging — see displayExpanded memo.
    const isDispExp = displayExpanded();
    const state = isDispExp ? "bento-zone--expanded" : "bento-zone--zen";
    const drop = isDropTarget() ? "bento-zone--drop-target" : "";
    const drag = isDragRepositioning() ? "bento-zone--dragging" : "";
    const resize = isResizing() ? "bento-zone--resizing" : "";
    const dragHover = isCrossDragHover() ? "bento-zone--drag-hover" : "";
    const selected = zoneSelected() ? "bento-zone--selected" : "";
    const locked = zoneLocked() ? "bento-zone--locked" : "";
    // Apply capsule shape to the OUTER container so border-radius works with overflow:hidden
    const shape = !isDispExp ? `bento-zone--shape-${props.zone.capsule_shape || "pill"}` : "";
    const anchor = currentAnchor();
    const anchorX = isDispExp && anchor.x === "right" ? "bento-zone--anchor-right" : "";
    const anchorY = isDispExp && anchor.y === "bottom" ? "bento-zone--anchor-bottom" : "";
    return `${base} ${state} ${drop} ${drag} ${resize} ${dragHover} ${selected} ${locked} ${shape} ${anchorX} ${anchorY}`;
  };

  return (
    <div
      ref={zoneRef}
      class={zoneClasses()}
      style={zoneStyle()}
      onMouseEnter={handleMouseEnter}
      onMouseMove={handleMouseMove}
      onMouseLeave={handleMouseLeave}
      onMouseDown={handleZoneMouseDown}
      onClick={handleZoneClick}
      onDblClick={handleZoneDblClick}
      onDragEnter={dropHandlers.onDragEnter}
      onDragOver={dropHandlers.onDragOver}
      onDragLeave={dropHandlers.onDragLeave}
      onDrop={dropHandlers.onDrop}
      data-zone-id={props.zone.id}
    >
      {/* Zen layer: visible when collapsed, fades out when expanded */}
      <div class={`bento-zone__zen-layer ${displayExpanded() ? "bento-zone__zen-layer--hidden" : ""}`}>
        <ZenCapsule zone={props.zone} />
      </div>
      {/* Bento layer: visible when expanded, fades in after container expands */}
      <div class={`bento-zone__bento-layer ${displayExpanded() ? "bento-zone__bento-layer--visible" : ""}`}>
        <BentoPanel
          zone={props.zone}
          onHeaderDragStart={handleHeaderDragStart}
        />
      </div>
      {/* Resize handles: only interactive when expanded */}
      <Show when={displayExpanded()}>
        <div
          class="bento-zone__resize-handle bento-zone__resize-handle--e"
          onMouseDown={(e) => handleResizeStart("e", e)}
        />
        <div
          class="bento-zone__resize-handle bento-zone__resize-handle--s"
          onMouseDown={(e) => handleResizeStart("s", e)}
        />
        <div
          class="bento-zone__resize-handle bento-zone__resize-handle--se"
          onMouseDown={(e) => handleResizeStart("se", e)}
        />
      </Show>
    </div>
  );
};

export default BentoZone;
