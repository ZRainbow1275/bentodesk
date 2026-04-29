import { Component, onCleanup, onMount } from "solid-js";
import type { BentoZone } from "../../types/zone";
import BentoPanel from "./BentoPanel";
import { registerZoneElement, unregisterZoneElement } from "../../services/hitTest";

/**
 * v8 round-6 (Bug Bloom-Real-Fix): the preview can now anchor to a
 * specific viewport rect instead of always sitting `position: absolute`
 * adjacent to the wrapper. When `anchorRect` is provided, the preview
 * uses `position: fixed` and computes left/top from the rect plus the
 * `horizontal` / `vertical` direction props.
 *
 * Before this round, FocusedZonePreview was always `position: absolute;
 * left: calc(100% + 16px); top: 0` — i.e. anchored to the StackWrapper's
 * own bounding rect. That made sense for the legacy StackTray-driven
 * preview (user clicks a member row in the tray, preview opens next to
 * the wrapper). It DID NOT work for the round-4 polar bloom: petals fly
 * out 110-200 px from the cursor and can land 300-500 px from the
 * original capsule. The preview would render adjacent to the original
 * capsule, completely off the user's gaze, and they reported "petals
 * appear, hover them, and nothing happens visibly". Frame-by-frame
 * analysis of their screen recording confirmed the preview WAS being
 * mounted (DOM tree changed, signal flipped), it just rendered far
 * outside their viewport focus.
 *
 * The legacy tray-driven path (StackTray's `onSelectPreview`) does NOT
 * pass anchorRect, so it falls back to the absolute behavior — no
 * regression there.
 */
interface FocusedZonePreviewProps {
  zone: BentoZone;
  /** Direction the preview grows in. `right` = preview is to the right
   *  of the anchor; `left` = preview is to the left of the anchor. */
  horizontal: "left" | "right";
  /** Direction the preview grows in. `top` = preview's top edge aligns
   *  with the anchor; `bottom` = preview's bottom edge aligns with it. */
  vertical: "top" | "bottom";
  /** Optional viewport-fixed anchor rect (the hovered bloom petal's
   *  getBoundingClientRect output). When set, preview switches to
   *  `position: fixed` and computes its position from this rect.
   *  When omitted, preview keeps the legacy `position: absolute`
   *  behavior anchored to the StackWrapper. */
  anchorRect?: { left: number; top: number; right: number; bottom: number; width: number; height: number };
  onClose: () => void;
  /** v8 round-7 (D4): callback fired on mouseenter so the parent
   *  StackWrapper can cancel its pending bloom-collapse timer. The
   *  preview lives `position: fixed` far from the wrapper's natural
   *  rect; without this hook, briefly grazing the gap between the
   *  hovered petal and the preview could fire mouseleave on the
   *  wrapper and start the 80 ms collapse timer, causing the bloom
   *  to retract while the user is on the preview. */
  onMouseEnter?: () => void;
}

const PREVIEW_GAP_PX = 12;
const PREVIEW_VIEWPORT_MARGIN = 16;
// v8 round-10 (Issue C): when the preview floats next to a bloom petal,
// it must stay compact regardless of the underlying zone's
// expanded_size. The internal BentoPanel content scrolls inside these
// bounds. The legacy tray-driven path does NOT receive an anchorRect,
// so it keeps the original sizing.
const FLOATING_PREVIEW_MAX_W_PX = 360;
const FLOATING_PREVIEW_MAX_H_PX = 420;

const FocusedZonePreview: Component<FocusedZonePreviewProps> = (props) => {
  let elementRef: HTMLDivElement | undefined;

  const isFloating = (): boolean => props.anchorRect !== undefined;

  const viewportWidth = () =>
    typeof window !== "undefined" ? window.innerWidth : 1440;
  const viewportHeight = () =>
    typeof window !== "undefined" ? window.innerHeight : 900;

  const widthPx = (): number => {
    const value = props.zone.expanded_size.w_percent;
    const raw = value > 0 ? (value / 100) * viewportWidth() : 360;
    return isFloating() ? Math.min(raw, FLOATING_PREVIEW_MAX_W_PX) : raw;
  };

  const heightPx = (): number => {
    const value = props.zone.expanded_size.h_percent;
    const raw = value > 0 ? (value / 100) * viewportHeight() : 420;
    return isFloating() ? Math.min(raw, FLOATING_PREVIEW_MAX_H_PX) : raw;
  };

  const debugPreview = (label: string, extra: Record<string, unknown> = {}): void => {
    if (typeof window === "undefined") return;
    const flag = (window as unknown as { __bento_debug_preview?: boolean })
      .__bento_debug_preview;
    if (!flag) return;
    // eslint-disable-next-line no-console
    console.log(`[preview:${props.zone.id}] ${label}`, {
      zoneName: props.zone.name,
      hasAnchorRect: props.anchorRect !== undefined,
      horizontal: props.horizontal,
      vertical: props.vertical,
      anchorRect: props.anchorRect,
      width: widthPx(),
      height: heightPx(),
      ...extra,
    });
  };

  /**
   * When pinned to a petal: position the preview adjacent to the petal
   * rect, clamping to the viewport so it never clips off-screen. The
   * `horizontal`/`vertical` props are interpreted as the *grow direction*
   * — `right` means preview's left edge sits to the right of the petal.
   */
  const fixedStyle = (rect: NonNullable<FocusedZonePreviewProps["anchorRect"]>): Record<string, string> => {
    const w = widthPx();
    const h = heightPx();
    const vw = viewportWidth();
    const vh = viewportHeight();

    let left: number;
    if (props.horizontal === "right") {
      left = rect.right + PREVIEW_GAP_PX;
      // If pushing right would clip, fall back to fitting next to the
      // petal on the OTHER side. This preserves the user's "preview
      // sits next to the petal I'm hovering" mental model even at the
      // viewport edge.
      if (left + w + PREVIEW_VIEWPORT_MARGIN > vw) {
        left = rect.left - PREVIEW_GAP_PX - w;
      }
    } else {
      left = rect.left - PREVIEW_GAP_PX - w;
      if (left < PREVIEW_VIEWPORT_MARGIN) {
        left = rect.right + PREVIEW_GAP_PX;
      }
    }
    // Final viewport clamp — guarantees the preview never escapes the
    // viewport even when the petal sits in a corner.
    left = Math.max(
      PREVIEW_VIEWPORT_MARGIN,
      Math.min(vw - w - PREVIEW_VIEWPORT_MARGIN, left),
    );

    let top: number;
    if (props.vertical === "top") {
      // Preview top edge aligned with petal top, but clamped so the
      // preview's bottom doesn't clip below the viewport.
      top = rect.top;
      if (top + h + PREVIEW_VIEWPORT_MARGIN > vh) {
        top = rect.bottom - h;
      }
    } else {
      // Preview bottom aligned with petal bottom.
      top = rect.bottom - h;
      if (top < PREVIEW_VIEWPORT_MARGIN) {
        top = rect.top;
      }
    }
    top = Math.max(
      PREVIEW_VIEWPORT_MARGIN,
      Math.min(vh - h - PREVIEW_VIEWPORT_MARGIN, top),
    );

    return {
      position: "fixed",
      left: `${left}px`,
      top: `${top}px`,
      width: `${w}px`,
      height: `${h}px`,
      // Sit above the bloom petals (z=51) and the buffer halo (z=49)
      // so the preview is always the front-most layer once it appears.
      "z-index": "60",
    };
  };

  const absoluteStyle = (): Record<string, string> => ({
    width: `${widthPx()}px`,
    height: `${heightPx()}px`,
  });

  const computedStyle = (): Record<string, string> => {
    const rect = props.anchorRect;
    return rect ? fixedStyle(rect) : absoluteStyle();
  };

  // v8 round-6: register the preview element with the hit-test poller
  // when it floats free (anchorRect-driven). Petals send the cursor
  // into a viewport-fixed rect that lives OUTSIDE the StackWrapper's
  // natural bounding rect; without this registration, the moment the
  // user moves the cursor from a petal onto the preview, the poller's
  // hit-test only sees the wrapper's rect and the state machine drops
  // to PASSTHROUGH — clicks on items inside the preview silently fall
  // through to the desktop. Mirrors the bloom buffer + petal
  // registration in StackWrapper.tsx (v8 round-5).
  //
  // For the legacy tray-driven path (no anchorRect), the preview lives
  // INSIDE the wrapper's stacking + hit-test context, so the wrapper's
  // own registration covers it; we skip the extra register/unregister
  // pair to avoid duplicating identity in the singleton zoneElements
  // map.
  onMount(() => {
    debugPreview("mount");
    if (props.anchorRect && elementRef) {
      registerZoneElement(elementRef);
      debugPreview("hit-test:registered");
    }
  });

  onCleanup(() => {
    if (elementRef) {
      unregisterZoneElement(elementRef);
    }
    debugPreview("cleanup");
  });

  return (
    <div
      ref={elementRef}
      class={`stack-focused-preview stack-focused-preview--${props.horizontal} stack-focused-preview--${props.vertical}${props.anchorRect ? " stack-focused-preview--floating" : ""}`}
      style={computedStyle()}
      onMouseDown={(e) => e.stopPropagation()}
      onMouseEnter={() => props.onMouseEnter?.()}
    >
      <BentoPanel
        zone={props.zone}
        onHeaderDragStart={() => {}}
        onClose={props.onClose}
      />
    </div>
  );
};

export default FocusedZonePreview;
