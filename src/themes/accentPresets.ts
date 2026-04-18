/**
 * Accent color presets used by the Bulk Manager palette picker.
 *
 * Each preset is a curated list of 12 colors that look good together when
 * applied across many zones at once (Nord, Dracula, Solarized — the
 * classic semantic palettes). Separate from `BentoTheme` because themes
 * own the full surface/border/text scheme, but a user might want to apply
 * only accent colors to zones without switching theme.
 */
import type { TranslationKey } from "../i18n/locales/zh-CN";

export interface AccentPreset {
  id: string;
  nameKey: TranslationKey;
  colors: readonly string[];
}

const NORD: AccentPreset = {
  id: "nord",
  nameKey: "paletteNord",
  colors: [
    "#bf616a",
    "#d08770",
    "#ebcb8b",
    "#a3be8c",
    "#b48ead",
    "#5e81ac",
    "#81a1c1",
    "#88c0d0",
    "#8fbcbb",
    "#d8dee9",
    "#e5e9f0",
    "#eceff4",
  ],
};

const DRACULA: AccentPreset = {
  id: "dracula",
  nameKey: "paletteDracula",
  colors: [
    "#ff5555",
    "#ff79c6",
    "#bd93f9",
    "#6272a4",
    "#8be9fd",
    "#50fa7b",
    "#f1fa8c",
    "#ffb86c",
    "#44475a",
    "#282a36",
    "#f8f8f2",
    "#6272a4",
  ],
};

const SOLARIZED: AccentPreset = {
  id: "solarized",
  nameKey: "paletteSolarized",
  colors: [
    "#b58900",
    "#cb4b16",
    "#dc322f",
    "#d33682",
    "#6c71c4",
    "#268bd2",
    "#2aa198",
    "#859900",
    "#002b36",
    "#073642",
    "#586e75",
    "#93a1a1",
  ],
};

const VIBRANT: AccentPreset = {
  id: "vibrant",
  nameKey: "paletteVibrant",
  colors: [
    "#ef4444",
    "#f97316",
    "#f59e0b",
    "#eab308",
    "#84cc16",
    "#22c55e",
    "#14b8a6",
    "#06b6d4",
    "#3b82f6",
    "#8b5cf6",
    "#d946ef",
    "#ec4899",
  ],
};

export const ACCENT_PRESETS: readonly AccentPreset[] = [
  NORD,
  DRACULA,
  SOLARIZED,
  VIBRANT,
];

export function findPresetById(id: string): AccentPreset | undefined {
  return ACCENT_PRESETS.find((p) => p.id === id);
}
