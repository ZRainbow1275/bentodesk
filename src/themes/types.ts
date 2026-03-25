/**
 * BentoTheme — Theme type definitions for BentoDesk.
 *
 * Each theme defines all CSS custom property values used across
 * the glassmorphism surface system, borders, text, accent, shadows,
 * blur, radius, and badge styling.
 */

/** Unique identifier for a theme. Built-in themes use well-known IDs; custom themes use user-defined strings. */
export type ThemeId = string;

/**
 * Complete theme definition containing all CSS variable values
 * that get applied to :root when the theme is active.
 */
export interface BentoTheme {
  /** Unique identifier — lowercase kebab-case, e.g. "dark", "midnight", "my-custom" */
  id: ThemeId;

  /** Display name key for i18n lookup — e.g. "themeDark", "themeMidnight" */
  name_key: string;

  /** Whether this is a built-in theme (cannot be deleted) */
  is_builtin: boolean;

  /** Preview swatches — an array of 3-4 representative hex colors for the theme picker UI */
  preview_colors: string[];

  /** Surface colors (glassmorphism) */
  surface_zen: string;
  surface_expanded: string;
  surface_hover: string;
  surface_active: string;
  surface_subtle: string;

  /** Border colors */
  border_zen: string;
  border_expanded: string;
  border_hover: string;

  /** Text colors */
  text_primary: string;
  text_secondary: string;
  text_muted: string;

  /** Accent colors */
  accent_blue: string;
  accent_purple: string;
  accent_green: string;
  accent_orange: string;
  accent_pink: string;
  accent_red: string;

  /** Shadows */
  shadow_zen: string;
  shadow_expanded: string;
  shadow_item_hover: string;

  /** Blur / backdrop-filter */
  blur_zen: string;
  blur_expanded: string;

  /** Badge */
  badge_bg: string;

  /** Border radius */
  radius_capsule: string;
  radius_expanded: string;
  radius_card: string;
  radius_badge: string;
}

/**
 * Serializable theme data for JSON import/export.
 * Identical to BentoTheme but with is_builtin always false.
 */
export type ExportableTheme = Omit<BentoTheme, "is_builtin">;
