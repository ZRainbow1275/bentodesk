import {
  Component,
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
import { acquireDragLock } from "../../services/hitTest";
import { getZoneDisplayMode } from "../../stores/settings";
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

  let wrapperRef: HTMLDivElement | undefined;

  const displayMode = () => getZoneDisplayMode();
  const baseZone = createMemo(() => props.zones[0]);
  const topZone = createMemo(() => props.zones[props.zones.length - 1]);
  const stackDisplayMode = () =>
    topZone()?.display_mode ?? baseZone()?.display_mode ?? displayMode();
  const stackLocked = () => props.zones.some((zone) => zone.locked);
  const previewZone = createMemo(() =>
    props.zones.find((zone) => zone.id === previewZoneId()) ?? null,
  );

  const wrapperStyle = (): Record<string, string> => {
    const zone = baseZone();
    const pos = dragPosition() ?? zone?.position;
    if (!pos) return {};
    return {
      left: `${pos.x_percent}%`,
      top: `${pos.y_percent}%`,
      "z-index": String((topZone()?.sort_order ?? 0) + 30),
    };
  };

  const closeTray = (): void => {
    if (stackDisplayMode() === "always") {
      setTrayOpen(true);
      return;
    }
    setTrayOpen(false);
    setPreviewZoneId(null);
  };

  const setTrayVisibility = (open: boolean): void => {
    if (!open) {
      closeTray();
      return;
    }
    setTrayOpen(true);
  };

  const updatePreviewAnchor = (): void => {
    const zone = previewZone();
    if (!wrapperRef || !zone) return;
    const rect = wrapperRef.getBoundingClientRect();
    const viewport = getViewportSize();
    const previewWidth = zone.expanded_size.w_percent > 0
      ? (zone.expanded_size.w_percent / 100) * viewport.width
      : 360;
    const previewHeight = zone.expanded_size.h_percent > 0
      ? (zone.expanded_size.h_percent / 100) * viewport.height
      : 420;
    const horizontal =
      rect.right + PREVIEW_GAP_PX + previewWidth <= viewport.width
        ? "right"
        : "left";
    const vertical =
      rect.top + previewHeight <= viewport.height || rect.bottom - previewHeight < 0
        ? "top"
        : "bottom";
    setPreviewAnchor({ horizontal, vertical });
  };

  const handleContextMenu = (e: MouseEvent): void => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenuOpen({ x: e.clientX, y: e.clientY });
  };

  const handleDissolve = async (): Promise<void> => {
    setContextMenuOpen(null);
    setPreviewZoneId(null);
    await unstackZonesAction(props.stackId);
  };

  const handleDetach = async (zoneId: string): Promise<void> => {
    if (previewZoneId() === zoneId) {
      setPreviewZoneId(null);
    }
    setContextMenuOpen(null);
    await detachZoneFromStackAction(props.stackId, zoneId);
  };

  const handleSelectPreview = (zoneId: string): void => {
    setTrayVisibility(true);
    setPreviewZoneId((prev) => (prev === zoneId ? null : zoneId));
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

        const onMouseMove = (ev: MouseEvent): void => {
          const dx = ev.clientX - startX;
          const dy = ev.clientY - startY;
          if (!moved && Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;
          moved = true;
          updateGroupZoneDrag({
            x_percent: (dx / window.innerWidth) * 100,
            y_percent: (dy / window.innerHeight) * 100,
          });
        };

        const onMouseUp = async (): Promise<void> => {
          document.removeEventListener("mousemove", onMouseMove);
          document.removeEventListener("mouseup", onMouseUp);
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

    const onMouseMove = (ev: MouseEvent): void => {
      const dx = ev.clientX - startX;
      const dy = ev.clientY - startY;
      if (!moved && Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;
      moved = true;
      const maxXPct = 100 - (capsuleSize.width / window.innerWidth) * 100;
      const maxYPct = 100 - (capsuleSize.height / window.innerHeight) * 100;
      setDragPosition({
        x_percent: clampPct(
          zone.position.x_percent + (dx / window.innerWidth) * 100,
          maxXPct,
        ),
        y_percent: clampPct(
          zone.position.y_percent + (dy / window.innerHeight) * 100,
          maxYPct,
        ),
      });
    };

    const onMouseUp = async (): Promise<void> => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
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
        releaseDrag();
      }
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  };

  const handleMouseEnter = () => {
    if (stackDisplayMode() === "hover") {
      setTrayVisibility(true);
    }
  };

  const handleMouseLeave = () => {
    if (stackDisplayMode() === "hover") {
      closeTray();
    }
  };

  const handleCapsuleClick = () => {
    if (suppressNextClick) {
      suppressNextClick = false;
      return;
    }
    if (stackDisplayMode() !== "click") return;
    setTrayVisibility(!trayOpen());
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

  onMount(() => {
    document.addEventListener("mousedown", handleOutsidePointer);
  });

  onCleanup(() => {
    document.removeEventListener("mousedown", handleOutsidePointer);
  });

  createEffect(() => {
    const mode = stackDisplayMode();
    if (mode === "always") {
      setTrayVisibility(true);
      return;
    }
    if (!trayOpen()) {
      setPreviewZoneId(null);
    }
  });

  createEffect(() => {
    trayOpen();
    previewZoneId();
    dragPosition();
    getViewportSize();
    queueMicrotask(updatePreviewAnchor);
  });

  return (
    <div
      ref={wrapperRef}
      class={`stack-wrapper ${trayOpen() ? "stack-wrapper--open" : ""} ${stackLocked() ? "stack-wrapper--locked" : ""}`}
      style={wrapperStyle()}
      role="group"
      aria-label={t("stackAriaLabel").replace("{n}", String(props.zones.length))}
      aria-expanded={trayOpen()}
      tabIndex={0}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      onKeyDown={handleKeyDown}
    >
      <div class="stack-wrapper__surface">
        <StackCapsule
          zones={props.zones}
          open={trayOpen()}
          hasPreview={previewZone() !== null}
          locked={stackLocked()}
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
      <Show when={previewZone()}>
        {(zone) => (
          <FocusedZonePreview
            zone={zone()}
            horizontal={previewAnchor().horizontal}
            vertical={previewAnchor().vertical}
            onClose={() => setPreviewZoneId(null)}
          />
        )}
      </Show>
      <Show when={contextMenuOpen()}>
        <div
          class="stack-context-menu"
          style={{
            left: `${contextMenuOpen()!.x}px`,
            top: `${contextMenuOpen()!.y}px`,
          }}
        >
          <button
            class="stack-context-menu__item stack-context-menu__item--danger"
            onClick={() => void handleDissolve()}
          >
            {t("stackDissolve")}
          </button>
        </div>
      </Show>
    </div>
  );
};

export default StackWrapper;
