/**
 * JSON Theme Provider — bridges the backend JSON theme plugin system
 * with the frontend CSS custom property application.
 *
 * Usage:
 *   import { loadAndApplyJsonTheme, applyJsonTheme } from "../themes/provider";
 *
 * This module loads a JsonTheme from the Tauri backend and translates
 * its structured fields (colors, capsule, animation, glassmorphism)
 * into CSS custom properties on :root.
 */
import type { JsonTheme } from "./types";
import { getActiveTheme, setActiveTheme } from "../services/ipc";

// ─── CSS Variable Mapping ───────────────────────────────────

/**
 * Apply a JsonTheme's properties as CSS custom properties on :root.
 *
 * This supplements (does not replace) the existing BentoTheme CSS
 * variable system. JSON theme variables use the `--jt-` prefix to
 * avoid collision with the existing `--surface-*`, `--text-*` vars.
 */
export function applyJsonTheme(theme: JsonTheme): void {
  const root = document.documentElement;

  // Colors
  root.style.setProperty("--jt-accent", theme.colors.accent);
  root.style.setProperty("--jt-background", theme.colors.background);
  root.style.setProperty("--jt-text", theme.colors.text);
  root.style.setProperty("--jt-border", theme.colors.border);

  // Capsule
  root.style.setProperty("--jt-capsule-shape", theme.capsule.shape);
  root.style.setProperty("--jt-capsule-size", theme.capsule.size);
  root.style.setProperty("--jt-capsule-blur-radius", `${theme.capsule.blur_radius}px`);

  // Animation
  root.style.setProperty("--jt-expand-duration", `${theme.animation.expand_duration_ms}ms`);
  root.style.setProperty("--jt-collapse-duration", `${theme.animation.collapse_duration_ms}ms`);

  // Glassmorphism
  root.style.setProperty(
    "--jt-glass-backdrop",
    `blur(${theme.glassmorphism.blur}px) saturate(${theme.glassmorphism.saturation * 100}%)`
  );
  root.style.setProperty("--jt-glass-opacity", `${theme.glassmorphism.opacity}`);
  root.style.setProperty("--jt-glass-blur", `${theme.glassmorphism.blur}px`);
  root.style.setProperty("--jt-glass-saturation", `${theme.glassmorphism.saturation * 100}%`);

  // Store active theme ID as data attribute for CSS selectors
  root.setAttribute("data-json-theme", theme.id);
}

/**
 * Load the active JSON theme from the backend and apply it.
 * Call during app initialization after the Tauri runtime is ready.
 */
export async function loadAndApplyJsonTheme(): Promise<JsonTheme | null> {
  try {
    const theme = await getActiveTheme();
    applyJsonTheme(theme);
    return theme;
  } catch (e) {
    console.warn("Failed to load active JSON theme:", e);
    return null;
  }
}

/**
 * Switch to a JSON theme by ID. Persists choice via backend and applies CSS.
 */
export async function switchJsonTheme(id: string): Promise<JsonTheme | null> {
  try {
    const theme = await setActiveTheme(id);
    applyJsonTheme(theme);
    return theme;
  } catch (e) {
    console.warn("Failed to switch JSON theme:", e);
    return null;
  }
}
