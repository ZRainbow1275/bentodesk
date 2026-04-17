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
import { Component, Show, batch, createMemo, createSignal, onMount, onCleanup } from "solid-js";
import type { BentoZone as BentoZoneType } from "../../types/zone";
import { isZoneExpanded, expandZone, collapseZone, getViewportSize } from "../../stores/ui";
import { getExpandDelay, getCollapseDelay } from "../../stores/settings";
import { monitorForClientRect, cachedMonitors } from "../../services/geometry";
import {
  createHitTestHandlers,
  registerZoneElement,
  unregisterZoneElement,
  acquireDragLock,
} from "../../services/hitTest";
import {
  createDropHandlers,
  activeDropZone,
  registerDropZoneElement,
  unregisterDropZoneElement,
} from "../../services/dropTarget";
import { internalDrag } from "../../services/drag";
import { preloadIcons } from "../../services/ipc";
import { updateZone } from "../../stores/zones";
import ZenCapsule from "./ZenCapsule";
import BentoPanel from "./BentoPanel";
import "./BentoZone.css";

interface BentoZoneProps {
  zone: BentoZoneType;
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

  // ─── Resize state ────────────────────────────────────────────
  const [isResizing, setIsResizing] = createSignal(false);
  // Local resize signal — updated every mousemove for instant visual feedback
  const [resizeSize, setResizeSize] = createSignal<{
    w_percent: number;
    h_percent: number;
  } | null>(null);

  const hitTestHandlers = createHitTestHandlers();
  const dropHandlers = createDropHandlers(props.zone.id);

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
      registerZoneElement(zoneRef);
      registerDropZoneElement(zoneRef, props.zone.id);
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
        const dpr = window.devicePixelRatio || 1;
        workLeft = mon.rect_work.x / dpr;
        workTop = mon.rect_work.y / dpr;
        workRight = (mon.rect_work.x + mon.rect_work.width) / dpr;
        workBottom = (mon.rect_work.y + mon.rect_work.height) / dpr;
      }
    }

    const wouldOverflowX = rect.left + panelW + MARGIN > workRight;
    const wouldOverflowY = rect.top + panelH + MARGIN > workBottom;
    // Also avoid flipping if flipping would itself push the LEFT edge
    // off the work-area — in that case the capsule is simply too wide
    // and the CSS max-width clamp handles it.
    const flipStillFitsX = rect.right - panelW - MARGIN >= workLeft;
    const flipStillFitsY = rect.bottom - panelH - MARGIN >= workTop;

    const anchorX: "left" | "right" =
      wouldOverflowX && flipStillFitsX ? "right" : "left";
    const anchorY: "top" | "bottom" =
      wouldOverflowY && flipStillFitsY ? "bottom" : "top";

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

  const handleMouseEnter = () => {
    hitTestHandlers.onPointerEnter();
    clearTimers();
    if (isHoverIntentSuspended()) return;
    if (!expanded()) {
      expandTimer = setTimeout(() => {
        // Freeze the anchor direction before flipping state → expanded
        // so `zoneStyle()` observes a stable snapshot on first paint.
        captureAnchorSnapshot();
        expandZone(props.zone.id);
        // R2: block collapse scheduling for 550ms so the spring settles
        // before we honour a transient mouseleave caused by animation.
        setExpandLockUntil(Date.now() + 550);
        // Preload icons for items in this zone so they render immediately
        const paths = props.zone.items.map((i) => i.path);
        if (paths.length > 0) {
          void preloadIcons(paths);
        }
      }, getExpandDelay());
    }
  };

  const handleMouseLeave = () => {
    hitTestHandlers.onPointerLeave();
    clearTimers();
    if (isHoverIntentSuspended()) return;
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
        // Drop the snapshot so the next expand re-evaluates against
        // current capsule position / viewport.
        setAnchorSnapshot(null);
      }, effectiveDelay);
    }
  };

  // Local drag position signal — updated on every mousemove, no IPC overhead
  const [dragPosition, setDragPosition] = createSignal<{
    x_percent: number;
    y_percent: number;
  } | null>(null);

  // Zone repositioning via drag on header
  const handleHeaderDragStart = (e: MouseEvent) => {
    e.preventDefault();
    clearTimers();
    const rect = (e.currentTarget as HTMLElement)
      .closest(".bento-zone")
      ?.getBoundingClientRect();
    if (!rect) return;

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

      // Clamp to viewport
      const clampedX = Math.max(0, Math.min(95, xPercent));
      const clampedY = Math.max(0, Math.min(95, yPercent));

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

      // Store is now synced — safe to clear local drag state
      batch(() => {
        setIsDragRepositioning(false);
        setDragPosition(null);
      });

      // Release the drag lock last — zone position is stable
      releaseDrag();
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  };

  // ─── Resize handle drag ──────────────────────────────────────
  type ResizeAxis = "se" | "e" | "s";

  const handleResizeStart = (axis: ResizeAxis, e: MouseEvent) => {
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
    const shape = props.zone.capsule_shape || "pill";
    const size = props.zone.capsule_size || "medium";

    // Size → width × height lookup (circle uses square aspect ratio)
    const sizeMap: Record<string, { w: string; h: string }> = {
      small:  { w: "120px", h: "36px" },
      medium: { w: "160px", h: "48px" },
      large:  { w: "200px", h: "56px" },
    };

    const dims = sizeMap[size] || sizeMap.medium;

    if (shape === "circle") {
      // Circle: square dimensions based on size
      const circleSize = size === "small" ? "42px" : size === "large" ? "64px" : "52px";
      return { w: circleSize, h: circleSize };
    }
    return dims;
  };

  /**
   * Current anchor in use. When expanded & snapshot exists we use the
   * frozen snapshot; while collapsed we default to top-left so the
   * capsule follows its stored position.
   */
  const currentAnchor = createMemo(() => {
    const snap = anchorSnapshot();
    if (expanded() && snap) return snap;
    return { x: "left" as const, y: "top" as const, flipOffsetX: 0, flipOffsetY: 0 };
  });

  // Compute inline position + animated dimensions
  const zoneStyle = () => {
    const isExp = expanded();
    const accent = props.zone.accent_color;
    // During drag, use local signal for instant visual feedback; otherwise use store
    const pos = dragPosition() ?? props.zone.position;
    const zen = zenDimensions();
    const anchor = currentAnchor();
    const base: Record<string, string> = {
      position: "absolute",
      "pointer-events": "auto",
      "z-index": isExp ? "100" : String(props.zone.sort_order + 10),
      // Dimensions driven by state — CSS transition animates the change
      width: isExp ? expandedWidth() : zen.w,
      height: isExp ? expandedHeight() : zen.h,
    };
    // R1: emit `right:` / `bottom:` when the anchor flipped so the
    // panel grows leftward / upward away from the overflow edge. The
    // offset is absolute pixels measured at expand-time; we avoid a
    // mid-animation jump that would occur if we mixed percentage
    // left + pixel right.
    if (isExp && anchor.x === "right") {
      base.right = `${anchor.flipOffsetX}px`;
    } else {
      base.left = `${pos.x_percent}%`;
    }
    if (isExp && anchor.y === "bottom") {
      base.bottom = `${anchor.flipOffsetY}px`;
    } else {
      base.top = `${pos.y_percent}%`;
    }
    // Inject zone accent as CSS custom property for child consumption
    if (accent) {
      base["--zone-accent"] = accent;
    }
    return base;
  };

  const zoneClasses = () => {
    const base = "bento-zone spring-expand";
    const state = expanded() ? "bento-zone--expanded" : "bento-zone--zen";
    const drop = isDropTarget() ? "bento-zone--drop-target" : "";
    const drag = isDragRepositioning() ? "bento-zone--dragging" : "";
    const resize = isResizing() ? "bento-zone--resizing" : "";
    const dragHover = isCrossDragHover() ? "bento-zone--drag-hover" : "";
    // Apply capsule shape to the OUTER container so border-radius works with overflow:hidden
    const shape = !expanded() ? `bento-zone--shape-${props.zone.capsule_shape || "pill"}` : "";
    const anchor = currentAnchor();
    const anchorX = expanded() && anchor.x === "right" ? "bento-zone--anchor-right" : "";
    const anchorY = expanded() && anchor.y === "bottom" ? "bento-zone--anchor-bottom" : "";
    return `${base} ${state} ${drop} ${drag} ${resize} ${dragHover} ${shape} ${anchorX} ${anchorY}`;
  };

  return (
    <div
      ref={zoneRef}
      class={zoneClasses()}
      style={zoneStyle()}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
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
