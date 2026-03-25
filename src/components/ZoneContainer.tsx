/**
 * ZoneContainer — Full viewport container for all BentoZones.
 * pointer-events: none on container so clicks pass through to desktop.
 * Each BentoZone sets pointer-events: auto on itself.
 */
import { Component, For } from "solid-js";
import { zonesStore } from "../stores/zones";
import BentoZone from "./BentoZone/BentoZone";

const ZoneContainer: Component = () => {
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
      <For each={zonesStore.zones}>
        {(zone) => <BentoZone zone={zone} />}
      </For>
    </div>
  );
};

export default ZoneContainer;
