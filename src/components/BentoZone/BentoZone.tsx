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
import { Component, Show, batch, createEffect, createMemo, createSignal, onMount, onCleanup } from "solid-js";
import type { BentoZone as BentoZoneType } from "../../types/zone";
import { isZoneExpanded, expandZone, collapseZone, getViewportSize } from "../../stores/ui";
import { getExpandDelay, getCollapseDelay, getZoneDisplayMode } from "../../stores/settings";
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
import { suggestStack } from "../../services/stack";
import { stackZonesAction } from "../../stores/stacks";
import {
  computeTransformOrigin,
  computeZonePositionStyle,
  SPRING_TRANSITION_MS,
} from "../../services/anchorOrigin";
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
  const zoneDisplayMode = () => props.zone.display_mode ?? getZoneDisplayMode();
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

    // Default to viewport-based overflow test — works for primary-only.
    let workLeft = 0;
    let workTop = 0;
    let workRight = vp.width;
    let workBottom = vp.height;

    // Multi-monitor: prefer the capsule's monitor work-area, translated
    // into viewport (logical) coordinates via devicePixelRatio.
    const cached = cachedMonitors();
    if (cached && cached.length > 1) {
      const mon = monitorForClientRect(rect);
      if (mon) {
        // D1.3 fix: use per-monitor dpi_scale when available so secondary
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

    const wouldOverflowX = rect.left + effPanelW + MARGIN > workRight;
    const wouldOverflowY = rect.top + effPanelH + MARGIN > workBottom;
    // Also avoid flipping if flipping would itself push the LEFT edge
    // off the work-area — in that case the capsule is simply too wide
    // and the CSS max-width clamp handles it.
    const flipStillFitsX = rect.right - effPanelW - MARGIN >= workLeft;
    const flipStillFitsY = rect.bottom - effPanelH - MARGIN >= workTop;
    // D1.1 fix: flip only if space on the natural side cannot fit the panel
    // AND the flipped side can. The old "workBottom - rect.bottom < panelH + MARGIN"
    // was too eager — it flipped whenever the capsule was merely close to the edge,
    // even if the natural side had enough room, which made panels grow upward
    // from any y_percent > ~60%.
    const spaceBelow = workBottom - rect.bottom;
    const spaceAbove = rect.bottom - workTop;
    const spaceRight = workRight - rect.right;
    const spaceLeft = rect.right - workLeft;
    const nearBottomEdge =
      spaceBelow < effPanelH + MARGIN &&
      spaceAbove >= effPanelH + MARGIN &&
      flipStillFitsY;
    const nearRightEdge =
      spaceRight < effPanelW + MARGIN &&
      spaceLeft >= effPanelW + MARGIN &&
      flipStillFitsX;

    const anchorX: "left" | "right" =
      (wouldOverflowX || nearRightEdge) && flipStillFitsX ? "right" : "left";
    const anchorY: "top" | "bottom" =
      (wouldOverflowY || nearBottomEdge) && flipStillFitsY ? "bottom" : "top";

    // When anchored right/bottom, `right:` / `bottom:` is measured from
    // the viewport edge, which equals (viewport - rect.right/bottom).
    const flipOffsetX = Math.max(0, vp.width - rect.right);
    const flipOffsetY = Math.max(0, vp.height - rect.bottom);

    setAnchorSnapshot({
      x: anchorX,
      y: anchorY,
      flipOffsetX,
      flipOffsetY,
    });
  };

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
    hitTestHandlers.onPointerEnter();
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

  // v1.2.1 — `click` mode: a single left-click on the collapsed capsule
  // expands the zone. Once expanded, mouse-leave still auto-collapses, so
  // the interaction feels like a launcher pop-over rather than a mode toggle.
  const handleZoneClick = (e: MouseEvent) => {
    if (isStackMember()) return;
    if (zoneDisplayMode() !== "click") return;
    if (expanded()) return;
    if (e.button !== 0) return;
    // Fire only when the user clicked the zen (capsule) surface; clicks on
    // resize handles or header-drag are already gated by !expanded(), but an
    // explicit target check prevents accidentally swallowing a click on any
    // overlaid tooltip portal that happens to sit above the capsule.
    const target = e.target as HTMLElement | null;
    if (target && target.closest(".bento-zone__resize-handle")) return;
    triggerExpand(false);
  };

  const handleMouseLeave = () => {
    hitTestHandlers.onPointerLeave();
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

        const onMouseMove = (ev: MouseEvent) => {
          const dx = ev.clientX - startX;
          const dy = ev.clientY - startY;
          if (!moved && Math.hypot(dx, dy) < ZONE_DRAG_THRESHOLD_PX) return;
          moved = true;
          updateGroupZoneDrag({
            x_percent: (dx / window.innerWidth) * 100,
            y_percent: (dy / window.innerHeight) * 100,
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
            setIsDragRepositioning(false);
            // #5 fix: group-drag moved this capsule too → invalidate the
            // anchor snapshot so the next expand recaptures against the
            // new position.
            clearSnapshotReleaseTimer();
            setAnchorSnapshot(null);
            releaseDrag();
          }
        };

        document.addEventListener("mousemove", onMouseMove);
        document.addEventListener("mouseup", onMouseUp);
        return;
      }
    }

    selectZone(props.zone.id, {
      shift: e.shiftKey,
      ctrl: e.ctrlKey || e.metaKey,
      orderedIds: zonesStore.zones.map((zone) => zone.id),
    });
  };

  // Zone repositioning via drag on header
  const handleHeaderDragStart = (e: MouseEvent) => {
    if (zoneLocked()) return;
    e.preventDefault();
    clearTimers();
    const rect = (e.currentTarget as HTMLElement)
      .closest(".bento-zone")
      ?.getBoundingClientRect();
    if (!rect) return;

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

      const onMouseMove = (ev: MouseEvent) => {
        const dx = ev.clientX - startX;
        const dy = ev.clientY - startY;
        if (!moved && Math.hypot(dx, dy) < ZONE_DRAG_THRESHOLD_PX) return;
        moved = true;
        updateGroupZoneDrag({
          x_percent: (dx / window.innerWidth) * 100,
          y_percent: (dy / window.innerHeight) * 100,
        });
      };

      const onMouseUp = async () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
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
          setIsDragRepositioning(false);
          // #5 fix: group-drag moved this capsule (header path) → invalidate
          // the anchor snapshot so the next expand recaptures.
          clearSnapshotReleaseTimer();
          setAnchorSnapshot(null);
          releaseDrag();
        }
      };

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
      return;
    }

    const offsetX = e.clientX - rect.left;
    const offsetY = e.clientY - rect.top;

    // Acquire drag lock — prevents the poller from toggling passthrough
    // while the zone element is moving under the cursor
    const releaseDrag = acquireDragLock();

    setIsDragRepositioning(true);
    setDragOffset({ x: offsetX, y: offsetY });

    // Initialize local drag position to current zone position
    setDragPosition({
      x_percent: props.zone.position.x_percent,
      y_percent: props.zone.position.y_percent,
    });

    const onMouseMove = (ev: MouseEvent) => {
      if (!isDragRepositioning()) return;

      const xPercent =
        ((ev.clientX - offsetX) / window.innerWidth) * 100;
      const yPercent =
        ((ev.clientY - offsetY) / window.innerHeight) * 100;

      // Clamp to viewport — the max is 100% minus capsule dimension so the capsule
      // never extends past the screen edge. Old hard-coded 95% was too restrictive
      // on 1080p (capsule only occupies ~4.4%), preventing users from dragging zones
      // all the way to the bottom/right edge.
      const capsuleH = rect.height || 48;
      const capsuleW = rect.width || 160;
      const maxXPct = Math.max(0, 100 - (capsuleW / window.innerWidth) * 100);
      const maxYPct = Math.max(0, 100 - (capsuleH / window.innerHeight) * 100);
      const clampedX = Math.max(0, Math.min(maxXPct, xPercent));
      const clampedY = Math.max(0, Math.min(maxYPct, yPercent));

      // Update local signal only — no IPC call
      setDragPosition({ x_percent: clampedX, y_percent: clampedY });
    };

    const onMouseUp = async () => {
      // Remove listeners first to prevent duplicate triggers
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);

      // Persist final position via IPC — MUST await so that the store
      // updates props.zone.position BEFORE we clear the local signal.
      // Without await, clearing dragPosition causes the zone to momentarily
      // snap back to the old store position (flicker/jump bug).
      const finalPos = dragPosition();
      if (finalPos) {
        await updateZone(props.zone.id, {
          position: { x_percent: finalPos.x_percent, y_percent: finalPos.y_percent },
        });
      }

      // D2: after drag settles, scan neighbours for substantial overlap.
      // If this zone joined (or created) a new cluster that's not already
      // a single stack, promote it via `stack_zones`. Done *after*
      // persistence so rects reflect the new position.
      try {
        const clusters = suggestStack(zonesStore.zones);
        for (const cluster of clusters) {
          if (!cluster.includes(props.zone.id)) continue;
          // Skip if the cluster already shares one stack_id.
          const firstSid = zonesStore.zones.find((z) => z.id === cluster[0])?.stack_id;
          const allSame = firstSid && cluster.every(
            (id) => zonesStore.zones.find((z) => z.id === id)?.stack_id === firstSid,
          );
          if (allSame) continue;
          await stackZonesAction(cluster);
          break;
        }
      } catch (err) {
        console.warn("suggestStack failed:", err);
      }

      // Store is now synced — safe to clear local drag state
      batch(() => {
        setIsDragRepositioning(false);
        setDragPosition(null);
      });

      // #5 fix: capsule moved → previous flipOffset is stale. Drop the
      // snapshot so the next expand re-captures against the new capsule
      // position; until then zoneStyle falls back to the `left: x%` path.
      clearSnapshotReleaseTimer();
      setAnchorSnapshot(null);

      // Release the drag lock last — zone position is stable
      releaseDrag();
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

  // Compute inline position + animated dimensions
  const zoneStyle = () => {
    const isExp = expanded();
    const accent = props.zone.accent_color;
    // During drag, use local signal for instant visual feedback; otherwise use store
    const pos =
      dragPosition() ??
      getGroupDragPreviewPosition(props.zone.id) ??
      props.zone.position;
    const zen = zenDimensions();
    const base: Record<string, string> = {
      position: "absolute",
      "pointer-events": "auto",
      "z-index": isExp ? "100" : String(props.zone.sort_order + 10),
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
    const isDraggingZen =
      !isExp &&
      (dragPosition() !== null ||
        getGroupDragPreviewPosition(props.zone.id) !== null);
    const positionStyle = computeZonePositionStyle({
      snapshot: anchorSnapshot(),
      pos,
      isDraggingZen,
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
    const state = expanded() ? "bento-zone--expanded" : "bento-zone--zen";
    const drop = isDropTarget() ? "bento-zone--drop-target" : "";
    const drag = isDragRepositioning() ? "bento-zone--dragging" : "";
    const resize = isResizing() ? "bento-zone--resizing" : "";
    const dragHover = isCrossDragHover() ? "bento-zone--drag-hover" : "";
    const selected = zoneSelected() ? "bento-zone--selected" : "";
    const locked = zoneLocked() ? "bento-zone--locked" : "";
    // Apply capsule shape to the OUTER container so border-radius works with overflow:hidden
    const shape = !expanded() ? `bento-zone--shape-${props.zone.capsule_shape || "pill"}` : "";
    const anchor = currentAnchor();
    const anchorX = expanded() && anchor.x === "right" ? "bento-zone--anchor-right" : "";
    const anchorY = expanded() && anchor.y === "bottom" ? "bento-zone--anchor-bottom" : "";
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
      onDragEnter={dropHandlers.onDragEnter}
      onDragOver={dropHandlers.onDragOver}
      onDragLeave={dropHandlers.onDragLeave}
      onDrop={dropHandlers.onDrop}
      data-zone-id={props.zone.id}
    >
      {/* Zen layer: visible when collapsed, fades out when expanded */}
      <div class={`bento-zone__zen-layer ${expanded() ? "bento-zone__zen-layer--hidden" : ""}`}>
        <ZenCapsule zone={props.zone} />
      </div>
      {/* Bento layer: visible when expanded, fades in after container expands */}
      <div class={`bento-zone__bento-layer ${expanded() ? "bento-zone__bento-layer--visible" : ""}`}>
        <BentoPanel
          zone={props.zone}
          onHeaderDragStart={handleHeaderDragStart}
        />
      </div>
      {/* Resize handles: only interactive when expanded */}
      <Show when={expanded()}>
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
