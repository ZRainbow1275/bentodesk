# IconPicker — Visual Spec

Source: `bentodesk/src/components/IconPicker/IconPicker.tsx` (301 LOC) + `IconPicker.css`.

- **Window:** separate `WindowKind::IconPicker` HWND, 480 × 640 px default, 16 px corner radius (DComp visual clip), `palette.surface` background, 1 px `palette.border`, scale-in 200 ms cubic-bezier(0.16, 1, 0.3, 1).
- **Toolbar (top):** 48 px tall, padding 12 px. Layout: search Input (flex-grow, 32 px tall, 6 px radius, `palette.surface_alt` bg, 1 px border, autofocus) + Upload button (96 px wide, secondary style).
- **Tab strip:** 36 px tall, horizontal scroll if overflow, `palette.surface` background, 1 px `palette.border` bottom. Each tab 28 px tall × auto-width, 6 px corner radius, padding 4 px 12 px, 12 px Medium text. Active tab: `palette.accent` 12% opacity background + `palette.accent` text + 2 px `palette.accent` underline.
- **Grid:** CSS-grid 6 columns × N rows (auto-rows 56 px), 8 px gap, padding 12 px, scrollable.
- **Cell:** 56×56 px, 8 px corner radius, `palette.surface` bg, 1 px transparent border. Hover: `palette.hover_overlay` bg + 1 px `palette.border`. Selected: 2 px `palette.accent` border + `palette.accent` 8% bg.
- **Cell icon:** 24×24 px centered, `palette.text` tint for Lucide/builtin, raw RGBA for custom.
- **Custom icon delete:** 14×14 px floating "×" badge top-right, `palette.danger` bg, white text, only on hover.
- **Empty state:** 13 px italic `palette.text_muted` centered, "No icons found".
- **Hint:** 11 px `palette.text_muted` below grid when capped at 200 results, "Refine your search to see more".
- **Virtualization:** IntersectionObserver pattern → port as VirtualGrid (renders only visible cells + 200 px buffer rootMargin).
