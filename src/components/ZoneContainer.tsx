/**
 * ZoneContainer — Full viewport container for all BentoZones.
 * pointer-events: none on container so clicks pass through to desktop.
 * Each BentoZone sets pointer-events: auto on itself.
 *
 * D2: zones with a shared `stack_id` are rendered inside a `StackWrapper`
 * so they appear as a visual pile (macOS Dock-style). Free-standing zones
 * render through the original flow.
 */
import { Component, For, Show, createMemo, createSignal, onCleanup } from "solid-js";
import { zonesStore } from "../stores/zones";
import { stackMap } from "../stores/stacks";
import BentoZone from "./BentoZone/BentoZone";
import StackWrapper from "./BentoZone/StackWrapper";
import {
  canActivateMarquee,
  startMarquee,
  updateMarquee,
  endMarquee,
  cancelMarquee,
  marquee,
  marqueeRect,
} from "../services/marquee";
import {
  replaceWithMarqueeSelection,
  unionMarqueeSelection,
  clearMultiSelection,
} from "../stores/selection";

const ZoneContainer: Component = () => {
  // Split zones into (free-standing) + (grouped by stack_id). The stackMap
  // memo only includes stacks with >= 2 members; singleton "stacks" fall
  // through to the free-standing list.
  const freeZones = createMemo(() => {
    const stacked = new Set<string>();
    for (const arr of stackMap().values()) {
      for (const z of arr) stacked.add(z.id);
    }
    return zonesStore.zones.filter((z) => !stacked.has(z.id));
  });

  const stacks = createMemo(() => Array.from(stackMap().entries()));

  // ─── Theme C: marquee (drag-to-select) ────────────────────
  const [isDragging, setIsDragging] = createSignal(false);
  const [dragModifiers, setDragModifiers] = createSignal<{
    shift: boolean;
    ctrl: boolean;
  }>({ shift: false, ctrl: false });

  function onMarqueeMouseMove(e: MouseEvent): void {
    updateMarquee(e.clientX, e.clientY);
  }

  function onMarqueeMouseUp(): void {
    if (!isDragging()) return;
    const mods = dragModifiers();
    const { zoneIds, itemIds } = endMarquee();
    if (mods.shift || mods.ctrl) {
      unionMarqueeSelection(
        zoneIds,
        itemIds.map((x) => x.itemId)
      );
    } else if (zoneIds.length === 0 && itemIds.length === 0) {
      clearMultiSelection();
    } else {
      replaceWithMarqueeSelection(
        zoneIds,
        itemIds.map((x) => x.itemId)
      );
    }
    setIsDragging(false);
    document.removeEventListener("mousemove", onMarqueeMouseMove);
    document.removeEventListener("mouseup", onMarqueeMouseUp);
    document.removeEventListener("keydown", onMarqueeCancelKey);
  }

  function onMarqueeCancelKey(e: KeyboardEvent): void {
    if (e.key === "Escape" && isDragging()) {
      cancelMarquee();
      setIsDragging(false);
      document.removeEventListener("mousemove", onMarqueeMouseMove);
      document.removeEventListener("mouseup", onMarqueeMouseUp);
      document.removeEventListener("keydown", onMarqueeCancelKey);
    }
  }

  function onMarqueeMouseDown(e: MouseEvent): void {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest(".bento-zone")) return;
    if (!canActivateMarquee()) return;
    e.preventDefault();
    startMarquee(e.clientX, e.clientY);
    setIsDragging(true);
    setDragModifiers({ shift: e.shiftKey, ctrl: e.ctrlKey });
    document.addEventListener("mousemove", onMarqueeMouseMove);
    document.addEventListener("mouseup", onMarqueeMouseUp);
    document.addEventListener("keydown", onMarqueeCancelKey);
  }

  onCleanup(() => {
    document.removeEventListener("mousemove", onMarqueeMouseMove);
    document.removeEventListener("mouseup", onMarqueeMouseUp);
    document.removeEventListener("keydown", onMarqueeCancelKey);
  });

  return (
    <div
      style={{
        position: "fixed",
        top: "0",
        left: "0",
        width: "100vw",
        height: "100vh",
        "pointer-events": "none",
        overflow: "hidden",
      }}
    >
      <For each={freeZones()}>
        {(zone) => <BentoZone zone={zone} />}
      </For>
      <Show when={stacks().length > 0}>
        <For each={stacks()}>
          {([stackId, zones]) => (
            <StackWrapper stackId={stackId} zones={zones} />
          )}
        </For>
      </Show>
      {/**
        * Theme C marquee surface — pointer-events only when marquee is
        * allowed (zone expanded / modal open) so passthrough still wins
        * on an empty desktop.
        */}
      <div
        class="zone-container__marquee-surface"
        style={{
          position: "fixed",
          inset: 0,
          "pointer-events": canActivateMarquee() ? "auto" : "none",
          "z-index": 5,
        }}
        onMouseDown={onMarqueeMouseDown}
      />
      <Show when={marquee()}>
        {(() => {
          const box = marqueeRect();
          if (!box) return null;
          return (
            <div
              class="zone-container__marquee-box"
              style={{
                position: "fixed",
                left: `${box.left}px`,
                top: `${box.top}px`,
                width: `${box.width}px`,
                height: `${box.height}px`,
                "pointer-events": "none",
                border: "1px dashed rgba(59, 130, 246, 0.9)",
                background: "rgba(59, 130, 246, 0.08)",
                "border-radius": "2px",
                "z-index": 1000,
              }}
            />
          );
        })()}
      </Show>
    </div>
  );
};

export default ZoneContainer;
