/**
 * BentoDesk — Solid.js entry point.
 * Renders the App component into the #root element.
 */
/* @refresh reload */
import { render } from "solid-js/web";

// Global CSS — imported here so Vite processes and bundles them in production.
// Order matters: variables → reset → utilities → animations.
import "./styles/variables.css";
import "./styles/base.css";
import "./styles/glassmorphism.css";
import "./styles/animations.css";
import "./styles/theme-effects.css";
import "./styles/toast.css";

import App from "./App";
import MiniBarView from "./views/MiniBarView";

const root = document.getElementById("root");
if (!root) {
  throw new Error("Root element #root not found in DOM");
}

// Theme E2-c — Mini Bar webviews use the same index.html with
// `?minibar={zone_id}` in the URL. Fork at the root so mini bars skip
// the heavyweight App tree (hit-test polling, global hotkeys, viewport
// trackers — none of which make sense for a 40px floating bar).
const minibarZoneId = new URLSearchParams(window.location.search).get("minibar");

if (minibarZoneId) {
  render(() => <MiniBarView zoneId={minibarZoneId} />, root);
} else {
  render(() => <App />, root);
}
