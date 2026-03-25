# BentoDesk Documentation

Developer documentation for BentoDesk -- a bento-box style Windows desktop organizer.

## Guides

| Document | Description |
|----------|-------------|
| [Architecture](./architecture.md) | High-level technical architecture, module map, IPC commands, and data flow |
| [Theme API](./theme-api.md) | Complete theme development guide: BentoTheme interface, CSS variables, custom theme creation, import/export |
| [Zone API](./zone-api.md) | Zone and item data model: BentoZone, BentoItem, capsule shapes, grid layout |
| [i18n Guide](./i18n-guide.md) | Internationalization: adding languages, translation keys, t() function, persistence |

## Quick Links

- **Source code**: `bentodesk/src/` (frontend), `bentodesk/src-tauri/src/` (backend)
- **Theme presets**: `src/themes/presets.ts` (10 built-in themes)
- **Locale files**: `src/i18n/locales/zh-CN.ts`, `src/i18n/locales/en.ts`
- **IPC wrappers**: `src/services/ipc.ts` (all Tauri invoke commands)
- **Zone types**: `src/types/zone.ts` (TypeScript data model)

## Getting Started

```bash
# Install dependencies
pnpm install

# Run in development mode
pnpm tauri dev

# Build for production
pnpm tauri build
```

See the project [README](../README.md) for full installation and usage instructions.
