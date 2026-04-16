/**
 * Built-in theme presets for BentoDesk.
 *
 * 10 themes:
 * - dark: Current dark theme (default)
 * - light: Current light theme
 * - midnight: Deep navy/indigo
 * - forest: Green/earth tones
 * - sunset: Warm amber/orange
 * - frosted: Strong blur translucent surfaces
 * - solid: Fully opaque, no blur
 * - order: Swiss/Bauhaus — clean, structured, geometric
 * - neo: Neomorphism — soft shadows, extruded/inset effect
 * - flat: Flat Design — bold colors, zero shadows, sharp corners
 */
import type { BentoTheme } from "./types";

export const darkTheme: BentoTheme = {
  id: "dark",
  name_key: "themeDark",
  is_builtin: true,
  preview_colors: ["#12121a", "#3b82f6", "#f0f0f5", "#1a1a24"],

  surface_zen: "rgba(18, 18, 24, 0.55)",
  surface_expanded: "rgba(12, 12, 18, 0.82)",
  surface_hover: "rgba(255, 255, 255, 0.08)",
  surface_active: "rgba(255, 255, 255, 0.05)",
  surface_subtle: "rgba(255, 255, 255, 0.03)",

  border_zen: "rgba(255, 255, 255, 0.1)",
  border_expanded: "rgba(255, 255, 255, 0.12)",
  border_hover: "rgba(255, 255, 255, 0.2)",

  text_primary: "#f0f0f5",
  text_secondary: "#c0c0cc",
  text_muted: "#78788a",

  accent_blue: "#3b82f6",
  accent_purple: "#8b5cf6",
  accent_green: "#22c55e",
  accent_orange: "#f97316",
  accent_pink: "#ec4899",
  accent_red: "#ef4444",

  shadow_zen: "0 2px 8px rgba(0, 0, 0, 0.15), 0 8px 32px rgba(0, 0, 0, 0.25)",
  shadow_expanded: "0 4px 16px rgba(0, 0, 0, 0.2), 0 16px 48px rgba(0, 0, 0, 0.4)",
  shadow_item_hover: "0 2px 8px rgba(0, 0, 0, 0.12), 0 8px 24px rgba(0, 0, 0, 0.08)",

  blur_zen: "blur(20px) saturate(160%)",
  blur_expanded: "blur(24px) saturate(170%)",

  badge_bg: "rgba(255, 255, 255, 0.12)",

  radius_capsule: "24px",
  radius_expanded: "16px",
  radius_card: "10px",
  radius_badge: "10px",
};

export const lightTheme: BentoTheme = {
  id: "light",
  name_key: "themeLight",
  is_builtin: true,
  preview_colors: ["#fafafc", "#3b82f6", "#111118", "#ffffff"],

  surface_zen: "rgba(250, 250, 252, 0.6)",
  surface_expanded: "rgba(255, 255, 255, 0.85)",
  surface_hover: "rgba(0, 0, 0, 0.05)",
  surface_active: "rgba(0, 0, 0, 0.03)",
  surface_subtle: "rgba(0, 0, 0, 0.02)",

  border_zen: "rgba(0, 0, 0, 0.08)",
  border_expanded: "rgba(0, 0, 0, 0.1)",
  border_hover: "rgba(0, 0, 0, 0.16)",

  text_primary: "#111118",
  text_secondary: "#3c3c46",
  text_muted: "#8e8e9a",

  accent_blue: "#3b82f6",
  accent_purple: "#8b5cf6",
  accent_green: "#22c55e",
  accent_orange: "#f97316",
  accent_pink: "#ec4899",
  accent_red: "#ef4444",

  shadow_zen: "0 2px 8px rgba(0, 0, 0, 0.04), 0 8px 32px rgba(0, 0, 0, 0.06)",
  shadow_expanded: "0 4px 16px rgba(0, 0, 0, 0.06), 0 16px 48px rgba(0, 0, 0, 0.12)",
  shadow_item_hover: "0 2px 8px rgba(0, 0, 0, 0.04), 0 4px 12px rgba(0, 0, 0, 0.04)",

  blur_zen: "blur(20px) saturate(130%)",
  blur_expanded: "blur(24px) saturate(140%)",

  badge_bg: "rgba(0, 0, 0, 0.06)",

  radius_capsule: "24px",
  radius_expanded: "16px",
  radius_card: "10px",
  radius_badge: "10px",
};

export const midnightTheme: BentoTheme = {
  id: "midnight",
  name_key: "themeMidnight",
  is_builtin: true,
  preview_colors: ["#0f172a", "#6366f1", "#e2e8f0", "#1e293b"],

  surface_zen: "rgba(15, 23, 42, 0.6)",
  surface_expanded: "rgba(15, 23, 42, 0.88)",
  surface_hover: "rgba(99, 102, 241, 0.1)",
  surface_active: "rgba(99, 102, 241, 0.06)",
  surface_subtle: "rgba(99, 102, 241, 0.03)",

  border_zen: "rgba(99, 102, 241, 0.15)",
  border_expanded: "rgba(99, 102, 241, 0.2)",
  border_hover: "rgba(99, 102, 241, 0.35)",

  text_primary: "#e2e8f0",
  text_secondary: "#94a3b8",
  text_muted: "#64748b",

  accent_blue: "#6366f1",
  accent_purple: "#a78bfa",
  accent_green: "#34d399",
  accent_orange: "#fb923c",
  accent_pink: "#f472b6",
  accent_red: "#f87171",

  shadow_zen: "0 2px 8px rgba(0, 0, 0, 0.2), 0 8px 32px rgba(15, 23, 42, 0.4)",
  shadow_expanded: "0 4px 16px rgba(0, 0, 0, 0.25), 0 16px 48px rgba(15, 23, 42, 0.5)",
  shadow_item_hover: "0 2px 8px rgba(99, 102, 241, 0.08), 0 8px 24px rgba(0, 0, 0, 0.1)",

  blur_zen: "blur(22px) saturate(180%)",
  blur_expanded: "blur(28px) saturate(190%)",

  badge_bg: "rgba(99, 102, 241, 0.15)",

  radius_capsule: "24px",
  radius_expanded: "16px",
  radius_card: "10px",
  radius_badge: "10px",
};

export const forestTheme: BentoTheme = {
  id: "forest",
  name_key: "themeForest",
  is_builtin: true,
  preview_colors: ["#1a2e1a", "#22c55e", "#e8f5e9", "#2d4a2d"],

  surface_zen: "rgba(26, 46, 26, 0.55)",
  surface_expanded: "rgba(20, 38, 20, 0.85)",
  surface_hover: "rgba(34, 197, 94, 0.1)",
  surface_active: "rgba(34, 197, 94, 0.06)",
  surface_subtle: "rgba(34, 197, 94, 0.03)",

  border_zen: "rgba(34, 197, 94, 0.12)",
  border_expanded: "rgba(34, 197, 94, 0.18)",
  border_hover: "rgba(34, 197, 94, 0.3)",

  text_primary: "#e8f5e9",
  text_secondary: "#a5d6a7",
  text_muted: "#6b8e6b",

  accent_blue: "#4ade80",
  accent_purple: "#a78bfa",
  accent_green: "#22c55e",
  accent_orange: "#fbbf24",
  accent_pink: "#f472b6",
  accent_red: "#ef4444",

  shadow_zen: "0 2px 8px rgba(0, 0, 0, 0.15), 0 8px 32px rgba(10, 30, 10, 0.3)",
  shadow_expanded: "0 4px 16px rgba(0, 0, 0, 0.2), 0 16px 48px rgba(10, 30, 10, 0.4)",
  shadow_item_hover: "0 2px 8px rgba(34, 197, 94, 0.06), 0 8px 24px rgba(0, 0, 0, 0.08)",

  blur_zen: "blur(20px) saturate(150%)",
  blur_expanded: "blur(24px) saturate(160%)",

  badge_bg: "rgba(34, 197, 94, 0.15)",

  radius_capsule: "24px",
  radius_expanded: "16px",
  radius_card: "10px",
  radius_badge: "10px",
};

export const sunsetTheme: BentoTheme = {
  id: "sunset",
  name_key: "themeSunset",
  is_builtin: true,
  preview_colors: ["#2a1a0a", "#f59e0b", "#fef3c7", "#3d2b16"],

  surface_zen: "rgba(42, 26, 10, 0.55)",
  surface_expanded: "rgba(35, 20, 8, 0.85)",
  surface_hover: "rgba(245, 158, 11, 0.1)",
  surface_active: "rgba(245, 158, 11, 0.06)",
  surface_subtle: "rgba(245, 158, 11, 0.03)",

  border_zen: "rgba(245, 158, 11, 0.14)",
  border_expanded: "rgba(245, 158, 11, 0.2)",
  border_hover: "rgba(245, 158, 11, 0.32)",

  text_primary: "#fef3c7",
  text_secondary: "#fcd34d",
  text_muted: "#a88a4a",

  accent_blue: "#f59e0b",
  accent_purple: "#c084fc",
  accent_green: "#84cc16",
  accent_orange: "#f97316",
  accent_pink: "#fb7185",
  accent_red: "#ef4444",

  shadow_zen: "0 2px 8px rgba(0, 0, 0, 0.15), 0 8px 32px rgba(30, 15, 0, 0.3)",
  shadow_expanded: "0 4px 16px rgba(0, 0, 0, 0.2), 0 16px 48px rgba(30, 15, 0, 0.4)",
  shadow_item_hover: "0 2px 8px rgba(245, 158, 11, 0.06), 0 8px 24px rgba(0, 0, 0, 0.08)",

  blur_zen: "blur(20px) saturate(160%)",
  blur_expanded: "blur(24px) saturate(170%)",

  badge_bg: "rgba(245, 158, 11, 0.15)",

  radius_capsule: "24px",
  radius_expanded: "16px",
  radius_card: "10px",
  radius_badge: "10px",
};

export const frostedTheme: BentoTheme = {
  id: "frosted",
  name_key: "themeFrosted",
  is_builtin: true,
  preview_colors: ["rgba(255,255,255,0.15)", "#60a5fa", "#f0f0f5", "rgba(255,255,255,0.25)"],

  surface_zen: "rgba(255, 255, 255, 0.08)",
  surface_expanded: "rgba(255, 255, 255, 0.12)",
  surface_hover: "rgba(255, 255, 255, 0.15)",
  surface_active: "rgba(255, 255, 255, 0.1)",
  surface_subtle: "rgba(255, 255, 255, 0.05)",

  border_zen: "rgba(255, 255, 255, 0.18)",
  border_expanded: "rgba(255, 255, 255, 0.22)",
  border_hover: "rgba(255, 255, 255, 0.35)",

  text_primary: "#f0f0f5",
  text_secondary: "#d0d0dd",
  text_muted: "#8888aa",

  accent_blue: "#60a5fa",
  accent_purple: "#a78bfa",
  accent_green: "#4ade80",
  accent_orange: "#fbbf24",
  accent_pink: "#f472b6",
  accent_red: "#f87171",

  shadow_zen: "0 2px 8px rgba(0, 0, 0, 0.08), 0 8px 32px rgba(0, 0, 0, 0.12)",
  shadow_expanded: "0 4px 16px rgba(0, 0, 0, 0.1), 0 16px 48px rgba(0, 0, 0, 0.2)",
  shadow_item_hover: "0 2px 8px rgba(0, 0, 0, 0.06), 0 4px 16px rgba(0, 0, 0, 0.06)",

  blur_zen: "blur(40px) saturate(200%)",
  blur_expanded: "blur(50px) saturate(220%)",

  badge_bg: "rgba(255, 255, 255, 0.15)",

  radius_capsule: "24px",
  radius_expanded: "16px",
  radius_card: "10px",
  radius_badge: "10px",
};

export const solidTheme: BentoTheme = {
  id: "solid",
  name_key: "themeSolid",
  is_builtin: true,
  preview_colors: ["#1e1e2e", "#89b4fa", "#cdd6f4", "#313244"],

  surface_zen: "rgba(30, 30, 46, 1)",
  surface_expanded: "rgba(24, 24, 37, 1)",
  surface_hover: "rgba(49, 50, 68, 1)",
  surface_active: "rgba(45, 45, 60, 1)",
  surface_subtle: "rgba(35, 35, 50, 1)",

  border_zen: "rgba(69, 71, 90, 1)",
  border_expanded: "rgba(88, 91, 112, 1)",
  border_hover: "rgba(108, 112, 134, 1)",

  text_primary: "#cdd6f4",
  text_secondary: "#a6adc8",
  text_muted: "#6c7086",

  accent_blue: "#89b4fa",
  accent_purple: "#cba6f7",
  accent_green: "#a6e3a1",
  accent_orange: "#fab387",
  accent_pink: "#f5c2e7",
  accent_red: "#f38ba8",

  shadow_zen: "0 2px 8px rgba(0, 0, 0, 0.2), 0 8px 32px rgba(0, 0, 0, 0.3)",
  shadow_expanded: "0 4px 16px rgba(0, 0, 0, 0.25), 0 16px 48px rgba(0, 0, 0, 0.45)",
  shadow_item_hover: "0 2px 8px rgba(0, 0, 0, 0.15), 0 4px 16px rgba(0, 0, 0, 0.1)",

  blur_zen: "blur(0px) saturate(100%)",
  blur_expanded: "blur(0px) saturate(100%)",

  badge_bg: "rgba(69, 71, 90, 0.6)",

  radius_capsule: "24px",
  radius_expanded: "16px",
  radius_card: "10px",
  radius_badge: "10px",
};

export const orderTheme: BentoTheme = {
  id: "order",
  name_key: "themeOrder",
  is_builtin: true,
  preview_colors: ["#FF512F", "#FAFAFA", "#1F2937", "#CBD5E1"],

  surface_zen: "rgba(250, 250, 250, 0.95)",
  surface_expanded: "rgba(255, 255, 255, 0.98)",
  surface_hover: "rgba(0, 0, 0, 0.04)",
  surface_active: "rgba(0, 0, 0, 0.06)",
  surface_subtle: "rgba(0, 0, 0, 0.02)",

  border_zen: "rgba(203, 213, 225, 0.5)",
  border_expanded: "rgba(203, 213, 225, 0.6)",
  border_hover: "rgba(203, 213, 225, 0.8)",

  text_primary: "#1F2937",
  text_secondary: "#374151",
  text_muted: "#94A3B8",

  accent_blue: "#FF512F",
  accent_purple: "#DD2476",
  accent_green: "#22c55e",
  accent_orange: "#f97316",
  accent_pink: "#DD2476",
  accent_red: "#ef4444",

  shadow_zen: "0 1px 3px rgba(0, 0, 0, 0.08)",
  shadow_expanded: "0 2px 8px rgba(0, 0, 0, 0.1)",
  shadow_item_hover: "0 1px 4px rgba(0, 0, 0, 0.06)",

  blur_zen: "none",
  blur_expanded: "none",

  badge_bg: "rgba(255, 81, 47, 0.12)",

  radius_capsule: "8px",
  radius_expanded: "8px",
  radius_card: "6px",
  radius_badge: "6px",
};

export const neoTheme: BentoTheme = {
  id: "neo",
  name_key: "themeNeo",
  is_builtin: true,
  preview_colors: ["#667EEA", "#E6E8EE", "#2D3748", "#FFFFFF"],

  surface_zen: "rgba(230, 232, 238, 0.95)",
  surface_expanded: "rgba(230, 232, 238, 0.98)",
  surface_hover: "rgba(255, 255, 255, 0.5)",
  surface_active: "rgba(255, 255, 255, 0.3)",
  surface_subtle: "rgba(255, 255, 255, 0.2)",

  border_zen: "transparent",
  border_expanded: "transparent",
  border_hover: "rgba(163, 177, 198, 0.2)",

  text_primary: "#2D3748",
  text_secondary: "#4A5568",
  text_muted: "#A0AEC0",

  accent_blue: "#667EEA",
  accent_purple: "#9F7AEA",
  accent_green: "#48BB78",
  accent_orange: "#ED8936",
  accent_pink: "#ED64A6",
  accent_red: "#FC8181",

  shadow_zen: "6px 6px 12px rgba(163, 177, 198, 0.6), -6px -6px 12px rgba(255, 255, 255, 0.8)",
  shadow_expanded: "8px 8px 16px rgba(163, 177, 198, 0.6), -8px -8px 16px rgba(255, 255, 255, 0.8)",
  shadow_item_hover: "4px 4px 8px rgba(163, 177, 198, 0.5), -4px -4px 8px rgba(255, 255, 255, 0.7)",

  blur_zen: "none",
  blur_expanded: "none",

  badge_bg: "rgba(102, 126, 234, 0.12)",

  radius_capsule: "16px",
  radius_expanded: "16px",
  radius_card: "12px",
  radius_badge: "12px",
};

export const flatTheme: BentoTheme = {
  id: "flat",
  name_key: "themeFlat",
  is_builtin: true,
  preview_colors: ["#E74C3C", "#2C3E50", "#ECF0F1", "#3498DB"],

  surface_zen: "rgba(44, 62, 80, 0.95)",
  surface_expanded: "rgba(44, 62, 80, 0.98)",
  surface_hover: "rgba(255, 255, 255, 0.1)",
  surface_active: "rgba(255, 255, 255, 0.06)",
  surface_subtle: "rgba(255, 255, 255, 0.03)",

  border_zen: "rgba(236, 240, 241, 0.15)",
  border_expanded: "rgba(236, 240, 241, 0.2)",
  border_hover: "rgba(236, 240, 241, 0.3)",

  text_primary: "#ECF0F1",
  text_secondary: "#BDC3C7",
  text_muted: "#7F8C8D",

  accent_blue: "#3498DB",
  accent_purple: "#9B59B6",
  accent_green: "#2ECC71",
  accent_orange: "#E67E22",
  accent_pink: "#E91E8C",
  accent_red: "#E74C3C",

  shadow_zen: "none",
  shadow_expanded: "none",
  shadow_item_hover: "none",

  blur_zen: "none",
  blur_expanded: "none",

  badge_bg: "rgba(231, 76, 60, 0.2)",

  radius_capsule: "4px",
  radius_expanded: "4px",
  radius_card: "4px",
  radius_badge: "4px",
};

export const oceanBlueTheme: BentoTheme = {
  id: "ocean-blue",
  name_key: "themeOceanBlue",
  is_builtin: true,
  preview_colors: ["#082f49", "#0ea5e9", "#e0f2fe", "#0c4a6e"],

  surface_zen: "rgba(8, 47, 73, 0.6)",
  surface_expanded: "rgba(8, 47, 73, 0.85)",
  surface_hover: "rgba(14, 165, 233, 0.1)",
  surface_active: "rgba(14, 165, 233, 0.06)",
  surface_subtle: "rgba(14, 165, 233, 0.03)",

  border_zen: "rgba(14, 165, 233, 0.15)",
  border_expanded: "rgba(14, 165, 233, 0.2)",
  border_hover: "rgba(14, 165, 233, 0.35)",

  text_primary: "#e0f2fe",
  text_secondary: "#7dd3fc",
  text_muted: "#0369a1",

  accent_blue: "#0ea5e9",
  accent_purple: "#a78bfa",
  accent_green: "#34d399",
  accent_orange: "#fb923c",
  accent_pink: "#f472b6",
  accent_red: "#f87171",

  shadow_zen: "0 2px 8px rgba(0, 0, 0, 0.2), 0 8px 32px rgba(8, 47, 73, 0.4)",
  shadow_expanded: "0 4px 16px rgba(0, 0, 0, 0.25), 0 16px 48px rgba(8, 47, 73, 0.5)",
  shadow_item_hover: "0 2px 8px rgba(14, 165, 233, 0.08), 0 8px 24px rgba(0, 0, 0, 0.1)",

  blur_zen: "blur(20px) saturate(160%)",
  blur_expanded: "blur(24px) saturate(170%)",

  badge_bg: "rgba(14, 165, 233, 0.15)",

  radius_capsule: "24px",
  radius_expanded: "16px",
  radius_card: "10px",
  radius_badge: "10px",
};

export const roseGoldTheme: BentoTheme = {
  id: "rose-gold",
  name_key: "themeRoseGold",
  is_builtin: true,
  preview_colors: ["#4c1d27", "#f43f5e", "#fff1f2", "#881337"],

  surface_zen: "rgba(76, 29, 39, 0.6)",
  surface_expanded: "rgba(76, 29, 39, 0.85)",
  surface_hover: "rgba(244, 63, 94, 0.1)",
  surface_active: "rgba(244, 63, 94, 0.06)",
  surface_subtle: "rgba(244, 63, 94, 0.03)",

  border_zen: "rgba(244, 63, 94, 0.15)",
  border_expanded: "rgba(244, 63, 94, 0.2)",
  border_hover: "rgba(244, 63, 94, 0.35)",

  text_primary: "#fff1f2",
  text_secondary: "#fda4af",
  text_muted: "#9f1239",

  accent_blue: "#f43f5e",
  accent_purple: "#c084fc",
  accent_green: "#4ade80",
  accent_orange: "#fbbf24",
  accent_pink: "#f472b6",
  accent_red: "#ef4444",

  shadow_zen: "0 2px 8px rgba(0, 0, 0, 0.2), 0 8px 32px rgba(76, 29, 39, 0.4)",
  shadow_expanded: "0 4px 16px rgba(0, 0, 0, 0.25), 0 16px 48px rgba(76, 29, 39, 0.5)",
  shadow_item_hover: "0 2px 8px rgba(244, 63, 94, 0.08), 0 8px 24px rgba(0, 0, 0, 0.1)",

  blur_zen: "blur(22px) saturate(150%)",
  blur_expanded: "blur(28px) saturate(160%)",

  badge_bg: "rgba(244, 63, 94, 0.15)",

  radius_capsule: "24px",
  radius_expanded: "16px",
  radius_card: "10px",
  radius_badge: "10px",
};

export const forestGreenTheme: BentoTheme = {
  id: "forest-green",
  name_key: "themeForestGreen",
  is_builtin: true,
  preview_colors: ["#142e1a", "#22c55e", "#dcfce7", "#166534"],

  surface_zen: "rgba(20, 46, 26, 0.6)",
  surface_expanded: "rgba(20, 46, 26, 0.85)",
  surface_hover: "rgba(34, 197, 94, 0.1)",
  surface_active: "rgba(34, 197, 94, 0.06)",
  surface_subtle: "rgba(34, 197, 94, 0.03)",

  border_zen: "rgba(34, 197, 94, 0.15)",
  border_expanded: "rgba(34, 197, 94, 0.2)",
  border_hover: "rgba(34, 197, 94, 0.35)",

  text_primary: "#dcfce7",
  text_secondary: "#86efac",
  text_muted: "#166534",

  accent_blue: "#22c55e",
  accent_purple: "#a78bfa",
  accent_green: "#4ade80",
  accent_orange: "#fbbf24",
  accent_pink: "#f472b6",
  accent_red: "#ef4444",

  shadow_zen: "0 2px 8px rgba(0, 0, 0, 0.2), 0 8px 32px rgba(20, 46, 26, 0.4)",
  shadow_expanded: "0 4px 16px rgba(0, 0, 0, 0.25), 0 16px 48px rgba(20, 46, 26, 0.5)",
  shadow_item_hover: "0 2px 8px rgba(34, 197, 94, 0.08), 0 8px 24px rgba(0, 0, 0, 0.1)",

  blur_zen: "blur(20px) saturate(150%)",
  blur_expanded: "blur(24px) saturate(160%)",

  badge_bg: "rgba(34, 197, 94, 0.15)",

  radius_capsule: "24px",
  radius_expanded: "16px",
  radius_card: "10px",
  radius_badge: "10px",
};

export const brutalismTheme: BentoTheme = {
  id: "brutalism",
  name_key: "themeBrutalism",
  is_builtin: true,
  preview_colors: ["#FFD400", "#000000", "#FFFFFF", "#E63946"],

  surface_zen: "#FFD400",
  surface_expanded: "rgba(255, 255, 255, 0.98)",
  surface_hover: "rgba(0, 0, 0, 0.08)",
  surface_active: "rgba(0, 0, 0, 0.12)",
  surface_subtle: "rgba(0, 0, 0, 0.04)",

  border_zen: "#000000",
  border_expanded: "#000000",
  border_hover: "#000000",

  text_primary: "#000000",
  text_secondary: "#1a1a1a",
  text_muted: "#4a4a4a",

  accent_blue: "#0066FF",
  accent_purple: "#7C3AED",
  accent_green: "#00A86B",
  accent_orange: "#FFD400",
  accent_pink: "#FF2E93",
  accent_red: "#E63946",

  shadow_zen: "none",
  shadow_expanded: "none",
  shadow_item_hover: "none",

  blur_zen: "none",
  blur_expanded: "none",

  badge_bg: "rgba(0, 0, 0, 0.12)",

  radius_capsule: "0px",
  radius_expanded: "0px",
  radius_card: "0px",
  radius_badge: "0px",

  is_light: true,
  border_width: "3px",
  effect: "none",
};

export const terminalTheme: BentoTheme = {
  id: "terminal",
  name_key: "themeTerminal",
  is_builtin: true,
  preview_colors: ["#0A0E0C", "#00FF9C", "#050705", "#003D24"],

  surface_zen: "rgba(10, 14, 12, 0.92)",
  surface_expanded: "rgba(5, 7, 5, 0.98)",
  surface_hover: "rgba(0, 255, 156, 0.08)",
  surface_active: "rgba(0, 255, 156, 0.14)",
  surface_subtle: "rgba(0, 255, 156, 0.04)",

  border_zen: "rgba(0, 255, 156, 0.35)",
  border_expanded: "rgba(0, 255, 156, 0.45)",
  border_hover: "rgba(0, 255, 156, 0.7)",

  text_primary: "#00FF9C",
  text_secondary: "rgba(0, 255, 156, 0.78)",
  text_muted: "rgba(0, 255, 156, 0.5)",

  accent_blue: "#00D4FF",
  accent_purple: "#B794FF",
  accent_green: "#00FF9C",
  accent_orange: "#FFB400",
  accent_pink: "#FF6EC7",
  accent_red: "#FF3366",

  shadow_zen: "0 0 0 1px rgba(0, 255, 156, 0.25), 0 0 16px rgba(0, 255, 156, 0.15)",
  shadow_expanded: "0 0 0 1px rgba(0, 255, 156, 0.4), 0 0 32px rgba(0, 255, 156, 0.2)",
  shadow_item_hover: "0 0 12px rgba(0, 255, 156, 0.25)",

  blur_zen: "none",
  blur_expanded: "none",

  badge_bg: "rgba(0, 255, 156, 0.13)",

  radius_capsule: "2px",
  radius_expanded: "2px",
  radius_card: "2px",
  radius_badge: "2px",

  is_light: false,
  font_family: '"JetBrains Mono", "Consolas", ui-monospace, monospace',
  effect: "scanlines",
};

export const cyberpunkTheme: BentoTheme = {
  id: "cyberpunk",
  name_key: "themeCyberpunk",
  is_builtin: true,
  preview_colors: ["#0C0420", "#00F0FF", "#FF2E93", "#1A0B3B"],

  surface_zen: "rgba(12, 4, 32, 0.78)",
  surface_expanded: "rgba(12, 4, 32, 0.94)",
  surface_hover: "rgba(0, 240, 255, 0.08)",
  surface_active: "rgba(255, 46, 147, 0.1)",
  surface_subtle: "rgba(0, 240, 255, 0.04)",

  border_zen: "rgba(0, 240, 255, 0.55)",
  border_expanded: "rgba(255, 46, 147, 0.6)",
  border_hover: "rgba(0, 240, 255, 0.85)",

  text_primary: "#E7F7FF",
  text_secondary: "#7FD3F7",
  text_muted: "#6B5E8E",

  accent_blue: "#00F0FF",
  accent_purple: "#B026FF",
  accent_green: "#39FF14",
  accent_orange: "#FFB400",
  accent_pink: "#FF2E93",
  accent_red: "#FF3864",

  shadow_zen: "0 0 16px rgba(0, 240, 255, 0.35), 0 0 32px rgba(255, 46, 147, 0.2)",
  shadow_expanded: "0 0 24px rgba(0, 240, 255, 0.5), 0 0 48px rgba(255, 46, 147, 0.3)",
  shadow_item_hover: "0 0 12px rgba(0, 240, 255, 0.4), 0 0 24px rgba(255, 46, 147, 0.25)",

  blur_zen: "blur(12px) saturate(180%)",
  blur_expanded: "blur(16px) saturate(200%)",

  badge_bg: "rgba(0, 240, 255, 0.16)",

  radius_capsule: "3px",
  radius_expanded: "3px",
  radius_card: "3px",
  radius_badge: "3px",

  is_light: false,
  border_width: "1.5px",
  effect: "neon",
};

export const editorialTheme: BentoTheme = {
  id: "editorial",
  name_key: "themeEditorial",
  is_builtin: true,
  preview_colors: ["#FAFAFA", "#0A0A0A", "#D7263D", "#E5E5E5"],

  surface_zen: "rgba(250, 250, 250, 0.96)",
  surface_expanded: "rgba(255, 255, 255, 1)",
  surface_hover: "rgba(0, 0, 0, 0.03)",
  surface_active: "rgba(0, 0, 0, 0.05)",
  surface_subtle: "rgba(0, 0, 0, 0.015)",

  border_zen: "rgba(0, 0, 0, 0.1)",
  border_expanded: "rgba(0, 0, 0, 0.12)",
  border_hover: "rgba(0, 0, 0, 0.2)",

  text_primary: "#0A0A0A",
  text_secondary: "#3a3a3a",
  text_muted: "#888888",

  accent_blue: "#1F3A8A",
  accent_purple: "#5B21B6",
  accent_green: "#166534",
  accent_orange: "#C2410C",
  accent_pink: "#BE185D",
  accent_red: "#D7263D",

  shadow_zen: "none",
  shadow_expanded: "none",
  shadow_item_hover: "none",

  blur_zen: "none",
  blur_expanded: "none",

  badge_bg: "rgba(215, 38, 61, 0.1)",

  radius_capsule: "0px",
  radius_expanded: "0px",
  radius_card: "0px",
  radius_badge: "0px",

  is_light: true,
  font_family: '"Playfair Display", Georgia, "Times New Roman", serif',
  border_width: "1px",
  // Subtle chromatic aberration on h1/h2 — fits the magazine/print aesthetic
  // and exercises the `chromatic` channel defined in theme-effects.css that
  // was previously a typed-but-unused option.
  effect: "chromatic",
};

/** All built-in themes in display order. */
export const BUILTIN_THEMES: readonly BentoTheme[] = [
  darkTheme,
  lightTheme,
  midnightTheme,
  forestTheme,
  sunsetTheme,
  frostedTheme,
  solidTheme,
  orderTheme,
  neoTheme,
  flatTheme,
  oceanBlueTheme,
  roseGoldTheme,
  forestGreenTheme,
  brutalismTheme,
  terminalTheme,
  cyberpunkTheme,
  editorialTheme,
];
