# Theme Development Guide

BentoDesk's theme system controls all visual aspects of the UI through CSS custom properties. Themes define colors for glassmorphism surfaces, borders, text, accents, shadows, blur effects, and border radii.

## BentoTheme Interface

Every theme implements the `BentoTheme` interface defined in `src/themes/types.ts`. The interface has 31 fields (4 metadata + 27 CSS variable values).

### Metadata Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique identifier, lowercase kebab-case (e.g. `"my-custom-theme"`) |
| `name_key` | `string` | i18n translation key for the display name (e.g. `"themeCustom"`) |
| `is_builtin` | `boolean` | Whether this is a built-in theme (cannot be deleted). Always `false` for custom themes. |
| `preview_colors` | `string[]` | Array of 3-4 representative hex colors for the theme picker UI |

### Surface Colors (Glassmorphism)

These control the translucent background layers of zones and UI elements.

| Field | CSS Variable | Description |
|-------|-------------|-------------|
| `surface_zen` | `--surface-zen` | Background of collapsed zone capsules. Typically semi-transparent (e.g. `rgba(18, 18, 24, 0.55)`) |
| `surface_expanded` | `--surface-expanded` | Background of expanded zone panels. More opaque than zen (e.g. `rgba(12, 12, 18, 0.82)`) |
| `surface_hover` | `--surface-hover` | Background on hover state for interactive elements |
| `surface_active` | `--surface-active` | Background on active/pressed state |
| `surface_subtle` | `--surface-subtle` | Very subtle background for secondary areas |

### Border Colors

| Field | CSS Variable | Description |
|-------|-------------|-------------|
| `border_zen` | `--border-zen` | Border of collapsed zone capsules |
| `border_expanded` | `--border-expanded` | Border of expanded zone panels |
| `border_hover` | `--border-hover` | Border on hover state |

### Text Colors

| Field | CSS Variable | Description |
|-------|-------------|-------------|
| `text_primary` | `--text-primary` | Main text color (headings, file names) |
| `text_secondary` | `--text-secondary` | Secondary text (descriptions, metadata) |
| `text_muted` | `--text-muted` | Muted text (placeholders, disabled) |

### Accent Colors

| Field | CSS Variable | Description |
|-------|-------------|-------------|
| `accent_blue` | `--accent-blue` | Primary accent color. Used for zone highlights, buttons, links |
| `accent_purple` | `--accent-purple` | Secondary accent |
| `accent_green` | `--accent-green` | Success/positive accent |
| `accent_orange` | `--accent-orange` | Warning accent |
| `accent_pink` | `--accent-pink` | Decorative accent |
| `accent_red` | `--accent-red` | Error/danger accent |

### Shadows

| Field | CSS Variable | Description |
|-------|-------------|-------------|
| `shadow_zen` | `--shadow-zen` | Box shadow for collapsed zone capsules |
| `shadow_expanded` | `--shadow-expanded` | Box shadow for expanded zone panels |
| `shadow_item_hover` | `--shadow-item-hover` | Box shadow for hovered file items |

### Blur / Backdrop Filter

| Field | CSS Variable | Description |
|-------|-------------|-------------|
| `blur_zen` | `--blur-zen` | Backdrop-filter for collapsed capsules (e.g. `"blur(20px) saturate(160%)"`) |
| `blur_expanded` | `--blur-expanded` | Backdrop-filter for expanded panels |

Set to `"none"` to disable blur effects entirely (as in the Solid, Order, Neo, and Flat themes).

### Badge

| Field | CSS Variable | Description |
|-------|-------------|-------------|
| `badge_bg` | `--badge-bg` | Background color for item count badges on capsules |

### Border Radius

| Field | CSS Variable | Description |
|-------|-------------|-------------|
| `radius_capsule` | `--radius-capsule` | Border radius for collapsed zone capsules |
| `radius_expanded` | `--radius-expanded` | Border radius for expanded zone panels |
| `radius_card` | `--radius-card` | Border radius for file item cards |
| `radius_badge` | `--radius-badge` | Border radius for badges |

## Built-in Themes

BentoDesk ships with 10 built-in themes:

| ID | Name | Style |
|----|------|-------|
| `dark` | Dark | Default dark glassmorphism theme |
| `light` | Light | Light frosted glass |
| `midnight` | Midnight | Deep navy/indigo color palette |
| `forest` | Forest | Green/earth tones |
| `sunset` | Sunset | Warm amber/orange hues |
| `frosted` | Frosted | Strong blur, highly translucent surfaces |
| `solid` | Solid | Fully opaque surfaces, no blur (Catppuccin-inspired) |
| `order` | Order | Swiss/Bauhaus -- clean, structured, geometric |
| `neo` | Neo | Neomorphism -- soft dual shadows, extruded/inset effect |
| `flat` | Flat | Flat Design -- bold colors, zero shadows, sharp corners |

## Creating a Custom Theme

### Step 1: Define the Theme JSON

Create a JSON file with all required fields:

```json
{
  "id": "ocean-breeze",
  "name_key": "themeCustom",
  "preview_colors": ["#0a1628", "#0ea5e9", "#e0f2fe", "#1e3a5f"],

  "surface_zen": "rgba(10, 22, 40, 0.6)",
  "surface_expanded": "rgba(8, 18, 35, 0.88)",
  "surface_hover": "rgba(14, 165, 233, 0.1)",
  "surface_active": "rgba(14, 165, 233, 0.06)",
  "surface_subtle": "rgba(14, 165, 233, 0.03)",

  "border_zen": "rgba(14, 165, 233, 0.15)",
  "border_expanded": "rgba(14, 165, 233, 0.2)",
  "border_hover": "rgba(14, 165, 233, 0.35)",

  "text_primary": "#e0f2fe",
  "text_secondary": "#7dd3fc",
  "text_muted": "#0369a1",

  "accent_blue": "#0ea5e9",
  "accent_purple": "#a78bfa",
  "accent_green": "#34d399",
  "accent_orange": "#fb923c",
  "accent_pink": "#f472b6",
  "accent_red": "#f87171",

  "shadow_zen": "0 2px 8px rgba(0, 0, 0, 0.2), 0 8px 32px rgba(10, 22, 40, 0.4)",
  "shadow_expanded": "0 4px 16px rgba(0, 0, 0, 0.25), 0 16px 48px rgba(10, 22, 40, 0.5)",
  "shadow_item_hover": "0 2px 8px rgba(14, 165, 233, 0.08), 0 8px 24px rgba(0, 0, 0, 0.1)",

  "blur_zen": "blur(22px) saturate(180%)",
  "blur_expanded": "blur(28px) saturate(190%)",

  "badge_bg": "rgba(14, 165, 233, 0.15)",

  "radius_capsule": "24px",
  "radius_expanded": "16px",
  "radius_card": "10px",
  "radius_badge": "10px"
}
```

### Step 2: Import via Settings

1. Open BentoDesk Settings (right-click tray icon or click gear icon)
2. Scroll to **Developer Options** section
3. Click **Import Theme**
4. Paste the JSON into the text area
5. Confirm the import

The theme will appear in the theme picker immediately.

### Step 3: Activate the Theme

Select the imported theme from the theme picker in Settings > Appearance.

## Programmatic API

### `registerCustomTheme(theme: BentoTheme): void`

Register a custom theme. Persists to `localStorage`. If a custom theme with the same ID already exists, it is replaced.

```typescript
import { registerCustomTheme } from "../themes";

registerCustomTheme({
  id: "ocean-breeze",
  name_key: "themeCustom",
  is_builtin: false,
  preview_colors: ["#0a1628", "#0ea5e9", "#e0f2fe", "#1e3a5f"],
  surface_zen: "rgba(10, 22, 40, 0.6)",
  // ... all other fields
});
```

### `importThemeFromJSON(json: string): BentoTheme | null`

Import a theme from a JSON string. Validates all required fields, prevents overwriting built-in theme IDs, and registers the theme on success. Returns `null` on validation failure.

```typescript
import { importThemeFromJSON, setTheme } from "../themes";

const theme = importThemeFromJSON(jsonString);
if (theme) {
  setTheme(theme.id); // Activate the imported theme
}
```

### `exportThemeAsJSON(id: ThemeId): string`

Export any theme (built-in or custom) as a formatted JSON string suitable for file sharing. The `is_builtin` field is stripped from the export.

```typescript
import { exportThemeAsJSON } from "../themes";

const json = exportThemeAsJSON("dark");
// Copy to clipboard or save to file
```

### `setTheme(id: ThemeId): void`

Switch to a theme by ID. Persists the choice to `localStorage` and applies all CSS variables to `:root` immediately.

```typescript
import { setTheme } from "../themes";
setTheme("ocean-breeze");
```

### `getTheme(): BentoTheme`

Get the currently active theme object (reactive -- components using this will re-render on theme change).

### `getThemeId(): ThemeId`

Get the currently active theme ID (reactive).

### `getAvailableThemes(): readonly BentoTheme[]`

Get all available themes (built-in + custom) in display order (reactive).

### `removeCustomTheme(id: ThemeId): boolean`

Remove a custom theme by ID. Built-in themes cannot be removed. If the removed theme is currently active, falls back to `"dark"`. Returns `false` if the ID is a built-in theme.

### `applyCurrentTheme(): void`

Apply the saved theme on startup. Call once during app initialization.

## How Themes Are Applied

When a theme is activated via `setTheme()`:

1. The theme ID is saved to `localStorage` under key `bentodesk-theme-id`
2. Each field in the `BentoTheme` object is mapped to a CSS variable name:
   - `surface_zen` -> `--surface-zen`
   - `text_primary` -> `--text-primary`
   - etc. (27 CSS variables total)
3. All CSS variables are set on `document.documentElement.style` (`:root`)
4. The `data-theme` attribute is set to `"light"` or `"dark"` for backward compatibility

Custom themes are persisted to `localStorage` under key `bentodesk-custom-themes` as a JSON array.

## CSS Variable Reference

Use these CSS variables in your stylesheets:

```css
.my-element {
  /* Surfaces */
  background: var(--surface-zen);
  background: var(--surface-expanded);
  background: var(--surface-hover);
  background: var(--surface-active);
  background: var(--surface-subtle);

  /* Borders */
  border-color: var(--border-zen);
  border-color: var(--border-expanded);
  border-color: var(--border-hover);

  /* Text */
  color: var(--text-primary);
  color: var(--text-secondary);
  color: var(--text-muted);

  /* Accents */
  color: var(--accent-blue);
  color: var(--accent-purple);
  color: var(--accent-green);
  color: var(--accent-orange);
  color: var(--accent-pink);
  color: var(--accent-red);

  /* Shadows */
  box-shadow: var(--shadow-zen);
  box-shadow: var(--shadow-expanded);
  box-shadow: var(--shadow-item-hover);

  /* Blur */
  backdrop-filter: var(--blur-zen);
  backdrop-filter: var(--blur-expanded);

  /* Badge */
  background: var(--badge-bg);

  /* Radii */
  border-radius: var(--radius-capsule);
  border-radius: var(--radius-expanded);
  border-radius: var(--radius-card);
  border-radius: var(--radius-badge);
}
```

## Design Tips

### Glassmorphism Themes

- Use `rgba()` with low alpha (0.4-0.7) for `surface_zen` and `surface_expanded`
- Set `blur_zen` and `blur_expanded` to `blur(20px) saturate(160%)` or higher
- Keep borders subtle (alpha 0.1-0.2) for the frosted glass effect

### Opaque Themes

- Use `rgba()` with alpha 1.0 for surfaces (e.g. `rgba(30, 30, 46, 1)`)
- Set blur values to `blur(0px) saturate(100%)` or `none`
- Use stronger border colors for visual separation

### Neomorphism Themes

- Use dual shadows (one light, one dark) for the extruded effect
- Set borders to `transparent`
- Use a neutral, slightly colored background

### Flat Themes

- Set all shadows to `"none"`
- Use small border radii (4px) for sharp corners
- Use bold, saturated accent colors
- Disable blur with `"none"`

## Validation Rules

When importing a theme via `importThemeFromJSON()`:

1. `id` must be a non-empty string
2. All 30 fields (excluding `is_builtin`) must be present in the JSON
3. The `id` must not match any built-in theme ID (`dark`, `light`, `midnight`, `forest`, `sunset`, `frosted`, `solid`, `order`, `neo`, `flat`)
4. `is_builtin` is always set to `false` regardless of the input value
