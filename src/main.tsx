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

import App from "./App";

const root = document.getElementById("root");
if (!root) {
  throw new Error("Root element #root not found in DOM");
}

render(() => <App />, root);
