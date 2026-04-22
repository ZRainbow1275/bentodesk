/**
 * DebugOverlay — Development/diagnostic overlay for D1 hit-test visualization.
 *
 * Shows per-zone hit rectangle (including inflate), anchor snapshot, and
 * expand state in real time. Opt-in via `settings.debug_overlay`.
 *
 * Render pipeline:
 *   - Reads registered zone elements from the DOM via `[data-zone-id]`
 *   - Computes inflated rect mirroring hitTest.ts logic
 *   - Updates on requestAnimationFrame while visible
 */
import { Component, For, Show, createSignal, onCleanup, onMount } from "solid-js";
import { getDebugOverlayEnabled } from "../../stores/settings";
import { zonesStore } from "../../stores/zones";
import { isZoneExpanded } from "../../stores/ui";
import { computeInflateForPosition, getCapsuleBoxPx } from "../../services/hitTest";
import "./DebugOverlay.css";

interface HitRect {
  zoneId: string;
  name: string;
  left: number;
  top: number;
  width: number;
  height: number;
  expanded: boolean;
  inflate: { top: number; right: number; bottom: number; left: number };
  anchorX: string;
  anchorY: string;
}

const DebugOverlay: Component = () => {
  const [rects, setRects] = createSignal<HitRect[]>([]);
  let rafId: number | null = null;

  const tick = () => {
    if (!getDebugOverlayEnabled()) {
      rafId = requestAnimationFrame(tick);
      return;
    }
    const next: HitRect[] = [];
    for (const zone of zonesStore.zones) {
      const el = document.querySelector<HTMLElement>(
        `[data-zone-id="${zone.id}"]`
      );
      if (!el) continue;
      const rect = el.getBoundingClientRect();
      const raw = computeInflateForPosition(zone.position, {
        kind: zone.stack_id ? "stack" : "zone",
        boxPx: getCapsuleBoxPx(zone.capsule_shape, zone.capsule_size),
      });
      const inflate = {
        top: raw.top ?? 0,
        right: raw.right ?? 0,
        bottom: raw.bottom ?? 0,
        left: raw.left ?? 0,
      };
      const cs = getComputedStyle(el);
      next.push({
        zoneId: zone.id,
        name: zone.alias ?? zone.name,
        left: rect.left - inflate.left,
        top: rect.top - inflate.top,
        width: rect.width + inflate.left + inflate.right,
        height: rect.height + inflate.top + inflate.bottom,
        expanded: isZoneExpanded(zone.id),
        inflate,
        anchorX: cs.getPropertyValue("--origin-x").trim() || "left",
        anchorY: cs.getPropertyValue("--origin-y").trim() || "top",
      });
    }
    setRects(next);
    rafId = requestAnimationFrame(tick);
  };

  onMount(() => {
    rafId = requestAnimationFrame(tick);
  });

  onCleanup(() => {
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
  });

  return (
    <Show when={getDebugOverlayEnabled()}>
      <div class="debug-overlay" aria-hidden="true">
        <For each={rects()}>
          {(r) => (
            <div
              class={`debug-overlay__rect ${r.expanded ? "debug-overlay__rect--expanded" : ""}`}
              style={{
                left: `${r.left}px`,
                top: `${r.top}px`,
                width: `${r.width}px`,
                height: `${r.height}px`,
              }}
            >
              <div class="debug-overlay__label">
                {r.name} · {r.expanded ? "EXP" : "ZEN"} · {r.anchorX}-{r.anchorY}
                <Show when={r.inflate.top || r.inflate.right || r.inflate.bottom || r.inflate.left}>
                  <span class="debug-overlay__label-inflate">
                    {` +`}
                    {r.inflate.top ? `T${r.inflate.top} ` : ""}
                    {r.inflate.right ? `R${r.inflate.right} ` : ""}
                    {r.inflate.bottom ? `B${r.inflate.bottom} ` : ""}
                    {r.inflate.left ? `L${r.inflate.left}` : ""}
                  </span>
                </Show>
              </div>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
};

export default DebugOverlay;
