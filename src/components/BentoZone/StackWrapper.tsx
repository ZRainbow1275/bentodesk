/**
 * StackWrapper — D2 Zone Stack container.
 *
 * Visual model:
 *   - Base pile: each layer translated by `4px * depth + rotate(-1deg * depth)`
 *     so stacks look like macOS Dock-style cards.
 *   - On hover-top: children spread into a fan (radial translateY + rotate),
 *     revealing all members at once. Leaving the top collapses the fan.
 *   - Separation gestures:
 *       * double-click top zone        → pop top-of-stack to a new free slot
 *       * context-menu "解散堆栈"       → dissolve the whole stack
 *       * Shift+mousedown on stack     → translate all members together
 *       * drag a member out (distance > 40px) → pop that member
 *
 * accessibility:
 *   `role="group"` + `aria-label="Zone stack of N items"` on the wrapper,
 *   and the top-most capsule stays keyboard-focusable via its usual tabIndex.
 */
import {
  Component,
  For,
  createSignal,
  createMemo,
  onCleanup,
  onMount,
} from "solid-js";
import type { BentoZone as BentoZoneType } from "../../types/zone";
import BentoZone from "./BentoZone";
import {
  stackZonesAction,
  unstackZonesAction,
} from "../../stores/stacks";
import { updateZone } from "../../stores/zones";
import { getViewportSize } from "../../stores/ui";
import { acquireDragLock } from "../../services/hitTest";
import { t } from "../../i18n";
import "./StackWrapper.css";

interface StackWrapperProps {
  stackId: string;
  zones: BentoZoneType[]; // sorted bottom → top by stack_order
}

const FAN_RADIUS_PX = 72;
const SEPARATE_DISTANCE_PX = 40;

// Clamp a percentage so the stack doesn't leave the viewport entirely.
// BentoZone's single-capsule drag uses a per-element dynamic max (100 - rect/vp*100)
// but Shift+stack-drag applies a single dx/dy to every member, whose capsule sizes
// may differ. 4% is a conservative constant that keeps any reasonably-sized capsule
// on screen without per-member rect lookups — acceptable for this MVP gesture.
const clampPct = (v: number): number => Math.max(0, Math.min(100 - 4, v));

const StackWrapper: Component<StackWrapperProps> = (props) => {
  const [fanOut, setFanOut] = createSignal(false);
  const [contextMenuOpen, setContextMenuOpen] = createSignal<{
    x: number;
    y: number;
  } | null>(null);

  let wrapperRef: HTMLDivElement | undefined;

  const count = createMemo(() => props.zones.length);

  // Anchor: the stack's viewport position/size follows the bottom-most
  // member (smallest stack_order). `props.zones` is already sorted bottom→top.
  const baseZone = createMemo(() => props.zones[0]);

  const wrapperStyle = (): Record<string, string> => {
    const z = baseZone();
    if (!z) return {};
    const vp = getViewportSize();
    const w = z.expanded_size.w_percent > 0
      ? `${(z.expanded_size.w_percent / 100) * vp.width}px`
      : "360px";
    const h = z.expanded_size.h_percent > 0
      ? `${(z.expanded_size.h_percent / 100) * vp.height}px`
      : "420px";
    return {
      "--stack-x": `${z.position.x_percent}%`,
      "--stack-y": `${z.position.y_percent}%`,
      "--stack-w": w,
      "--stack-h": h,
    };
  };

  // Fan-out angle distribution: evenly spread from -60° to +60° around top.
  const angleFor = (idx: number): number => {
    const total = count();
    if (total <= 1) return 0;
    const SPREAD = 120; // degrees, -60 to +60
    const step = SPREAD / (total - 1);
    return -SPREAD / 2 + step * idx;
  };

  // ── Pop top-of-stack: dblclick on top layer. Keep remaining members as a
  // stack when possible; dissolve entirely only when the residual pile
  // would be < 2 members.
  const handleTopDblClick = async (): Promise<void> => {
    const members = props.zones; // sorted bottom→top
    if (members.length === 0) return;
    if (members.length <= 2) {
      // 2 members → no meaningful residual stack, just dissolve.
      await unstackZonesAction(props.stackId);
      return;
    }
    // ≥3 members: unstack all, then re-stack everyone except the top.
    // The top (members[last]) is left free-standing.
    const remainingIds = members.slice(0, -1).map((z) => z.id);
    await unstackZonesAction(props.stackId);
    if (remainingIds.length >= 2) {
      await stackZonesAction(remainingIds);
    }
  };

  const handleContextMenu = (e: MouseEvent): void => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenuOpen({ x: e.clientX, y: e.clientY });
  };

  const handleDissolve = async (): Promise<void> => {
    setContextMenuOpen(null);
    await unstackZonesAction(props.stackId);
  };

  // ── Shift+drag moves the whole stack in concert. We persist the final
  // positions in a single batch on mouseup; the visual during-drag
  // feedback is deferred to keep this MVP narrow (the cursor snaps the
  // stack to its landing spot rather than previewing a ghost).
  const handleWrapperMouseDown = (e: MouseEvent): void => {
    if (!e.shiftKey || e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();

    const releaseDrag = acquireDragLock();
    const startX = e.clientX;
    const startY = e.clientY;
    const startPositions = props.zones.map((z) => ({
      id: z.id,
      x: z.position.x_percent,
      y: z.position.y_percent,
    }));

    let lastDx = 0;
    let lastDy = 0;

    const onMouseMove = (ev: MouseEvent): void => {
      lastDx = ((ev.clientX - startX) / window.innerWidth) * 100;
      lastDy = ((ev.clientY - startY) / window.innerHeight) * 100;
    };

    const onMouseUp = async (): Promise<void> => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      try {
        await Promise.all(
          startPositions.map((sp) =>
            updateZone(sp.id, {
              position: {
                x_percent: clampPct(sp.x + lastDx),
                y_percent: clampPct(sp.y + lastDy),
              },
            }),
          ),
        );
      } finally {
        releaseDrag();
      }
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  };

  const handleMouseEnter = () => {
    setFanOut(true);
  };
  const handleMouseLeave = () => {
    setFanOut(false);
  };

  // Reorder on keyboard: Esc collapses fan, Enter = dblclick-equivalent.
  const handleKeyDown = (e: KeyboardEvent): void => {
    if (e.key === "Escape") setFanOut(false);
  };

  // Close context menu on outside click.
  const closeCtxOnOutside = (e: MouseEvent) => {
    if (!contextMenuOpen()) return;
    const inside = (e.target as HTMLElement)?.closest?.(".stack-context-menu");
    if (!inside) setContextMenuOpen(null);
  };

  onMount(() => {
    document.addEventListener("mousedown", closeCtxOnOutside);
  });
  onCleanup(() => {
    document.removeEventListener("mousedown", closeCtxOnOutside);
  });

  // ── Drag-out-of-stack detection: any child zone dragged more than
  // SEPARATE_DISTANCE_PX triggers unstack. MVP ships with dblclick +
  // context-menu; elastic detection is a follow-up so the drag ownership
  // stays with BentoZone and doesn't fight it.

  return (
    <div
      ref={wrapperRef}
      class={`stack-wrapper ${fanOut() ? "stack-wrapper--fanned" : ""}`}
      style={wrapperStyle()}
      role="group"
      aria-label={t("stackAriaLabel").replace("{n}", String(count()))}
      tabIndex={0}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      onMouseDown={handleWrapperMouseDown}
      onKeyDown={handleKeyDown}
      onContextMenu={handleContextMenu}
    >
      <For each={props.zones}>
        {(zone, idx) => {
          const depth = () => (count() - 1) - idx();
          const isTop = () => idx() === count() - 1;
          const fanAngle = () => (fanOut() ? angleFor(idx()) : 0);
          const fanDist = () => (fanOut() && !isTop() ? FAN_RADIUS_PX : 0);
          const style = () => ({
            "--stack-depth": String(depth()),
            "--fan-angle": `${fanAngle()}deg`,
            "--fan-dist": `${fanDist()}px`,
          });
          return (
            <div
              class="stack-wrapper__layer"
              style={style() as Record<string, string>}
              onDblClick={(e: MouseEvent) => {
                if (isTop()) {
                  e.stopPropagation();
                  void handleTopDblClick();
                }
              }}
            >
              <BentoZone zone={zone} />
            </div>
          );
        }}
      </For>
      {contextMenuOpen() && (
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
      )}
    </div>
  );
};

export default StackWrapper;
