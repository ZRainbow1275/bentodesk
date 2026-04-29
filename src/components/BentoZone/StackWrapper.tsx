import {
  Component,
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import type { BentoZone as BentoZoneType } from "../../types/zone";
import { detachZoneFromStackAction, unstackZonesAction } from "../../stores/stacks";
import { loadZones, updateZone, zonesStore } from "../../stores/zones";
import { getViewportSize } from "../../stores/ui";
import {
  acquireDragLock,
  registerZoneElement,
  unregisterZoneElement,
  computeInflateForPosition,
  getCapsuleBoxPx,
} from "../../services/hitTest";
import { resolvePetalRow, pickPetalSize } from "../../services/petalLayout";
import {
  HOVER_INTENT_MS,
  LEAVE_GRACE_MS,
  STICKY_GRACE_MS,
} from "../../services/hoverIntent";
import { getZoneDisplayMode } from "../../stores/settings";
import { isDragging, setIsDragging } from "../../stores/dragging";
import { bulkUpdateZones } from "../../services/ipc";
import {
  beginGroupZoneDrag,
  endGroupZoneDrag,
  selectedZoneIds,
  updateGroupZoneDrag,
} from "../../stores/selection";
import { t } from "../../i18n";
import FocusedZonePreview from "./FocusedZonePreview";
import StackCapsule from "./StackCapsule";
import StackTray from "./StackTray";
import ZoneIcon from "../Icons/ZoneIcon";
import {
  Z_ZONE_IDLE_OFFSET,
  Z_ZONE_PROMOTED,
  Z_ZONE_DRAG,
} from "../../styles/zStack";
import "./StackWrapper.css";

interface StackWrapperProps {
  stackId: string;
  zones: BentoZoneType[];
}

const DRAG_THRESHOLD_PX = 4;
const PREVIEW_GAP_PX = 16;

const clampPct = (value: number, max: number): number =>
  Math.max(0, Math.min(max, value));

function getCapsulePixelSize(zone: BentoZoneType): { width: number; height: number } {
  const shape = zone.capsule_shape || "pill";
  const size = zone.capsule_size || "medium";

  if (shape === "circle") {
    const px = size === "small" ? 42 : size === "large" ? 64 : 52;
    return { width: px, height: px };
  }

  const dims: Record<string, { width: number; height: number }> = {
    small: { width: 120, height: 36 },
    medium: { width: 160, height: 48 },
    large: { width: 200, height: 56 },
  };
  return dims[size] ?? dims.medium;
}

const StackWrapper: Component<StackWrapperProps> = (props) => {
  let suppressNextClick = false;
  const [trayOpen, setTrayOpen] = createSignal(false);
  const [previewZoneId, setPreviewZoneId] = createSignal<string | null>(null);
  const [contextMenuOpen, setContextMenuOpen] = createSignal<{
    x: number;
    y: number;
  } | null>(null);
  const [dragPosition, setDragPosition] = createSignal<{
    x_percent: number;
    y_percent: number;
  } | null>(null);
  const [previewAnchor, setPreviewAnchor] = createSignal<{
    horizontal: "left" | "right";
    vertical: "top" | "bottom";
  }>({
    horizontal: "right",
    vertical: "top",
  });
  // v6 #4 hover-bloom: when hovering the stack the members briefly fan out
  // (CSS transform only — no layout.json mutation). Hovering / clicking a
  // bloom petal opens that member's FocusedZonePreview (== "expand"). Mouse
  // leaving the wrapper collapses the bloom AND closes the preview.
  const [isBloomed, setIsBloomed] = createSignal(false);
  // v8 round-12 (bloom-row-layout): the radial polar layout from rounds 4–11
  // failed visually at viewport edges — when the capsule sits near the
  // top-right corner, the collision-avoidance solver scatters petals to
  // weird positions to avoid clipping. The user feedback abandons the
  // radial bloom entirely: petals must always render as a horizontal row
  // directly below (or above, when capsule is near the bottom) the stack
  // capsule, side by side. Cursor coordinates are no longer the layout
  // anchor — the capsule rect is. Cursor capture is dropped along with
  // bloomCursor/bloomCenter.

  let wrapperRef: HTMLDivElement | undefined;
  // v8 round-5 (Bug B real-fix): the bloom buffer + every petal needs to
  // be registered with the cursor hit-test poller while the bloom is
  // active. Without this, the polar petals fly OUTSIDE the wrapper's
  // bounding rect (because they are `position: fixed`), so the only
  // hit-test target the poller sees is the original capsule rect. The
  // moment the cursor sits over a petal far from the capsule, the
  // state machine drops to PASSTHROUGH → the webview ignores every
  // petal mouseenter / click → the user sees petals fan out but cannot
  // operate on any of them. Registering the buffer halo + each petal
  // every render keeps the cursor "captured" anywhere inside the bloom.
  let bloomBufferRef: HTMLDivElement | undefined;
  const petalRefs = new Map<string, HTMLButtonElement>();
  // v8 #4: mouseleave race-protection. v7's bloom collapsed the moment the
  // cursor crossed the wrapper edge — including the brief gap when the
  // cursor moved between capsule and a petal, or grazed a neighbouring
  // zone whose higher z-index briefly stole the hover target. We now
  // schedule the collapse on a LEAVE_GRACE_MS timer (80 ms by default
  // via `services/hoverIntent.ts`) so a re-entry within that window
  // cancels it. v8 round-14 unified this with the shared hover-intent
  // constant so external-zone leave-collapse uses the same window.
  let bloomCollapseTimer: ReturnType<typeof setTimeout> | null = null;
  const cancelBloomCollapse = (): void => {
    if (bloomCollapseTimer !== null) {
      clearTimeout(bloomCollapseTimer);
      bloomCollapseTimer = null;
    }
  };

  // v8 round-13 (no-auto-open): the active-petal state is now decoupled
  // from previewZoneId. Pre-round-13 the petal class list keyed off
  // `previewZoneId() === zone.id` and the only path that flipped
  // previewZoneId on hover was `handlePetalEnter` (no debounce). The
  // user reported "stack默认直接打开其中的一个zone" — bloom-open was
  // implicitly committing to a member zone the moment the cursor grazed
  // a petal during the bloom entry animation. Round-13 introduces
  // explicit hover-intent gating:
  //
  //   - `activePetalId` — drives the `.stack-bloom__petal--active`
  //     class (and the breathing pulse). Flips IMMEDIATELY on petal
  //     mouseenter, reverts after a small grace period on mouseleave so
  //     gap-crossing between petals doesn't flicker.
  //   - `previewZoneId` — drives the FocusedZonePreview mounting.
  //     Only opens after a 150 ms hover-intent debounce, OR
  //     synchronously on click ("sticky" commit).
  //   - `previewSticky` — set by click. Sticky previews survive
  //     hover-off of the original petal; only cleared by hovering a
  //     DIFFERENT petal (which switches preview), an explicit close,
  //     or bloom collapse.
  //
  // Default state on bloom open: both signals are null. No member is
  // implicitly active, no preview is implicitly open. Bloom is purely
  // a "show options" affordance until the user makes a choice.
  const [activePetalId, setActivePetalId] = createSignal<string | null>(null);
  const [previewSticky, setPreviewSticky] = createSignal(false);
  // v8 round-14 (unify hover-intent): the round-13 hard-coded values are
  // gone. We now alias the shared `services/hoverIntent.ts` constants so
  // bloom petal wake/leave timing matches external-zone wake/leave
  // timing exactly. The aliases preserve the round-13 source-text
  // contract tests (which assert `PREVIEW_HOVER_INTENT_MS` /
  // `ACTIVE_PETAL_GRACE_MS` appear in callback bodies) while giving the
  // shared constants a single source of truth.
  const PREVIEW_HOVER_INTENT_MS = HOVER_INTENT_MS;
  const ACTIVE_PETAL_GRACE_MS = LEAVE_GRACE_MS;
  let previewOpenTimer: ReturnType<typeof setTimeout> | null = null;
  let activeRevertTimer: ReturnType<typeof setTimeout> | null = null;
  const cancelPreviewOpenTimer = (): void => {
    if (previewOpenTimer !== null) {
      clearTimeout(previewOpenTimer);
      previewOpenTimer = null;
    }
  };
  const cancelActiveRevertTimer = (): void => {
    if (activeRevertTimer !== null) {
      clearTimeout(activeRevertTimer);
      activeRevertTimer = null;
    }
  };

  const displayMode = () => getZoneDisplayMode();
  const baseZone = createMemo(() => props.zones[0]);
  const topZone = createMemo(() => props.zones[props.zones.length - 1]);
  // Fix-FE-7: global setting wins over per-zone override (matches BentoZone).
  const stackDisplayMode = () =>
    displayMode() ?? topZone()?.display_mode ?? baseZone()?.display_mode ?? "hover";
  const stackLocked = () => props.zones.some((zone) => zone.locked);
  const previewZone = createMemo(() =>
    props.zones.find((zone) => zone.id === previewZoneId()) ?? null,
  );

  const wrapperStyle = (): Record<string, string> => {
    const zone = baseZone();
    const pos = dragPosition() ?? zone?.position;
    if (!pos) return {};
    // v9 (Issue Y fix): align the stack ladder with the BentoZone ladder
    // via the shared `styles/zStack.ts` constants. Pre-v9 the stack used
    // `sort_order + 30` at rest while BentoZone used `sort_order + 10`,
    // which made every stack visually outrank every free zone at rest
    // without any semantic reason. The user reported "stack appears on
    // top of zone" which was exactly this ladder mismatch.
    //
    // The promoted (bloom) and drag tiers stay above the hover band
    // because:
    //   - drag (Z_ZONE_DRAG = 1100): the moved cluster must never
    //     disappear under a hovered neighbour or an expanded panel.
    //   - promoted/bloom (Z_ZONE_PROMOTED = 950): petals render OUTSIDE
    //     the wrapper's natural rect (position: fixed), but the wrapper
    //     itself must outrank a hovered neighbour to keep the petals
    //     visually frontmost.
    //
    // Z_ZONE_DRAG is reused for the dragging case (was 950 pre-v9). The
    // change is intentional: the previous 950-during-drag tied with the
    // bloom tier, which meant a stack being dragged past a hovered free
    // zone could lose stacking to a future-expanded panel. v9 promotes
    // it to the same 1100 BentoZone uses so behaviour matches free
    // zones during drag.
    const zIndex = dragPosition() !== null
      ? Z_ZONE_DRAG
      : isBloomed()
        ? Z_ZONE_PROMOTED
        : (topZone()?.sort_order ?? 0) + Z_ZONE_IDLE_OFFSET;
    return {
      left: `${pos.x_percent}%`,
      top: `${pos.y_percent}%`,
      "z-index": String(zIndex),
    };
  };

  const closeTray = (): void => {
    if (stackDisplayMode() === "always") {
      setTrayOpen(true);
      return;
    }
    setTrayOpen(false);
    setPreviewZoneId(null);
    // v8 round-13: also clear active-petal + sticky flag + pending
    // hover-intent timer so a subsequent bloom open starts from a
    // clean "no choice made" state.
    cancelPreviewOpenTimer();
    cancelActiveRevertTimer();
    setActivePetalId(null);
    setPreviewSticky(false);
  };

  const setTrayVisibility = (open: boolean): void => {
    if (!open) {
      closeTray();
      return;
    }
    setTrayOpen(true);
  };

  // v8 round-6 (Bloom-Real-Fix): when the bloom is active and a petal
  // is being previewed, the preview must anchor to the PETAL's rect
  // rather than the wrapper. We snapshot the petal's
  // getBoundingClientRect on every reactive tick that affects layout
  // (preview id, bloom cursor, viewport size) so the preview re-pins
  // itself if the cursor wanders to a different petal mid-bloom.
  // When NOT bloomed (legacy tray-driven preview), this returns null
  // and FocusedZonePreview falls back to the absolute-position layout.
  const [previewAnchorRect, setPreviewAnchorRect] = createSignal<{
    left: number;
    top: number;
    right: number;
    bottom: number;
    width: number;
    height: number;
  } | null>(null);

  const updatePreviewAnchor = (): void => {
    const zone = previewZone();
    if (!zone) {
      setPreviewAnchorRect(null);
      return;
    }
    const viewport = getViewportSize();
    // v8 round-10 (Issue C): when the preview is bloom-anchored, the
    // panel caps at 360×420 so the anchor logic must respect those
    // bounds — otherwise we'd flip horizontal sides based on a
    // non-existent panel width and the user would see the preview pop
    // on the wrong side of the petal.
    const FLOATING_MAX_W = 360;
    const FLOATING_MAX_H = 420;
    const rawW = zone.expanded_size.w_percent > 0
      ? (zone.expanded_size.w_percent / 100) * viewport.width
      : 360;
    const rawH = zone.expanded_size.h_percent > 0
      ? (zone.expanded_size.h_percent / 100) * viewport.height
      : 420;
    const previewWidth = isBloomed()
      ? Math.min(rawW, FLOATING_MAX_W)
      : rawW;
    const previewHeight = isBloomed()
      ? Math.min(rawH, FLOATING_MAX_H)
      : rawH;

    // v8 round-6: when bloomed, anchor to the active petal so the
    // preview pops next to where the user is actually looking. This
    // is the user-visible fix from round-6 — frame analysis of their
    // recording showed the preview WAS rendering, just always against
    // the original capsule far from the petal they hovered.
    if (isBloomed()) {
      const id = previewZoneId();
      const petalEl = id !== null ? petalRefs.get(id) : undefined;
      if (petalEl) {
        const petalRect = petalEl.getBoundingClientRect();
        // v8 round-7: gated diagnostic so the team can verify in the
        // release exe that the anchor IS being set and the rect is
        // sane (non-zero width/height). Hot path is a single property
        // read when the flag is off.
        if (typeof window !== "undefined") {
          const flag = (window as unknown as { __bento_debug_preview?: boolean })
            .__bento_debug_preview;
          if (flag) {
            // eslint-disable-next-line no-console
            console.log("[stack-wrapper:preview-anchor]", {
              zoneId: id,
              zoneName: zone.name,
              petalRect: {
                left: petalRect.left,
                top: petalRect.top,
                width: petalRect.width,
                height: petalRect.height,
              },
              previewWidth,
              previewHeight,
            });
          }
        }
        // Decide grow direction based on where the petal sits in the
        // viewport — preview should always grow toward the interior
        // so it doesn't clip off-screen. We piggyback on
        // decideAnchorFromRect-style logic but inline it here because
        // the contract is slightly different: we want "preview goes
        // RIGHT of petal when petal is in the LEFT half" rather than
        // "panel anchors to RIGHT edge of capsule".
        const petalCenterX = (petalRect.left + petalRect.right) / 2;
        const petalCenterY = (petalRect.top + petalRect.bottom) / 2;
        const horizontal: "left" | "right" =
          petalCenterX + previewWidth + PREVIEW_GAP_PX <= viewport.width
            ? "right"
            : "left";
        const vertical: "top" | "bottom" =
          petalCenterY + previewHeight <= viewport.height
            ? "top"
            : "bottom";
        setPreviewAnchor({ horizontal, vertical });
        setPreviewAnchorRect({
          left: petalRect.left,
          top: petalRect.top,
          right: petalRect.right,
          bottom: petalRect.bottom,
          width: petalRect.width,
          height: petalRect.height,
        });
        return;
      }
    }

    // Legacy tray-driven path: anchor to the wrapper. anchorRect
    // stays null → FocusedZonePreview uses position: absolute.
    if (!wrapperRef) return;
    const rect = wrapperRef.getBoundingClientRect();
    const horizontal =
      rect.right + PREVIEW_GAP_PX + previewWidth <= viewport.width
        ? "right"
        : "left";
    const vertical =
      rect.top + previewHeight <= viewport.height || rect.bottom - previewHeight < 0
        ? "top"
        : "bottom";
    setPreviewAnchor({ horizontal, vertical });
    setPreviewAnchorRect(null);
  };

  // v9 (Issue X): gated diagnostic for the entire dissolve flow. Flip
  // `window.__bento_debug_stack = true` in DevTools (or via the helper
  // exposed from `main.tsx`) to log every step from right-click capture
  // → menu open → dissolve click → IPC call → store reload. Hot path is
  // a single property read when the flag is off.
  const debugStack = (label: string, extra: Record<string, unknown> = {}): void => {
    if (typeof window === "undefined") return;
    const flag = (window as unknown as { __bento_debug_stack?: boolean })
      .__bento_debug_stack;
    if (!flag) return;
    // eslint-disable-next-line no-console
    console.log(`[stack:${props.stackId}] ${label}`, {
      members: props.zones.map((z) => z.id),
      contextMenuOpen: contextMenuOpen(),
      previewZoneId: previewZoneId(),
      isBloomed: isBloomed(),
      isDragging: isDragging(),
      ...extra,
    });
  };

  const handleContextMenu = (e: MouseEvent): void => {
    e.preventDefault();
    e.stopPropagation();
    debugStack("contextmenu:captured", {
      cursor: { x: e.clientX, y: e.clientY },
    });
    // v9 (Issue X fix): on right-click, immediately collapse bloom + tray
    // + preview so the dissolve menu lands on a clean visual surface.
    // Pre-v9 the bloom petals + buffer (z-index 49-51) stayed visible
    // around the cursor while the user moved toward "解散堆栈" — the
    // mouseleave on the wrapper fired ~80 ms after the cursor crossed
    // the wrapper edge, the bloomCollapseTimer was set, and depending
    // on cursor path the bloom either retracted mid-click (visual jank)
    // OR the buffer halo's z-index 49 sat under the menu (z-index 10000
    // is correct, but the user reported visual confusion). Forcing the
    // bloom + tray closed at right-click time guarantees the menu is
    // the only interactive surface.
    cancelBloomCollapse();
    // v8 round-13: also tear down the round-13 hover-intent state so a
    // pending preview-open timer doesn't fire ON TOP of the freshly
    // opened context menu.
    cancelPreviewOpenTimer();
    cancelActiveRevertTimer();
    setActivePetalId(null);
    setPreviewSticky(false);
    setIsBloomed(false);
    setPreviewZoneId(null);
    setTrayOpen(false);
    setContextMenuOpen({ x: e.clientX, y: e.clientY });
    debugStack("contextmenu:menu-set", {
      cursor: { x: e.clientX, y: e.clientY },
    });
  };

  const handleDissolve = async (): Promise<void> => {
    debugStack("dissolve:click");
    setContextMenuOpen(null);
    setPreviewZoneId(null);
    setTrayOpen(false);
    setIsBloomed(false);
    // v8 round-13: clear hover-intent state too so a stale timer can't
    // re-pop a preview after the dissolve IPC roundtrip.
    cancelPreviewOpenTimer();
    cancelActiveRevertTimer();
    setActivePetalId(null);
    setPreviewSticky(false);
    debugStack("dissolve:invoking-ipc");
    const ok = await unstackZonesAction(props.stackId);
    debugStack("dissolve:ipc-returned", {
      ok,
      // After loadZones() inside unstackZonesAction, the store has
      // re-derived. If `ok` is true and stackMap no longer contains
      // this stack, the StackWrapper is about to unmount — log that
      // for confirmation.
      stillExists: zonesStore.zones.some(
        (z) => z.stack_id === props.stackId,
      ),
    });
  };

  const handleDetach = async (zoneId: string): Promise<void> => {
    debugStack("detach:click", { zoneId });
    if (previewZoneId() === zoneId) {
      setPreviewZoneId(null);
      setPreviewSticky(false);
    }
    if (activePetalId() === zoneId) {
      setActivePetalId(null);
    }
    setContextMenuOpen(null);
    const ok = await detachZoneFromStackAction(props.stackId, zoneId);
    debugStack("detach:ipc-returned", { zoneId, ok });
  };

  const handleSelectPreview = (zoneId: string): void => {
    setTrayVisibility(true);
    // v8 round-13: derive the next preview value first so we can mirror
    // it into the sticky flag in the same tick. The tray-driven path is
    // an explicit click, treat it as sticky so a sibling petal hover
    // doesn't tear it down (matches the round-13 click-to-stick
    // semantics on the bloom path).
    const next = previewZoneId() === zoneId ? null : zoneId;
    setPreviewZoneId(next);
    setPreviewSticky(next !== null);
  };

  const handleCapsuleMouseDown = (e: MouseEvent): void => {
    if (e.button !== 0) return;
    if (stackLocked()) return;
    e.preventDefault();
    e.stopPropagation();

    const zone = baseZone();
    if (!zone) return;

    // Group-drag path: when this stack is part of a multi-selection alongside
    // free-standing zones (or other stacks), drag every selected zone as one.
    // The preview is driven by the shared selection store so BentoZone's
    // dragPosition/preview consumers light up too.
    const memberIds = new Set(props.zones.map((m) => m.id));
    const currentSelection = selectedZoneIds();
    const stackIsSelected = props.zones.some((m) => currentSelection.has(m.id));
    if (stackIsSelected && currentSelection.size > 1) {
      const allSelectedZones = zonesStore.zones.filter((z) =>
        currentSelection.has(z.id),
      );
      const reachesOutsideStack = allSelectedZones.some(
        (z) => !memberIds.has(z.id),
      );
      if (
        reachesOutsideStack &&
        allSelectedZones.length >= 2 &&
        !allSelectedZones.some((z) => z.locked)
      ) {
        // Include every stack member (selected or not) so the cluster moves
        // as a rigid unit alongside the cross-stack selection.
        const draggable = new Map<string, BentoZoneType>();
        for (const z of allSelectedZones) draggable.set(z.id, z);
        for (const m of props.zones) draggable.set(m.id, m);

        const releaseDrag = acquireDragLock();
        const startX = e.clientX;
        const startY = e.clientY;
        let moved = false;

        beginGroupZoneDrag(
          [...draggable.values()].map((z) => ({
            id: z.id,
            position: z.position,
          })),
        );
        // v6 fix #3: global drag flag for cross-zone hover/click guards.
        setIsDragging(true);

        // v8 round-4 #1: rAF-coalesce — see BentoZone.tsx for rationale.
        // v8 round-4 real-fix (Bug 1): center-on-cursor for the dragged
        // stack's representative capsule, then ship a delta to
        // `updateGroupZoneDrag` so every other selected member shifts
        // identically. Same model as the BentoZone capsule-driven group
        // drag — preserves group preview semantics.
        const stackCapPx = getCapsulePixelSize(topZone() ?? zone);
        let lastMoveEvent: MouseEvent | null = null;
        let moveRafId: number | null = null;
        const flushMove = (): void => {
          moveRafId = null;
          const ev = lastMoveEvent;
          if (!ev) return;
          const dx = ev.clientX - startX;
          const dy = ev.clientY - startY;
          if (!moved && Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;
          moved = true;
          const desiredX =
            ((ev.clientX - stackCapPx.width / 2) / window.innerWidth) * 100;
          const desiredY =
            ((ev.clientY - stackCapPx.height / 2) / window.innerHeight) * 100;
          updateGroupZoneDrag({
            x_percent: desiredX - zone.position.x_percent,
            y_percent: desiredY - zone.position.y_percent,
          });
        };
        const onMouseMove = (ev: MouseEvent): void => {
          lastMoveEvent = ev;
          if (moveRafId !== null) return;
          moveRafId = requestAnimationFrame(flushMove);
        };

        const onMouseUp = async (): Promise<void> => {
          document.removeEventListener("mousemove", onMouseMove);
          document.removeEventListener("mouseup", onMouseUp);
          if (moveRafId !== null) {
            cancelAnimationFrame(moveRafId);
            moveRafId = null;
          }
          // v8 round-4 hotfix (Bug 1): flush the latest mousemove event
          // SYNCHRONOUSLY before reading the persisted preview. Without
          // this, the cancelAnimationFrame above discards the pending tick
          // and the group preview is one frame stale — the dropped stack
          // group oscillates to the previous frame's position on render.
          flushMove();
          try {
            const finalPreview = endGroupZoneDrag();
            if (!moved) return;
            suppressNextClick = true;
            const updates = Object.entries(finalPreview).map(
              ([id, position]) => ({ id, position }),
            );
            if (updates.length > 0) {
              await bulkUpdateZones(updates);
              await loadZones();
            }
          } finally {
            // v6 fix #3: clear the global flag last so the synthesized
            // click off this mouseup is still suppressed.
            setIsDragging(false);
            releaseDrag();
          }
        };

        document.addEventListener("mousemove", onMouseMove);
        document.addEventListener("mouseup", onMouseUp);
        return;
      }
    }

    // Default path: drag the stack as a self-contained cluster — each member
    // shifts by the same delta as the base capsule. This is unchanged from
    // the pre-#3 behaviour for a stack that isn't in a wider selection.
    const releaseDrag = acquireDragLock();
    const startX = e.clientX;
    const startY = e.clientY;
    const startPositions = props.zones.map((entry) => ({
      id: entry.id,
      x: entry.position.x_percent,
      y: entry.position.y_percent,
    }));
    const capsuleSize = getCapsulePixelSize(topZone() ?? zone);
    let moved = false;
    // v6 fix #3: global drag flag for cross-zone hover/click guards.
    setIsDragging(true);

    // v8 round-4 #1: rAF-coalesce single-stack drag too.
    // v8 round-4 real-fix (Bug 1): switch from delta-based positioning
    // (`zone.position + dx`) to absolute center-on-cursor. The base
    // capsule top-left = cursor - capsulePx/2, so the cluster visibly
    // settles where the cursor releases. Member offsets relative to the
    // base are preserved via the deltaX/deltaY math in onMouseUp below.
    let lastMoveEvent: MouseEvent | null = null;
    let moveRafId: number | null = null;
    const flushMove = (): void => {
      moveRafId = null;
      const ev = lastMoveEvent;
      if (!ev) return;
      const dx = ev.clientX - startX;
      const dy = ev.clientY - startY;
      if (!moved && Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;
      moved = true;
      const maxXPct = 100 - (capsuleSize.width / window.innerWidth) * 100;
      const maxYPct = 100 - (capsuleSize.height / window.innerHeight) * 100;
      const desiredX =
        ((ev.clientX - capsuleSize.width / 2) / window.innerWidth) * 100;
      const desiredY =
        ((ev.clientY - capsuleSize.height / 2) / window.innerHeight) * 100;
      setDragPosition({
        x_percent: clampPct(desiredX, maxXPct),
        y_percent: clampPct(desiredY, maxYPct),
      });
    };
    const onMouseMove = (ev: MouseEvent): void => {
      lastMoveEvent = ev;
      if (moveRafId !== null) return;
      moveRafId = requestAnimationFrame(flushMove);
    };

    const onMouseUp = async (): Promise<void> => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      if (moveRafId !== null) {
        cancelAnimationFrame(moveRafId);
        moveRafId = null;
      }
      // v8 round-4 hotfix (Bug 1): flush the latest mousemove event
      // SYNCHRONOUSLY before reading dragPosition. Without this, the
      // cancelAnimationFrame above discards the pending tick and the
      // stack capsule is persisted at the second-to-last frame's coords,
      // which on the next render snaps the dropped stack visibly back
      // to its previous frame's location.
      flushMove();
      try {
        const finalPos = dragPosition();
        if (!moved || !finalPos) {
          return;
        }
        suppressNextClick = true;
        const deltaX = finalPos.x_percent - zone.position.x_percent;
        const deltaY = finalPos.y_percent - zone.position.y_percent;
        await Promise.all(
          startPositions.map((entry) =>
            updateZone(entry.id, {
              position: {
                x_percent: clampPct(entry.x + deltaX, 96),
                y_percent: clampPct(entry.y + deltaY, 96),
              },
            }),
          ),
        );
      } finally {
        setDragPosition(null);
        // v6 fix #3: clear the global flag last so the synthesized click
        // off this mouseup is still suppressed by handleZoneClick.
        setIsDragging(false);
        releaseDrag();
      }
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  };

  // v8 round-12 (bloom-row-layout): petals lay out as a horizontal row
  // directly below (or above) the capsule. The radial constants from
  // rounds 9–11 (BLOOM_BASE_RADIUS, BLOOM_RADIUS_PER_PETAL, BLOOM_MIN_RADIUS,
  // PETAL_HALF_DIAG) are gone.
  //
  // v8 round-14 (many-member perf): petal size is no longer a constant.
  // Round-12 hard-coded 108×96 worked for ≤ 4 members but became visually
  // crowded AND animation-expensive for 12+ members. `pickPetalSize`
  // returns one of four buckets keyed off member count, with a smaller
  // tile + smaller icon for large stacks. The wrapper also gains a
  // `--many-members` modifier when count > 8 so CSS can disable the
  // breathing pulse + simplify box-shadow for small petals.
  //
  // v8 round-14 (overflow cap): when a stack has > MAX_VISIBLE_MEMBERS
  // (24) members, only the first MAX_VISIBLE_MEMBERS - 1 petals render
  // with their actual zone identity; the final slot becomes a "+N more"
  // indicator so the bloom layout doesn't explode visually. Clicking
  // the indicator is a no-op for now (deferred to a future round).
  const MAX_VISIBLE_MEMBERS = 24;
  const memberCount = createMemo(() => props.zones.length);
  const petalSize = createMemo(() => pickPetalSize(memberCount()));
  const isManyMembers = createMemo(() => memberCount() > 8);
  // The visible-petals count: clamp to MAX_VISIBLE_MEMBERS. When the
  // stack overflows we render MAX_VISIBLE_MEMBERS slots total — the
  // last slot is the overflow indicator, so up to MAX_VISIBLE_MEMBERS - 1
  // real members are visible.
  const visiblePetalCount = createMemo(() =>
    Math.min(memberCount(), MAX_VISIBLE_MEMBERS),
  );
  const overflowCount = createMemo(() =>
    Math.max(0, memberCount() - (MAX_VISIBLE_MEMBERS - 1)),
  );
  const isOverflowing = createMemo(() => memberCount() > MAX_VISIBLE_MEMBERS);
  /** Members that get a real petal in the bloom render. When the stack
   *  fits within MAX_VISIBLE_MEMBERS this is the full list; when it
   *  overflows, this is the first MAX_VISIBLE_MEMBERS - 1 members so
   *  the final row slot is reserved for the overflow indicator. */
  const visibleZones = createMemo(() => {
    if (!isOverflowing()) return props.zones;
    return props.zones.slice(0, MAX_VISIBLE_MEMBERS - 1);
  });
  // v8 round-12: bloom-active indicator changes from `bloomCursor` (the
  // captured cursor coords from rounds 4–11) to a simpler "are we
  // bloomed" + "where to drop from" pair. The capsule rect is the
  // single source of truth for layout and animation origin.
  const [bloomedTick, setBloomedTick] = createSignal(0);
  // Touch this signal whenever the petal layout needs to be re-evaluated
  // (viewport resize, bloom toggle). The capsule's getBoundingClientRect
  // also changes when the user drags the wrapper, but drag flips
  // bloomActive false anyway so we don't track that here.
  const bumpBloomedTick = (): void => {
    setBloomedTick((n) => n + 1);
  };

  /** Capsule rect snapshot in viewport-fixed coords. Returns null when
   *  the wrapper hasn't mounted yet or there's no DOM (test env). */
  const getCapsuleRect = (): { x: number; y: number; width: number; height: number } | null => {
    if (typeof window === "undefined") return null;
    if (!wrapperRef) return null;
    const rect = wrapperRef.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return null;
    // The wrapper contains the capsule (and may also contain the tray
    // when open). For the row layout we anchor on the capsule's own
    // rect, which sits at the top of the wrapper. Use getCapsuleBoxPx
    // to derive the visible capsule width/height — this matches the
    // capsule's rendered footprint regardless of tray state.
    const top = topZone() ?? baseZone();
    const boxPx = getCapsuleBoxPx(top?.capsule_shape, top?.capsule_size);
    return {
      x: rect.left,
      y: rect.top,
      // The wrapper's rect width may be inflated by the tray; clamp
      // back to the capsule's own width so the centre-on-capsule math
      // doesn't shift when the tray is open.
      width: boxPx.width,
      height: boxPx.height,
    };
  };

  /** Resolve the petal row layout for the current capsule rect + member
   *  count. Returns null when not bloomed or no DOM.
   *
   *  v8 round-14: petalSize is now adaptive (round-12 used a fixed
   *  108×96). The solver receives the bucket-picked size keyed off the
   *  member count, AND the petal count is clamped to
   *  MAX_VISIBLE_MEMBERS so the row geometry accounts for the
   *  overflow indicator slot rather than the full member count. */
  const petalRow = createMemo(() => {
    if (!isBloomed()) return null;
    // Touch the tick so resize / mode-flip re-runs the memo.
    bloomedTick();
    if (typeof window === "undefined") return null;
    const capsuleRect = getCapsuleRect();
    if (!capsuleRect) return null;
    const size = petalSize();
    return resolvePetalRow({
      capsuleRect,
      petalSize: { width: size.width, height: size.height },
      petalCount: visiblePetalCount(),
      viewport: { width: window.innerWidth, height: window.innerHeight },
    });
  });

  /** Capsule centre in viewport-fixed coords. Used as the animation
   *  origin so each petal "drops out" of the capsule. */
  const dropFromCenter = (): { x: number; y: number } | null => {
    const rect = getCapsuleRect();
    if (!rect) return null;
    return {
      x: rect.x + rect.width / 2,
      y: rect.y + rect.height / 2,
    };
  };

  // v8 #4 round-2: gated diagnostic logger so the team can verify the bloom
  // state machine in the release exe by toggling `window.__bento_debug_bloom`
  // in DevTools. Hot path is a single property read when the flag is off.
  // v8 round-12: switched ring-specific fields (rotationDeg/radius/etc.)
  // for the row solver's `flipped` and `wrapped` flags so a future
  // maintainer can see at a glance whether the row flipped above the
  // capsule (capsule near bottom edge) or wrapped to multi-row.
  const debugBloom = (label: string, extra: Record<string, unknown> = {}): void => {
    if (typeof window === "undefined") return;
    const flag = (window as unknown as { __bento_debug_bloom?: boolean })
      .__bento_debug_bloom;
    if (!flag) return;
    const row = petalRow();
    // eslint-disable-next-line no-console
    console.log(`[bloom:${props.stackId}] ${label}`, {
      isBloomed: isBloomed(),
      trayOpen: trayOpen(),
      stackLocked: stackLocked(),
      isDragging: isDragging(),
      stackDisplayMode: stackDisplayMode(),
      members: props.zones.length,
      rowFlipped: row?.flipped ?? null,
      rowWrapped: row?.wrapped ?? null,
      rowCount: row?.centers.length ?? null,
      ...extra,
    });
  };

  const handleMouseEnter = (_e: MouseEvent) => {
    debugBloom("mouseenter");
    // v6 fix #3: don't pop the tray while a drag is in flight — a flick
    // across this stack during a drag must not trigger an expand.
    if (isDragging()) return;
    // v8 #4: a re-entry inside the 80 ms grace window cancels the pending
    // collapse so the bloom never blinks.
    cancelBloomCollapse();
    // v7 #4: hover-bloom is now the canonical entry point for picking a
    // single member out of a stack. It supersedes the "hover" display
    // mode's tray pop — opening BOTH at once would double-render every
    // member and let the cursor escape into the gap between them.
    // The legacy tray is still available via "click" / "always" modes
    // and the keyboard shortcut path. In "always" mode we keep the
    // tray open and skip the bloom (tray already shows everything).
    if (stackLocked()) return;
    if (stackDisplayMode() === "always") {
      setTrayVisibility(true);
      debugBloom("mouseenter:always-mode-skip");
      return;
    }
    // v8 round-12: layout no longer depends on cursor coords. The row
    // is anchored to the capsule rect, which the petalRow memo reads
    // from wrapperRef.getBoundingClientRect() at render time.
    bumpBloomedTick();
    setIsBloomed(true);
    debugBloom("mouseenter:bloom-set");
  };

  const handleMouseLeave = () => {
    debugBloom("mouseleave");
    if (stackDisplayMode() === "hover") {
      closeTray();
    }
    // v8 #4: defer bloom + preview teardown by LEAVE_GRACE_MS so a brief
    // re-entry (cursor crossing the buffer edge, or a neighbouring zone
    // briefly stealing hover via higher z-index) cancels it via
    // clearTimeout. v8 round-14: the hard-coded 80 ms moved to the
    // shared `services/hoverIntent.ts` constants so external-zone
    // hover-leave and bloom collapse share a single source of truth.
    //
    // Sticky-preview escalation: when the user has explicitly clicked a
    // petal to commit a sticky preview, the leave-grace window extends
    // to STICKY_GRACE_MS (200 ms) — the user has made a deliberate
    // choice and the threshold for tearing down their committed surface
    // should be more lenient than a transient hover-off. A regular
    // hover-only bloom collapses in LEAVE_GRACE_MS (80 ms) as before.
    cancelBloomCollapse();
    const collapseGrace = previewSticky() ? STICKY_GRACE_MS : LEAVE_GRACE_MS;
    bloomCollapseTimer = setTimeout(() => {
      bloomCollapseTimer = null;
      setIsBloomed(false);
      setPreviewZoneId(null);
      // v8 round-13: full bloom collapse drops every transient
      // selection — active petal + sticky flag + pending hover-intent
      // timer — so a re-bloom starts from the same blank slate as the
      // first bloom of the session.
      cancelPreviewOpenTimer();
      cancelActiveRevertTimer();
      setActivePetalId(null);
      setPreviewSticky(false);
      debugBloom("mouseleave:collapse-fired");
    }, collapseGrace);
  };

  // v6 #4 bloom-petal interaction: each petal represents a single member
  // zone in the fanned-out state.
  //
  // v8 round-13 (no-auto-open): hover commits to active state IMMEDIATELY
  // (so the breathing pulse + soft ring confirm "you're focused on this
  // member"), but the FocusedZonePreview opens only after a 150 ms
  // hover-intent debounce. This solves the user's complaint that the
  // bloom "默认直接打开其中的一个zone" — pre-round-13 the cursor
  // grazing a petal during the bloom entry animation set previewZoneId
  // synchronously, popping a preview before the user had even chosen.
  // The 150 ms threshold is short enough to feel responsive but long
  // enough to skip incidental cursor sweeps across the row.
  //
  // If a sticky preview is already open (set by a previous click) and
  // the user hovers a DIFFERENT petal, switch the preview synchronously
  // — there's no "opening tear-down" cost to amortise because the
  // FocusedZonePreview is already mounted; we just swap its zone prop.
  const handlePetalEnter = (zoneId: string) => {
    if (isDragging()) return;
    cancelActiveRevertTimer();
    setActivePetalId(zoneId);
    // Preview swap path: an existing sticky preview points at a
    // different petal → swap immediately so the user doesn't see a
    // 150 ms latency on a panel that is already on screen.
    if (previewSticky() && previewZoneId() !== null && previewZoneId() !== zoneId) {
      cancelPreviewOpenTimer();
      setPreviewZoneId(zoneId);
      return;
    }
    // Hover-intent debounce: schedule the preview open. Clear any prior
    // pending timer — re-entering a different petal restarts the clock.
    cancelPreviewOpenTimer();
    previewOpenTimer = setTimeout(() => {
      previewOpenTimer = null;
      // Guard the deferred write: if the bloom collapsed, the user
      // started a drag, or the active petal moved on, drop the open.
      if (!bloomActive()) return;
      if (activePetalId() !== zoneId) return;
      setPreviewZoneId(zoneId);
    }, PREVIEW_HOVER_INTENT_MS);
  };
  const handlePetalLeave = (zoneId: string) => {
    // Hover-out: cancel any pending preview-open timer so the user's
    // quick sweep across petals doesn't leave a delayed bomb that
    // pops a preview the user is no longer looking at.
    cancelPreviewOpenTimer();
    // Reset active state after a small grace period — long enough that
    // crossing the gap between two adjacent petals doesn't strobe the
    // active ring on/off, short enough that an intent-confirmed leave
    // (cursor moves toward the buffer edge, away from any petal) feels
    // responsive.
    cancelActiveRevertTimer();
    activeRevertTimer = setTimeout(() => {
      activeRevertTimer = null;
      // Only revert if the active id didn't move on to a different
      // petal in the meantime (mouseenter on a sibling clears + resets
      // the active id synchronously).
      if (activePetalId() === zoneId) {
        setActivePetalId(null);
      }
    }, ACTIVE_PETAL_GRACE_MS);
  };
  const handlePetalClick = (e: MouseEvent, zoneId: string) => {
    e.preventDefault();
    e.stopPropagation();
    if (isDragging()) return;
    // Click is the explicit commit: synchronously set both the active
    // petal AND the preview, and mark the preview as sticky so a
    // hover-off doesn't tear it down. Clicking the SAME petal whose
    // preview is sticky toggles it closed (matches the prev round
    // semantics, just with the sticky flag tracked explicitly).
    cancelPreviewOpenTimer();
    cancelActiveRevertTimer();
    if (previewSticky() && previewZoneId() === zoneId) {
      setPreviewZoneId(null);
      setPreviewSticky(false);
      setActivePetalId(null);
      return;
    }
    setActivePetalId(zoneId);
    setPreviewZoneId(zoneId);
    setPreviewSticky(true);
  };

  const handleCapsuleClick = (_e: MouseEvent) => {
    if (suppressNextClick) {
      suppressNextClick = false;
      return;
    }
    // v6 fix #3: guard against the synthesized click from a drag mouseup.
    if (isDragging()) return;
    const mode = stackDisplayMode();
    if (mode === "click") {
      setTrayVisibility(!trayOpen());
      return;
    }
    if (mode === "always") {
      // Tray is already pinned open; click is a no-op.
      return;
    }
    // v8 round-3: hover mode now also responds to click as a "tap to bloom"
    // affordance — without this, a stack newly formed via drag-on-zone
    // looked inert because the user's cursor was already inside the
    // wrapper at the moment of drop and `mouseenter` had already fired
    // and exited (no fresh `mouseenter` while motionless). Click toggles
    // bloom so the user always has a positive trigger.
    if (stackLocked()) return;
    if (isBloomed()) {
      setIsBloomed(false);
      setPreviewZoneId(null);
      // v8 round-13: same teardown as the bloom collapse — clear the
      // hover-intent state so the next bloom open starts blank.
      cancelPreviewOpenTimer();
      cancelActiveRevertTimer();
      setActivePetalId(null);
      setPreviewSticky(false);
      return;
    }
    cancelBloomCollapse();
    // v8 round-12: row layout is anchored to the capsule rect, no need
    // to capture cursor coords. The petalRow memo reads the current
    // capsule rect on render.
    bumpBloomedTick();
    setIsBloomed(true);
  };

  const handleKeyDown = (e: KeyboardEvent): void => {
    if (e.key === "Escape") {
      closeTray();
      return;
    }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      setTrayVisibility(!trayOpen());
    }
  };

  const handleOutsidePointer = (e: MouseEvent): void => {
    const target = e.target as HTMLElement | null;
    const insideContext = target?.closest?.(".stack-context-menu");
    const insideWrapper = wrapperRef?.contains(target ?? null) ?? false;
    if (contextMenuOpen() && !insideContext) {
      setContextMenuOpen(null);
    }
    if (
      stackDisplayMode() === "click" &&
      trayOpen() &&
      !insideWrapper &&
      !insideContext
    ) {
      closeTray();
    }
  };

  // v8 round-4 hotfix (Bug 2): register the StackWrapper element with the
  // cursor hit-test poller so the webview captures cursor events whenever
  // the cursor sits over a stack. Previously only BentoZone instances
  // were registered — for free-standing stacks the poller never noticed
  // the cursor, the state machine sat in PASSTHROUGH, and the webview
  // ignored every click. Existing pre-formed stacks "worked" only because
  // either an adjacent free zone or a recently-released drag held
  // passthrough off via the GRACE_PERIOD; freshly auto-stacked clusters
  // lose that bridging effect the moment GRACE_PERIOD expires (~350 ms
  // after drop), and the user's next hover/click silently falls through
  // to the desktop. The capsule box used for the inflate calculation is
  // the top-of-stack member because that is the visible card.
  onMount(() => {
    document.addEventListener("mousedown", handleOutsidePointer);
    if (wrapperRef) {
      const top = topZone() ?? baseZone();
      const boxPx = getCapsuleBoxPx(top?.capsule_shape, top?.capsule_size);
      const pos = baseZone()?.position ?? { x_percent: 0, y_percent: 0 };
      registerZoneElement(wrapperRef, {
        inflate: computeInflateForPosition(pos, { kind: "stack", boxPx }),
      });
    }
  });

  onCleanup(() => {
    document.removeEventListener("mousedown", handleOutsidePointer);
    // v8 #4: cancel any pending bloom collapse so a stack that unmounts
    // mid-grace-window doesn't trip the timer on a stale closure.
    cancelBloomCollapse();
    // v8 round-13: same hygiene for the round-13 hover-intent timers —
    // dropping a stack mid-debounce must not leave a setTimeout alive
    // that fires on a torn-down closure.
    cancelPreviewOpenTimer();
    cancelActiveRevertTimer();
    if (wrapperRef) {
      unregisterZoneElement(wrapperRef);
    }
    // v8 round-5 (Bug B): symmetric cleanup for the bloom hit-test
    // registrations. Unmounting the StackWrapper while the bloom is
    // open (e.g. dissolve via context menu, drag-merge into another
    // stack) must drop the buffer + petal refs from the poller; leaving
    // them registered would keep the cursor "captured" over a stale
    // rect even after the elements have detached from the DOM.
    if (bloomBufferRef) {
      unregisterZoneElement(bloomBufferRef);
    }
    for (const el of petalRefs.values()) {
      unregisterZoneElement(el);
    }
    petalRefs.clear();
  });

  createEffect(() => {
    const mode = stackDisplayMode();
    if (import.meta.env.DEV) {
      // eslint-disable-next-line no-console
      console.debug("[picker-stack-effect]", {
        stackId: props.stackId,
        mode,
        trayOpen: trayOpen(),
      });
    }
    if (mode === "always") {
      setTrayVisibility(true);
      return;
    }
    // v8 fix #2 (round 2): when picker leaves "always", force-close the
    // persistent tray (and bloom + preview) so the mode switch is visibly
    // instantaneous. Round 1 already covered the trayOpen-true path; we now
    // clear bloom/preview unconditionally because both can latch open in
    // release exe via stale hover state and otherwise mask the mode change.
    setTrayOpen(false);
    setPreviewZoneId(null);
    setIsBloomed(false);
    // v8 round-13: when the display mode flips out of "always", also
    // drop the round-13 hover-intent state so a stale active petal /
    // sticky preview doesn't survive into the new mode.
    cancelPreviewOpenTimer();
    cancelActiveRevertTimer();
    setActivePetalId(null);
    setPreviewSticky(false);
  });

  createEffect(() => {
    trayOpen();
    previewZoneId();
    dragPosition();
    getViewportSize();
    // v8 round-12: re-snapshot the preview anchor rect when bloom toggles
    // or the layout tick advances (viewport resize, mode flip). Cursor
    // tracking from rounds 4–11 is gone — petal positions only change on
    // those events now, so the dependency list shrinks accordingly.
    bloomedTick();
    isBloomed();
    queueMicrotask(updatePreviewAnchor);
  });

  // v8 round-12: bump the layout tick whenever the viewport size
  // changes so the petal row re-resolves (clamps recompute). This
  // also re-runs the petalRow memo so flipped/wrapped flags refresh.
  createEffect(() => {
    getViewportSize();
    bumpBloomedTick();
  });

  // v6 #4: kill bloom + preview the moment ANY drag starts (this stack,
  // a sibling stack, a free zone, an item). Without this guard a drag
  // initiated AFTER bloom is up would leave the petals fanned out and
  // catch hovers as the cursor sweeps across.
  createEffect(() => {
    if (isDragging()) {
      // v8 #4: drag wins immediately — no 80 ms grace window here, the
      // cursor must visually leave the cluster while a drag is alive.
      cancelBloomCollapse();
      setIsBloomed(false);
      setPreviewZoneId(null);
      // v8 round-13: kill hover-intent state too. A petal-hover timer
      // firing during a drag would re-pop a preview on top of the
      // dragged cluster, which would never desync visually.
      cancelPreviewOpenTimer();
      cancelActiveRevertTimer();
      setActivePetalId(null);
      setPreviewSticky(false);
    }
  });

  // v6 #4: bloomed only fires the visual transform; the actual member
  // zones still live behind the StackCapsule. We render lightweight
  // "petal" buttons positioned via CSS nth-child rules so the user can
  // hover/click any single member after the cluster fans open.
  const bloomActive = createMemo(
    () => isBloomed() && !trayOpen() && !stackLocked() && !isDragging(),
  );

  // v8 round-8 polish #D1: smooth bloom-collapse — keep the petal DOM
  // mounted for ~220 ms after `bloomActive` flips false so the CSS
  // transitions (transform 260ms / opacity 200ms) on .stack-bloom__petal
  // can play in REVERSE. Without this, <Show when={bloomActive()}> would
  // immediately tear the petals out of the DOM and the user would see
  // them snap to nothing.
  //
  // Mechanism:
  //   - bloomActive  → controls `--bloomed` CSS class on the wrapper
  //                    (drives the petal `opacity: 1; scale(1)` rule).
  //   - bloomLeaving → controls the per-petal `--leaving` modifier
  //                    (triggers the reverse-stagger exit keyframe).
  //   - bloomVisible → controls whether <Show> renders the petal DOM.
  //
  // v8 round-11 (bloom-elegant-anim): we now drive entry + exit via
  // dedicated CSS keyframes instead of a transition pair. When
  // bloomActive flips false:
  //   1. bloomLeaving flips true → each petal gets `.stack-bloom__petal
  //      --leaving`, triggering `stack-bloom-petal-exit` (220 ms,
  //      reverse stagger).
  //   2. bloomVisible stays true for 240 ms (matches the longest exit
  //      delay + duration: (count-1)·30ms + 220ms ≤ 220 + 5·30 = 370 ms
  //      for typical clusters; we use 240 ms because the visual fade is
  //      mostly done by then and any tail beyond is barely perceptible).
  //   3. After 240 ms bloomVisible flips false → <Show> tears the DOM.
  // Re-blooming inside that window cancels the pending unmount AND
  // bloomLeaving flips back to false (the wrapper class flips back,
  // entry keyframes restart from the resting position).
  const [bloomVisible, setBloomVisible] = createSignal(false);
  const [bloomLeaving, setBloomLeaving] = createSignal(false);
  let bloomUnmountTimer: ReturnType<typeof setTimeout> | null = null;
  const cancelBloomUnmount = (): void => {
    if (bloomUnmountTimer !== null) {
      clearTimeout(bloomUnmountTimer);
      bloomUnmountTimer = null;
    }
  };
  createEffect(() => {
    const active = bloomActive();
    if (active) {
      cancelBloomUnmount();
      setBloomLeaving(false);
      setBloomVisible(true);
      return;
    }
    // Inactive — flip the leaving flag so the exit keyframe runs, then
    // schedule the unmount after the keyframe finishes.
    if (bloomVisible()) {
      cancelBloomUnmount();
      setBloomLeaving(true);
      bloomUnmountTimer = setTimeout(() => {
        bloomUnmountTimer = null;
        setBloomVisible(false);
        setBloomLeaving(false);
      }, 240);
    }
  });
  onCleanup(() => {
    cancelBloomUnmount();
  });

  // v8 #4 round-2: trace every bloomActive transition so the team can see in
  // DevTools which gate (tray/locked/dragging/isBloomed) flipped the memo.
  createEffect(() => {
    debugBloom("bloomActive:transition", { bloomActive: bloomActive() });
  });

  // v8 round-5 (Bug B real-fix): keep the bloom-buffer halo + every petal
  // registered with the cursor hit-test poller while the bloom is active.
  // Petals + buffer use `position: fixed` and live OUTSIDE the wrapper's
  // bounding rect; without registration the poller's hit-test only sees
  // the capsule rect, so cursor-over-petal lands in PASSTHROUGH and the
  // webview never receives the click that opens FocusedZonePreview. We
  // re-register on every reactive tick (members, cursor, bloomActive) so
  // newly-mounted petals are picked up the same frame they appear, and
  // we tear them down via onCleanup so the unregister fires immediately
  // when bloomActive flips false (instead of waiting for the next tick).
  createEffect(() => {
    if (!bloomActive()) return;
    // Touch the layout tick + member count so the effect re-runs
    // whenever petals re-position (and thus may extend beyond the
    // previous halo). v8 round-12: cursor signal removed.
    bloomedTick();
    void props.zones.length;
    // Snapshot the registrations so onCleanup can undo exactly the set
    // we just registered — guards against `petalRefs` mutating between
    // the register pass and the cleanup phase (e.g. a zone unmounting
    // mid-effect leaves a stale Map entry).
    const registered: HTMLElement[] = [];
    if (bloomBufferRef) {
      registerZoneElement(bloomBufferRef);
      registered.push(bloomBufferRef);
    }
    for (const el of petalRefs.values()) {
      registerZoneElement(el);
      registered.push(el);
    }
    onCleanup(() => {
      for (const el of registered) {
        unregisterZoneElement(el);
      }
    });
  });

  return (
    <div
      ref={wrapperRef}
      class={`stack-wrapper ${trayOpen() ? "stack-wrapper--open" : ""} ${stackLocked() ? "stack-wrapper--locked" : ""} ${bloomActive() ? "stack-wrapper--bloomed" : ""} ${dragPosition() !== null ? "stack-wrapper--dragging" : ""} ${isManyMembers() ? "stack-wrapper--many-members" : ""}`}
      style={wrapperStyle()}
      role="group"
      aria-label={t("stackAriaLabel").replace("{n}", String(props.zones.length))}
      aria-expanded={trayOpen()}
      tabIndex={0}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      onKeyDown={handleKeyDown}
    >
      {/*
        * v8 #4: also fire the bloom-open path when the cursor lands
        * directly on the inner surface. SolidJS's `onMouseEnter` on the
        * wrapper covers this in the normal case, but if a sibling
        * element with a higher z-index briefly intercepts the cursor
        * (e.g. a hovering free zone overlapping this stack), the
        * wrapper's mouseenter can race. Attaching the same idempotent
        * handler to the surface is a cheap belt-and-braces guarantee
        * — re-firing while bloomed is a no-op (cancelBloomCollapse +
        * setIsBloomed(true) are idempotent).
        */}
      <div
        class="stack-wrapper__surface"
        onMouseEnter={handleMouseEnter}
      >
        <StackCapsule
          zones={props.zones}
          open={trayOpen()}
          hasPreview={previewZone() !== null}
          locked={stackLocked()}
          bloomed={bloomActive()}
          onMouseDown={handleCapsuleMouseDown}
          onClick={handleCapsuleClick}
          onContextMenu={handleContextMenu}
        />
        <Show when={trayOpen()}>
          <StackTray
            zones={props.zones}
            previewZoneId={previewZoneId()}
            onSelectPreview={handleSelectPreview}
            onDetach={(zoneId) => void handleDetach(zoneId)}
            onDissolve={() => void handleDissolve()}
          />
        </Show>
      </div>
      {/*
        * v8 round-12 (bloom-row-layout): the petal row.
        *
        * The user model is "stack members appear neatly under the
        * capsule" — a horizontal row of petal tiles directly below
        * (or above, when the capsule sits near the bottom edge) the
        * stack capsule, side by side. The row clamps to the viewport
        * edges with a 16 px inset, and wraps to multiple rows when
        * the petal count is too large to fit horizontally. Two
        * layers, both `position: fixed` so they escape the wrapper's
        * stacking context entirely:
        *
        *   1. `.stack-bloom-buffer` — invisible viewport-sized halo
        *      (100vw × 100vh) registered with the cursor hit-test
        *      poller so the bloom stays open as long as the cursor
        *      is anywhere in the viewport. Petals sit on top via
        *      z-index 51, so clicks land on the petal not the buffer.
        *
        *   2. `.stack-bloom__petal` (× N) — each petal is placed at
        *      its row-resolved top-left corner. The 108×96 box gives
        *      the icon room to breathe and matches the user's "real
        *      ZenCapsule tile" feel. Hover/click still opens
        *      FocusedZonePreview for that member.
        */}
      <Show when={bloomVisible() && petalRow() !== null}>
        {/*
          * v8 round-12: the bloom buffer is now a viewport-sized
          * invisible halo (full 100vw × 100vh) so multi-row wrap
          * layouts still get hit-test coverage anywhere the petals
          * may render. Pre-round-12 the buffer was a circle sized
          * around the cursor radius, which broke when the row layout
          * extended outside that circle. The buffer's pointer-events
          * are still `auto` so cursor-anywhere-near-petals keeps the
          * bloom open via the registered hit-test entry; clicks pass
          * through to the petals (which sit on top via z-index: 51).
          */}
        <div
          class="stack-bloom-buffer"
          aria-hidden="true"
          ref={(el) => {
            bloomBufferRef = el;
          }}
          style={{
            position: "fixed",
            left: "0px",
            top: "0px",
            width: "100vw",
            height: "100vh",
          }}
          onMouseEnter={cancelBloomCollapse}
        />
        <div
          class="stack-bloom"
          role="menu"
          aria-label={t("stackAriaLabel").replace(
            "{n}",
            String(props.zones.length),
          )}
        >
          {/*
            * v8 round-14 (overflow cap): when the stack has more than
            * MAX_VISIBLE_MEMBERS members, the visible-zone slice contains
            * MAX_VISIBLE_MEMBERS - 1 members and the final slot becomes
            * a "+N more" indicator. For ≤ MAX_VISIBLE_MEMBERS members,
            * the slice is the full member list and no indicator renders.
            * Building the slice with createMemo via visibleZones() so
            * Solid's <For> can do its standard list-diff against the
            * stable identity of each zone.
            */}
          <For each={visibleZones()}>
            {(zone, index) => {
              const petalStyle = (): Record<string, string> => {
                const row = petalRow();
                if (!row) {
                  return { display: "none" };
                }
                // v8 round-12: row solver returns TOP-LEFT corners
                // (not centres). Compute the petal's centre from its
                // top-left + size so the animation origin math (the
                // cursor → petal-centre delta from rounds 4–11) keeps
                // the same shape, with `center` now meaning "capsule
                // centre" (the drop-from anchor) rather than cursor.
                const topLeft = row.centers[index()];
                if (!topLeft) {
                  return { display: "none" };
                }
                // v8 round-14: petal box dims come from the bucket
                // picker — small stacks keep the round-12 108×96
                // dimensions, large stacks shrink in three steps so
                // 16-member stacks don't dominate the viewport.
                const size = petalSize();
                const petal = {
                  x: topLeft.x + size.width / 2,
                  y: topLeft.y + size.height / 2,
                };
                const center = dropFromCenter() ?? petal;
                // v8 round-12 (bloom-elegant-anim, kept from round-11):
                // the entry/exit keyframes interpolate `transform:
                // translate(...)` between an "origin vector" (capsule
                // centre relative to the petal's resting centre) and
                // zero. At t=0 the petal is visually AT the capsule
                // centre and scale(0.4); at t=1 it sits at left/top
                // (resolved row position) with scale(1). This yields a
                // "petals drop out of the capsule" entry that matches
                // the user model: the row layout makes petals look
                // like they're emerging FROM the stack, not flying in
                // from the cursor.
                const dx = center.x - petal.x;
                const dy = center.y - petal.y;
                // Inject the member zone's accent color so the
                // petal's hover/active ring picks up that zone's
                // identity color instead of a generic blue.
                const accent = zone.accent_color ?? null;
                // v8 round-14 (stagger cap): pass the visible-petal
                // count to CSS via --bloom-petal-count so the
                // animation-delay calc adapts to the visible count
                // rather than the (potentially much larger) full
                // member count.
                const style: Record<string, string> = {
                  "--petal-index": String(index()),
                  "--bloom-petal-count": String(visiblePetalCount()),
                  "--bloom-origin-x": `${dx}px`,
                  "--bloom-origin-y": `${dy}px`,
                  "--bloom-to-x": "0px",
                  "--bloom-to-y": "0px",
                  // v8 round-14: icon-size custom prop so CSS can
                  // size the icon halo from the same size bucket as
                  // the tile. The petal box width/height come straight
                  // from inline `width`/`height` below — there is no
                  // CSS rule that consumes a `--petal-w` / `--petal-h`
                  // variable, so we don't emit those (they would be
                  // dead inline declarations).
                  "--petal-icon-size": `${size.iconSize}px`,
                  position: "fixed",
                  left: `${topLeft.x}px`,
                  top: `${topLeft.y}px`,
                  width: `${size.width}px`,
                  height: `${size.height}px`,
                };
                if (accent) {
                  style["--zone-accent"] = accent;
                }
                return style;
              };
              return (
                <button
                  type="button"
                  role="menuitem"
                  // v8 round-13 (no-auto-open): the active modifier now
                  // keys off `activePetalId()`, NOT `previewZoneId()`.
                  // Round-12 conflated the two — the petal class list
                  // tracked previewZoneId, which round-13 made
                  // intentionally lazy (hover-intent gated). Without
                  // this rename the breathing pulse + soft ring would
                  // also wait 150 ms after hover, which feels sluggish
                  // (the user expects immediate visual feedback that
                  // their cursor is on a petal). activePetalId flips
                  // synchronously on petal mouseenter so the petal
                  // confirms focus the moment the cursor lands on it.
                  class={`stack-bloom__petal ${
                    activePetalId() === zone.id
                      ? "stack-bloom__petal--active"
                      : ""
                  } ${bloomLeaving() ? "stack-bloom__petal--leaving" : ""}`}
                  style={petalStyle()}
                  ref={(el) => {
                    if (el) {
                      petalRefs.set(zone.id, el);
                      // v8 round-14 (will-change cleanup): release
                      // the GPU layer hint after the entry animation
                      // completes. We DOM-listen for animationend
                      // and flip will-change to auto so subsequent
                      // hover/leave transforms don't keep the layer
                      // alive needlessly. Reduced-motion users see
                      // a fade-only path with no transform animation
                      // — the listener still fires once for the
                      // single transition end, so cleanup happens
                      // either way.
                      const onAnimEnd = () => {
                        el.style.willChange = "auto";
                      };
                      el.addEventListener("animationend", onAnimEnd, {
                        once: true,
                      });
                    } else {
                      petalRefs.delete(zone.id);
                    }
                  }}
                  title={zone.alias ?? zone.name}
                  onMouseEnter={() => handlePetalEnter(zone.id)}
                  onMouseLeave={() => handlePetalLeave(zone.id)}
                  onClick={(e) => handlePetalClick(e, zone.id)}
                  onMouseDown={(e) => e.stopPropagation()}
                >
                  <span class="stack-bloom__petal-icon">
                    <ZoneIcon
                      icon={zone.icon}
                      size={Math.round(petalSize().iconSize * 0.6)}
                    />
                  </span>
                  <span class="stack-bloom__petal-name">
                    {zone.alias ?? zone.name}
                  </span>
                </button>
              );
            }}
          </For>
          {/*
            * v8 round-14 (overflow indicator): final slot when the
            * stack overflows MAX_VISIBLE_MEMBERS. Renders a "+N more"
            * tile sitting at the last row position (visiblePetalCount
            * - 1 → the index reserved by the row solver). Clicking is
            * a no-op for now (deferred to a future round). The tile
            * does NOT participate in the activePetalId / previewZoneId
            * state — it's purely an informational marker.
            */}
          <Show when={isOverflowing()}>
            <div
              class="stack-bloom__petal stack-bloom__petal--overflow"
              role="presentation"
              aria-label={`+${overflowCount()} more`}
              title={`+${overflowCount()} more`}
              style={(() => {
                const row = petalRow();
                if (!row) return { display: "none" };
                const lastIdx = visiblePetalCount() - 1;
                const topLeft = row.centers[lastIdx];
                if (!topLeft) return { display: "none" };
                const size = petalSize();
                return {
                  "--petal-index": String(lastIdx),
                  "--bloom-petal-count": String(visiblePetalCount()),
                  "--bloom-origin-x": "0px",
                  "--bloom-origin-y": "0px",
                  "--bloom-to-x": "0px",
                  "--bloom-to-y": "0px",
                  "--petal-icon-size": `${size.iconSize}px`,
                  position: "fixed",
                  left: `${topLeft.x}px`,
                  top: `${topLeft.y}px`,
                  width: `${size.width}px`,
                  height: `${size.height}px`,
                };
              })()}
            >
              <span class="stack-bloom__petal-name">
                +{overflowCount()}
              </span>
            </div>
          </Show>
        </div>
      </Show>
      <Show when={previewZone()}>
        {(zone) => (
          <FocusedZonePreview
            zone={zone()}
            horizontal={previewAnchor().horizontal}
            vertical={previewAnchor().vertical}
            anchorRect={previewAnchorRect() ?? undefined}
            onClose={() => {
              setPreviewZoneId(null);
              // v8 round-13: explicit close from the preview itself
              // (e.g. ESC key, close button) drops the sticky flag too
              // so the preview doesn't reopen on the next petal hover.
              setPreviewSticky(false);
            }}
            onMouseEnter={cancelBloomCollapse}
          />
        )}
      </Show>
      <Show when={contextMenuOpen()}>
        {/*
          * v9 (Issue X fix): the right-click context menu now exposes:
          *   1. Per-member "Detach" rows (one per stack member) so the
          *      user can pull a single zone out without navigating to
          *      the click-mode tray. Audit item #3 — `handleDetach` was
          *      previously only reachable via the StackTray's per-row
          *      action button, which is hidden in the default hover/
          *      bloom flow.
          *   2. The "Dissolve" row (terminal action, danger styling).
          *
          * Each row commits on `onMouseDown` (capturing the pointer
          * before any spurious blur/focus change can cancel the click
          * → effectively belt-and-braces robustness against the
          * "menu opens but click does nothing" failure mode). We also
          * `e.stopPropagation()` so the document-level mousedown
          * listener (`handleOutsidePointer`) sees the menu as inside
          * its own scope and doesn't race the close.
          */}
        <div
          class="stack-context-menu"
          role="menu"
          style={{
            left: `${contextMenuOpen()!.x}px`,
            top: `${contextMenuOpen()!.y}px`,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <For each={props.zones}>
            {(member) => (
              <button
                class="stack-context-menu__item"
                role="menuitem"
                onMouseDown={(e) => {
                  // mousedown commit: synchronously fire the action so
                  // a spurious focus change between mousedown and click
                  // cannot eat the operation.
                  e.preventDefault();
                  e.stopPropagation();
                  void handleDetach(member.id);
                }}
              >
                {`${t("stackDetachMember")}: ${member.alias ?? member.name}`}
              </button>
            )}
          </For>
          <button
            class="stack-context-menu__item stack-context-menu__item--danger"
            role="menuitem"
            onMouseDown={(e) => {
              // mousedown commit: see comment on detach buttons above.
              e.preventDefault();
              e.stopPropagation();
              void handleDissolve();
            }}
          >
            {t("stackDissolve")}
          </button>
        </div>
      </Show>
    </div>
  );
};

export default StackWrapper;
