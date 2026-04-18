/**
 * Tooltip — D3 shared portal-mounted tooltip.
 *
 * Rationale:
 *   - Native `title=` attributes are unreliable under Tauri's overlay
 *     passthrough: when `setIgnoreCursorEvents(true)` fires and the cursor
 *     crosses into the webview, the browser may never deliver the delayed
 *     hover event that shows the native tooltip.
 *   - Mounting via `<Portal>` into `document.body` keeps the tooltip outside
 *     any zone's overflow/transform context so it's never clipped by card
 *     borders or the stack-mode rotation transforms.
 *   - Every instance shares one rAF-driven positioning pass. We don't
 *     animate with JS — CSS opacity + translateY transitions keep it cheap.
 *
 * Accessibility:
 *   - The tooltip content is rendered with `role="tooltip"`.
 *   - Callers retain `aria-label={fullName}` on the trigger so screen
 *     readers always read the original, uncurated name.
 */
import {
  Component,
  JSX,
  createSignal,
  onCleanup,
  onMount,
  Show,
} from "solid-js";
import { Portal } from "solid-js/web";
import "./Tooltip.css";

interface TooltipProps {
  /** Tooltip body content — typically the untruncated name. */
  content: string;
  /** Trigger element; tooltip listens for `mouseenter/leave/focus/blur`. */
  children: JSX.Element;
  /** Delay in ms before showing after mouseenter. Default 400. */
  delay?: number;
  /** When true, suppress the tooltip (e.g. truncated text matches original). */
  disabled?: boolean;
}

const DEFAULT_DELAY = 400;

const Tooltip: Component<TooltipProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [placement, setPlacement] = createSignal<{
    left: number;
    top: number;
    direction: "above" | "below";
  }>({ left: 0, top: 0, direction: "above" });

  let triggerRef: HTMLSpanElement | undefined;
  let tooltipRef: HTMLDivElement | undefined;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const clearTimer = () => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  };

  const position = () => {
    if (!triggerRef || !tooltipRef) return;
    const r = triggerRef.getBoundingClientRect();
    const tipRect = tooltipRef.getBoundingClientRect();
    const margin = 8;
    let top = r.top - tipRect.height - margin;
    let direction: "above" | "below" = "above";
    if (top < 4) {
      top = r.bottom + margin;
      direction = "below";
    }
    let left = r.left + r.width / 2 - tipRect.width / 2;
    // Clamp to viewport so a tooltip next to a screen edge doesn't clip.
    const maxLeft = window.innerWidth - tipRect.width - 4;
    left = Math.max(4, Math.min(maxLeft, left));
    setPlacement({ left, top, direction });
  };

  const show = () => {
    if (props.disabled) return;
    clearTimer();
    const delay = props.delay ?? DEFAULT_DELAY;
    timer = setTimeout(() => {
      setOpen(true);
      // Position after the next frame so measureText has a layout.
      requestAnimationFrame(position);
    }, delay);
  };

  const hide = () => {
    clearTimer();
    setOpen(false);
  };

  onMount(() => {
    // If the trigger slot isn't a single element, fall back to the wrapper.
    // `triggerRef` is always a <span> we own; no surprise there.
    const el = triggerRef;
    if (!el) return;
    el.addEventListener("mouseenter", show);
    el.addEventListener("mouseleave", hide);
    el.addEventListener("focusin", show);
    el.addEventListener("focusout", hide);
  });

  onCleanup(() => {
    clearTimer();
    const el = triggerRef;
    if (!el) return;
    el.removeEventListener("mouseenter", show);
    el.removeEventListener("mouseleave", hide);
    el.removeEventListener("focusin", show);
    el.removeEventListener("focusout", hide);
  });

  return (
    <>
      <span ref={triggerRef} class="tooltip-trigger">
        {props.children}
      </span>
      <Show when={open() && props.content && !props.disabled}>
        <Portal mount={document.body}>
          <div
            ref={tooltipRef}
            role="tooltip"
            class={`tooltip tooltip--${placement().direction}`}
            style={{
              left: `${placement().left}px`,
              top: `${placement().top}px`,
            }}
          >
            {props.content}
          </div>
        </Portal>
      </Show>
    </>
  );
};

export default Tooltip;
