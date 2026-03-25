/**
 * Theme store — manages active theme, persistence, and CSS variable application.
 *
 * Usage:
 *   import { getThemeId, setTheme, getAvailableThemes, applyCurrentTheme } from "../themes";
 *
 * Themes are applied by setting CSS custom properties directly on :root,
 * replacing the previous data-theme attribute approach.
 * The active theme ID is persisted in localStorage.
 */
import { createSignal } from "solid-js";
import type { BentoTheme, ThemeId, ExportableTheme } from "./types";
import { BUILTIN_THEMES, darkTheme } from "./presets";

// Re-export types for convenient access
export type { BentoTheme, ThemeId, ExportableTheme } from "./types";
export { BUILTIN_THEMES } from "./presets";

const STORAGE_KEY_THEME = "bentodesk-theme-id";
const STORAGE_KEY_CUSTOM = "bentodesk-custom-themes";

// ─── Custom theme registry ──────────────────────────────────

/** Load custom themes from localStorage. */
function loadCustomThemes(): BentoTheme[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY_CUSTOM);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as ExportableTheme[];
    return parsed.map((t) => ({ ...t, is_builtin: false }));
  } catch {
    return [];
  }
}

/** Persist custom themes to localStorage. */
function saveCustomThemes(themes: BentoTheme[]): void {
  const exportable: ExportableTheme[] = themes.map(({ is_builtin: _, ...rest }) => rest);
  localStorage.setItem(STORAGE_KEY_CUSTOM, JSON.stringify(exportable));
}

const [customThemes, setCustomThemes] = createSignal<BentoTheme[]>(loadCustomThemes());

// ─── Active theme ───────────────────────────────────────────

const [activeThemeId, setActiveThemeId] = createSignal<ThemeId>(
  localStorage.getItem(STORAGE_KEY_THEME) ?? "dark"
);

// ─── Theme resolution ───────────────────────────────────────

/** Find a theme by ID across built-in and custom themes. */
function resolveTheme(id: ThemeId): BentoTheme {
  const builtin = BUILTIN_THEMES.find((t) => t.id === id);
  if (builtin) return builtin;

  const custom = customThemes().find((t) => t.id === id);
  if (custom) return custom;

  // Fallback to dark theme if ID not found
  return darkTheme;
}

// ─── CSS variable application ───────────────────────────────

/** Map from BentoTheme field names to CSS variable names. */
const THEME_TO_CSS: ReadonlyArray<[keyof BentoTheme, string]> = [
  ["surface_zen", "--surface-zen"],
  ["surface_expanded", "--surface-expanded"],
  ["surface_hover", "--surface-hover"],
  ["surface_active", "--surface-active"],
  ["surface_subtle", "--surface-subtle"],
  ["border_zen", "--border-zen"],
  ["border_expanded", "--border-expanded"],
  ["border_hover", "--border-hover"],
  ["text_primary", "--text-primary"],
  ["text_secondary", "--text-secondary"],
  ["text_muted", "--text-muted"],
  ["accent_blue", "--accent-blue"],
  ["accent_purple", "--accent-purple"],
  ["accent_green", "--accent-green"],
  ["accent_orange", "--accent-orange"],
  ["accent_pink", "--accent-pink"],
  ["accent_red", "--accent-red"],
  ["shadow_zen", "--shadow-zen"],
  ["shadow_expanded", "--shadow-expanded"],
  ["shadow_item_hover", "--shadow-item-hover"],
  ["blur_zen", "--blur-zen"],
  ["blur_expanded", "--blur-expanded"],
  ["badge_bg", "--badge-bg"],
  ["radius_capsule", "--radius-capsule"],
  ["radius_expanded", "--radius-expanded"],
  ["radius_card", "--radius-card"],
  ["radius_badge", "--radius-badge"],
];

/**
 * Apply a theme's CSS variables to :root.
 * Also sets data-theme attribute for any CSS that still depends on it.
 */
function applyCssVariables(theme: BentoTheme): void {
  const root = document.documentElement;

  for (const [field, cssVar] of THEME_TO_CSS) {
    const value = theme[field];
    if (typeof value === "string") {
      root.style.setProperty(cssVar, value);
    }
  }

  // Keep data-theme for backward compatibility with any direct CSS selectors
  // Determine if the theme is "light" for data-theme attribute
  const isLight = theme.id === "light";
  root.setAttribute("data-theme", isLight ? "light" : "dark");
}

// ─── Public API ─────────────────────────────────────────────

/**
 * Get the currently active theme ID (reactive).
 */
export function getThemeId(): ThemeId {
  return activeThemeId();
}

/**
 * Get the currently active theme object (reactive).
 */
export function getTheme(): BentoTheme {
  return resolveTheme(activeThemeId());
}

/**
 * Get all available themes (built-in + custom), in display order (reactive).
 */
export function getAvailableThemes(): readonly BentoTheme[] {
  return [...BUILTIN_THEMES, ...customThemes()];
}

/**
 * Switch to a theme by ID. Persists choice and applies CSS variables immediately.
 */
export function setTheme(id: ThemeId): void {
  const theme = resolveTheme(id);
  setActiveThemeId(theme.id);
  localStorage.setItem(STORAGE_KEY_THEME, theme.id);
  applyCssVariables(theme);
}

/**
 * Apply the currently saved theme on startup.
 * Call this once during app initialization.
 */
export function applyCurrentTheme(): void {
  const id = activeThemeId();
  const theme = resolveTheme(id);
  applyCssVariables(theme);
}

/**
 * Register a custom theme. Persists to localStorage.
 * If a custom theme with the same ID already exists, it is replaced.
 */
export function registerCustomTheme(theme: BentoTheme): void {
  const withBuiltin: BentoTheme = { ...theme, is_builtin: false };
  setCustomThemes((prev) => {
    const filtered = prev.filter((t) => t.id !== theme.id);
    const next = [...filtered, withBuiltin];
    saveCustomThemes(next);
    return next;
  });
}

/**
 * Remove a custom theme by ID. Built-in themes cannot be removed.
 * If the removed theme is currently active, falls back to "dark".
 */
export function removeCustomTheme(id: ThemeId): boolean {
  const isBuiltin = BUILTIN_THEMES.some((t) => t.id === id);
  if (isBuiltin) return false;

  setCustomThemes((prev) => {
    const next = prev.filter((t) => t.id !== id);
    saveCustomThemes(next);
    return next;
  });

  if (activeThemeId() === id) {
    setTheme("dark");
  }

  return true;
}

// ─── Import / Export ────────────────────────────────────────

/**
 * Export a theme as a JSON string suitable for file export.
 */
export function exportThemeAsJSON(id: ThemeId): string {
  const theme = resolveTheme(id);
  const { is_builtin: _, ...exportable } = theme;
  return JSON.stringify(exportable, null, 2);
}

/**
 * Import a theme from a JSON string.
 * Validates the required fields and registers as a custom theme.
 * Returns the imported theme on success, or null on failure.
 */
export function importThemeFromJSON(json: string): BentoTheme | null {
  try {
    const parsed = JSON.parse(json) as Record<string, unknown>;

    // Validate required fields exist
    if (typeof parsed.id !== "string" || parsed.id.trim().length === 0) {
      return null;
    }

    // Check that all CSS variable fields are present
    const requiredFields: Array<keyof BentoTheme> = [
      "id", "name_key", "preview_colors",
      "surface_zen", "surface_expanded", "surface_hover", "surface_active", "surface_subtle",
      "border_zen", "border_expanded", "border_hover",
      "text_primary", "text_secondary", "text_muted",
      "accent_blue", "accent_purple", "accent_green", "accent_orange", "accent_pink", "accent_red",
      "shadow_zen", "shadow_expanded", "shadow_item_hover",
      "blur_zen", "blur_expanded",
      "badge_bg",
      "radius_capsule", "radius_expanded", "radius_card", "radius_badge",
    ];

    for (const field of requiredFields) {
      if (!(field in parsed)) {
        return null;
      }
    }

    // Prevent overwriting built-in themes
    const isBuiltinId = BUILTIN_THEMES.some((t) => t.id === parsed.id);
    if (isBuiltinId) {
      return null;
    }

    const theme: BentoTheme = {
      ...(parsed as unknown as ExportableTheme),
      is_builtin: false,
    };

    registerCustomTheme(theme);
    return theme;
  } catch {
    return null;
  }
}
